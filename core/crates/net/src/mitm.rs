//! CONNECT handling: decrypt with a minted certificate, or tunnel.

use crate::flowctx::{AppSlot, FlowCtx, NewFlow};
use crate::handler::{empty, text_response, unix_ms, ConnCtx, ConnKind};
use crate::parse::parse_connect_authority;
use crate::{connector, Inner, ProxyBody};
use bytes::Bytes;
use flow::flags;
use http::{Request, Response, StatusCode};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use rustls::AlertDescription;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::LazyConfigAcceptor;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Replays bytes that were read ahead before handing the stream on.
pub(crate) struct Rewind<T> {
    prefix: Option<Bytes>,
    inner: T,
}

impl<T> Rewind<T> {
    pub fn new(inner: T, prefix: Bytes) -> Self {
        Rewind {
            prefix: Some(prefix).filter(|p| !p.is_empty()),
            inner,
        }
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for Rewind<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if let Some(mut prefix) = self.prefix.take() {
            let n = prefix.len().min(buf.remaining());
            buf.put_slice(&prefix.split_to(n));
            if !prefix.is_empty() {
                self.prefix = Some(prefix);
            }
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for Rewind<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// Why a client turned our certificate down, if that is what happened.
/// These alerts are how pinned apps and clients without the CA say no.
pub(crate) fn certificate_rejection(e: &io::Error) -> Option<&'static str> {
    let re = e.get_ref()?.downcast_ref::<rustls::Error>()?;
    match re {
        rustls::Error::AlertReceived(d) => match d {
            AlertDescription::UnknownCA => Some("unknown_ca"),
            AlertDescription::BadCertificate => Some("bad_certificate"),
            AlertDescription::CertificateUnknown => Some("certificate_unknown"),
            AlertDescription::UnsupportedCertificate => Some("unsupported_certificate"),
            AlertDescription::CertificateExpired => Some("certificate_expired"),
            _ => None,
        },
        _ => None,
    }
}

/// Look through an error chain for a TLS alert or a reset that follows a
/// finished handshake with no traffic.
fn rejection_in_chain(e: &(dyn std::error::Error + 'static)) -> Option<&'static str> {
    let mut cur: Option<&(dyn std::error::Error + 'static)> = Some(e);
    while let Some(err) = cur {
        if let Some(io) = err.downcast_ref::<io::Error>() {
            if let Some(reason) = certificate_rejection(io) {
                return Some(reason);
            }
            if matches!(
                io.kind(),
                io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::BrokenPipe
                    | io::ErrorKind::UnexpectedEof
            ) {
                return Some("closed_after_handshake");
            }
        }
        cur = err.source();
    }
    None
}

impl Inner {
    pub(crate) async fn serve_plain(self: Arc<Self>, stream: TcpStream, peer: SocketAddr) {
        let _ = stream.set_nodelay(true);
        self.serve_io(stream, peer).await;
    }

    pub(crate) async fn serve_io<T>(self: Arc<Self>, io: T, peer: SocketAddr)
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let app = if self.recording.load(std::sync::atomic::Ordering::Relaxed) {
            AppSlot::lookup(
                self.apps.clone(),
                peer.ip(),
                peer.port(),
                self.cfg.listen.port(),
            )
        } else {
            AppSlot::none()
        };
        let ctx = Arc::new(ConnCtx {
            kind: ConnKind::Plain,
            peer,
            app,
            requests: Default::default(),
        });
        let inner = self.clone();
        let svc = service_fn(move |req: Request<Incoming>| {
            let inner = inner.clone();
            let ctx = ctx.clone();
            async move { inner.handle(req, ctx).await }
        });
        let conn = hyper::server::conn::http1::Builder::new()
            .preserve_header_case(true)
            .serve_connection(TokioIo::new(io), svc)
            .with_upgrades();
        if let Err(e) = conn.await {
            tracing::debug!("connection from {peer} ended: {e}");
        }
    }

    pub(crate) fn handle_connect(
        self: &Arc<Self>,
        mut req: Request<Incoming>,
        ctx: &ConnCtx,
    ) -> Response<ProxyBody> {
        let parsed = req
            .uri()
            .authority()
            .and_then(|a| parse_connect_authority(a.as_str()));
        let Some((host, port)) = parsed else {
            return text_response(
                StatusCode::BAD_REQUEST,
                "Bad CONNECT request",
                "CONNECT needs a host and port.",
            );
        };
        let on_upgrade = hyper::upgrade::on(&mut req);
        let me = self.clone();
        let app = ctx.app.clone();
        let peer = ctx.peer;
        tokio::spawn(async move {
            match on_upgrade.await {
                Ok(upgraded) => {
                    me.after_connect(TokioIo::new(upgraded), host, port, app, peer)
                        .await
                }
                Err(e) => tracing::debug!("CONNECT upgrade failed: {e}"),
            }
        });
        Response::new(empty())
    }

    async fn after_connect<T>(
        self: Arc<Self>,
        mut io: T,
        host: String,
        port: u16,
        app: Arc<AppSlot>,
        peer: SocketAddr,
    ) where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        // The proxy's own page is always decrypted: it is how a device finds
        // out whether it trusts the CA yet.
        let local = host == crate::ca_page::LOCAL_HOST;
        if !local && self.pinned.lock().unwrap().contains(&host) {
            self.passthrough(io, host, port, flags::PINNED | flags::TUNNEL, app)
                .await;
            return;
        }
        // The person's list: hosts outside it are tunneled without a handshake.
        let policy = self.policy.lock().unwrap().clone();
        if let Some(p) = policy.filter(|_| !local) {
            if !p.should_decrypt(&host, port) {
                self.passthrough(io, host, port, flags::TUNNEL | flags::SSL_BYPASSED, app)
                    .await;
                return;
            }
        }
        let mut first = [0u8; 1];
        let n = match tokio::time::timeout(HANDSHAKE_TIMEOUT, io.read(&mut first)).await {
            Ok(Ok(n)) => n,
            _ => return,
        };
        if n == 0 {
            return;
        }
        let rewound = Rewind::new(io, Bytes::copy_from_slice(&first));
        if first[0] != 0x16 {
            // Not TLS: tunnel it untouched.
            self.passthrough(rewound, host, port, flags::TUNNEL, app)
                .await;
            return;
        }
        self.intercept(rewound, host, port, app, peer).await;
    }

    async fn intercept<T>(
        self: Arc<Self>,
        io: T,
        host: String,
        port: u16,
        app: Arc<AppSlot>,
        peer: SocketAddr,
    ) where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let accepted = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
            let start = LazyConfigAcceptor::new(rustls::server::Acceptor::default(), io).await?;
            let sni = start
                .client_hello()
                .server_name()
                .map(|s| s.to_ascii_lowercase());
            let name = sni.unwrap_or_else(|| host.clone());
            let cfg = self
                .ca
                .server_config(&name)
                .map_err(|e| io::Error::other(e.to_string()))?;
            start.into_stream(cfg).await
        })
        .await;

        let tls = match accepted {
            Ok(Ok(tls)) => tls,
            Ok(Err(e)) => {
                if let Some(reason) = certificate_rejection(&e) {
                    self.mark_pinned(&host, port, reason, &app);
                } else {
                    tracing::debug!("TLS accept for {host}:{port} failed: {e}");
                }
                return;
            }
            Err(_) => return,
        };

        let ctx = Arc::new(ConnCtx {
            kind: ConnKind::Tunnel {
                host: host.clone(),
                port,
            },
            peer,
            app: app.clone(),
            requests: Default::default(),
        });
        let handshake_done = Instant::now();
        let seen = ctx.clone();
        let inner = self.clone();
        let svc = service_fn(move |req: Request<Incoming>| {
            let inner = inner.clone();
            let ctx = ctx.clone();
            async move { inner.handle(req, ctx).await }
        });
        let mut builder = auto::Builder::new(TokioExecutor::new());
        builder.http1().preserve_header_case(true);
        if let Err(e) = builder
            .serve_connection_with_upgrades(TokioIo::new(tls), svc)
            .await
        {
            tracing::debug!("tunnel {host}:{port} ended: {e}");
            // Some clients (curl with OpenSSL among them) finish the handshake
            // and only then verify the chain. They drop the connection before
            // sending a single request. Treat a prompt reset like a rejection.
            let early = seen.requests.load(std::sync::atomic::Ordering::Relaxed) == 0
                && handshake_done.elapsed() < Duration::from_secs(2);
            if early {
                if let Some(reason) = rejection_in_chain(&*e) {
                    self.mark_pinned(&host, port, reason, &app);
                }
            }
        }
    }

