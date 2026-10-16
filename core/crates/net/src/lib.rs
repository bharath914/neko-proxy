//! The data path: listener, CONNECT handling, TLS interception, HTTP/1.1 and
//! HTTP/2 forwarding with timings, pinning detection and tunnel fallback.
//!
//! Nothing here waits on the UI or the disk. Flow events go to a
//! [`flow::FlowSink`] that must not block.

mod body;
mod ca_page;
mod connector;
mod flowctx;
mod handler;
mod mitm;
pub mod parse;
mod procinfo;

use body::BoxError;
use bytes::Bytes;
use connector::{Connector, UpstreamTls};
use flow::{ConnectPolicy, FlowSink, Gate, IdGen, InterceptorChain};
use http_body_util::combinators::UnsyncBoxBody;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use std::collections::BTreeSet;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tls::CaStore;
use tokio::net::TcpListener;

pub(crate) type ProxyBody = UnsyncBoxBody<Bytes, BoxError>;

#[derive(Clone, Debug)]
pub struct ProxyConfig {
    /// Address the proxy listens on. Used to recognize requests to the proxy
    /// itself and to find the client process behind a connection.
    pub listen: SocketAddr,
    /// Skip upstream certificate verification.
    pub insecure_upstream: bool,
    /// Per-direction capture cap for each flow, in bytes.
    pub body_cap: usize,
    /// Refuse every upstream connection. For fuzzing and offline tests.
    pub offline: bool,
}

impl ProxyConfig {
    pub fn new(listen: SocketAddr) -> Self {
        ProxyConfig {
            listen,
            insecure_upstream: false,
            body_cap: 8 * 1024 * 1024,
            offline: false,
        }
    }
}

pub(crate) struct Inner {
    pub cfg: ProxyConfig,
    pub ca: Arc<CaStore>,
    pub sink: Arc<dyn FlowSink>,
    pub ids: Arc<IdGen>,
    pub chain: InterceptorChain,
    pub recording: AtomicBool,
    pub pinned: Mutex<BTreeSet<String>>,
    /// The person's own decrypt-or-tunnel choice, set after construction.
    pub policy: Mutex<Option<Arc<dyn ConnectPolicy>>>,
    /// Holds messages at breakpoints, set after construction.
    pub gate: Mutex<Option<Arc<dyn Gate>>>,
    pub client: Client<Connector, ProxyBody>,
    pub upstream_tls: Arc<UpstreamTls>,
    pub apps: Arc<flowctx::AppResolverHandle>,
}

/// A request for the proxy to send again, usually a captured flow with edits.
#[derive(Clone, Debug)]
pub struct ReplayRequest {
    pub method: String,
    /// Absolute `http://` or `https://` URL.
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
}

#[derive(Clone, Debug)]
pub struct ReplayOutcome {
    pub flow_id: u64,
    pub status: u16,
}

/// A running proxy. Cheap to clone.
#[derive(Clone)]
pub struct Proxy {
    inner: Arc<Inner>,
}

impl Proxy {
    pub fn new(
        cfg: ProxyConfig,
        ca: Arc<CaStore>,
        sink: Arc<dyn FlowSink>,
        ids: Arc<IdGen>,
        chain: InterceptorChain,
    ) -> Proxy {
        let upstream_tls = UpstreamTls::new(cfg.insecure_upstream);
        let client = Client::builder(TokioExecutor::new())
            .pool_timer(TokioTimer::new())
            .pool_idle_timeout(Duration::from_secs(60))
            .pool_max_idle_per_host(32)
            .build(Connector::new(upstream_tls.clone(), cfg.offline));
        Proxy {
            inner: Arc::new(Inner {
                cfg,
                ca,
                sink,
                ids,
                chain,
                recording: AtomicBool::new(true),
                pinned: Mutex::new(BTreeSet::new()),
                policy: Mutex::new(None),
                gate: Mutex::new(None),
                client,
                upstream_tls,
                apps: Arc::new(flowctx::AppResolverHandle(procinfo::AppResolver::default())),
            }),
        }
    }

    /// Pause or resume recording. Traffic keeps flowing while paused; it just
    /// does not become flows.
    pub fn set_recording(&self, on: bool) {
        self.inner.recording.store(on, Ordering::Relaxed);
    }

    pub fn recording(&self) -> bool {
        self.inner.recording.load(Ordering::Relaxed)
    }

    /// Hosts whose clients rejected the minted certificate. They are tunneled.
    pub fn pinned_hosts(&self) -> Vec<String> {
        self.inner.pinned.lock().unwrap().iter().cloned().collect()
    }

    pub fn clear_pinned(&self) {
        self.inner.pinned.lock().unwrap().clear();
    }

    /// Give one host another chance at being decrypted.
    pub fn unpin(&self, host: &str) -> bool {
        self.inner.pinned.lock().unwrap().remove(host)
    }

    /// Decide per CONNECT whether TLS is decrypted. Without a policy every
    /// host is decrypted.
    pub fn set_connect_policy(&self, policy: Arc<dyn ConnectPolicy>) {
        *self.inner.policy.lock().unwrap() = Some(policy);
    }

    /// Send a request again as a new flow. It goes straight upstream: rules and
    /// breakpoints do not apply, so a replay shows what the server really says.
    pub async fn replay(&self, req: ReplayRequest) -> Result<ReplayOutcome, String> {
        self.inner.replay(req).await
    }

    /// Let breakpoints hold requests and responses. Without a gate nothing is
    /// ever held.
    pub fn set_gate(&self, gate: Arc<dyn Gate>) {
        *self.inner.gate.lock().unwrap() = Some(gate);
    }

    pub fn config(&self) -> &ProxyConfig {
        &self.inner.cfg
    }

    /// Serve one already-accepted connection to completion. Used by tests and
    /// the fuzz target to drive the parser with arbitrary bytes.
    pub async fn serve_io<T>(&self, io: T, peer: SocketAddr)
    where
        T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        self.inner.clone().serve_io(io, peer).await;
    }

    /// Accept connections until `shutdown` resolves.
    pub async fn serve(&self, listener: TcpListener, shutdown: impl Future<Output = ()>) {
        self.inner.upstream_tls.warm();
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                accepted = listener.accept() => match accepted {
                    Ok((stream, peer)) => {
                        let inner = self.inner.clone();
                        tokio::spawn(async move { inner.serve_plain(stream, peer).await });
                    }
                    Err(e) => {
                        tracing::warn!("accept failed: {e}");
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                },
                _ = &mut shutdown => break,
            }
        }
    }
}