    /// A client refused the minted certificate. Remember the host so the next
    /// CONNECT is tunneled, and leave a flow behind so it shows in the list.
    fn mark_pinned(&self, host: &str, port: u16, reason: &str, app: &Arc<AppSlot>) {
        // Refusing the proxy's own page just means the CA is not installed yet.
        if host == crate::ca_page::LOCAL_HOST {
            return;
        }
        self.pinned.lock().unwrap().insert(host.to_string());
        tracing::info!("{host}:{port} rejected the certificate ({reason}); tunneling from now on");
        if !self.recording.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let f = FlowCtx::start(
            self.sink.clone(),
            NewFlow {
                id: self.ids.next(),
                started_at_ms: unix_ms(),
                scheme: "https",
                host: host.to_string(),
                port,
                method: "CONNECT".into(),
                path: String::new(),
                query: String::new(),
                req_headers: Vec::new(),
                flags: flags::PINNED | flags::TUNNEL,
                protocol: "tunnel",
            },
            app.clone(),
            0,
        );
        f.fail(format!(
            "The client rejected the certificate ({reason}). The app pins its certificate, \
             or the proxy CA is not trusted. This host is tunneled from now on."
        ));
    }

    /// Copy bytes both ways without looking at them.
    async fn passthrough<T>(
        self: Arc<Self>,
        mut client: T,
        host: String,
        port: u16,
        extra_flags: u32,
        app: Arc<AppSlot>,
    ) where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let flow = if self.recording.load(std::sync::atomic::Ordering::Relaxed) {
            Some(FlowCtx::start(
                self.sink.clone(),
                NewFlow {
                    id: self.ids.next(),
                    started_at_ms: unix_ms(),
                    scheme: "https",
                    host: host.clone(),
                    port,
                    method: "CONNECT".into(),
                    path: String::new(),
                    query: String::new(),
                    req_headers: Vec::new(),
                    flags: extra_flags,
                    protocol: "tunnel",
                },
                app,
                0,
            ))
        } else {
            None
        };
        if self.cfg.offline {
            if let Some(f) = flow {
                f.fail("upstream connections are disabled".into());
            }
            return;
        }
        let (mut up, info) = match connector::connect(&host, port, None).await {
            Ok(v) => v,
            Err(e) => {
                if let Some(f) = flow {
                    f.fail(format!("Could not reach {host}:{port}: {e}"));
                }
                return;
            }
        };
        if let Some(f) = &flow {
            f.response_headers(200, "tunnel", Vec::new(), info.claim().then_some(&*info), 0);
        }
        let result = tokio::io::copy_bidirectional(&mut client, &mut up).await;
        if let Some(f) = flow {
            match result {
                Ok((a, b)) => f.finish_stream(a, b, None),
                Err(e) => f.finish_stream(0, 0, Some(format!("Tunnel ended: {e}"))),
            }
        }
    }
}
