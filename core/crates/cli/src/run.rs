use crate::paths;
use anyhow::{Context, Result};
use flow::{FlowEvent, FlowSink, IdGen, InterceptorChain};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tls::{CaStore, KeyStorage};

#[derive(clap::Args, Debug)]
pub struct RunArgs {
    /// Address the proxy listens on.
    #[arg(long, default_value = "127.0.0.1:8899")]
    pub listen: SocketAddr,
    /// Data directory (CA, database, blobs).
    #[arg(long)]
    pub data_dir: Option<PathBuf>,
    /// API socket path.
    #[arg(long)]
    pub socket: Option<PathBuf>,
    /// Do not verify upstream certificates.
    #[arg(long)]
    pub insecure_upstream: bool,
    /// Start with recording paused.
    #[arg(long)]
    pub no_record: bool,
    /// Where the CA private key lives.
    #[arg(long, value_enum, default_value = "auto")]
    pub key_storage: KeyStorageArg,
    /// Per-direction body capture cap, in MiB.
    #[arg(long, default_value_t = 8)]
    pub body_cap_mb: usize,
    /// Address of the MCP server for local AI clients (loopback only).
    #[arg(long, default_value = mcp::DEFAULT_ADDR)]
    pub mcp_listen: SocketAddr,
    /// Do not start the MCP server.
    #[arg(long)]
    pub no_mcp: bool,
    /// Port the in-app SDK listener uses while it is turned on.
    #[arg(long, default_value_t = sdk::DEFAULT_PORT)]
    pub sdk_port: u16,
    /// Accept SDK connections for this run even if they were turned off in
    /// settings. They are on by default.
    #[arg(long)]
    pub sdk: bool,
    /// Never start the SDK listener.
    #[arg(long, conflicts_with = "sdk")]
    pub no_sdk: bool,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub enum KeyStorageArg {
    Auto,
    File,
    Keyring,
}

impl From<KeyStorageArg> for KeyStorage {
    fn from(k: KeyStorageArg) -> Self {
        match k {
            KeyStorageArg::Auto => KeyStorage::Auto,
            KeyStorageArg::File => KeyStorage::File,
            KeyStorageArg::Keyring => KeyStorage::Keyring,
        }
    }
}

/// Sends every event to the hub (live subscribers) and the store (disk).
struct Fanout {
    hub: api::Hub,
    store: store::Store,
}

impl FlowSink for Fanout {
    fn publish(&self, event: FlowEvent) {
        if matches!(event, FlowEvent::Finished(_)) {
            self.store.publish(event.clone());
        }
        self.hub.publish(event);
    }
}

/// The proxy's sink: the usual fan-out, plus a tap that offers finished proxy
/// flows to the SDK merger.
struct ProxySink {
    inner: Fanout,
    sdk: Option<Arc<sdk::Sdk>>,
}

impl FlowSink for ProxySink {
    fn publish(&self, event: FlowEvent) {
        if let (Some(sdk), FlowEvent::Finished(rec)) = (&self.sdk, &event) {
            sdk.observe_proxy_flow(rec);
        }
        self.inner.publish(event);
    }
}

struct ProxyControl(net::Proxy);

impl api::Control for ProxyControl {
    fn recording(&self) -> bool {
        self.0.recording()
    }
    fn set_recording(&self, on: bool) {
        self.0.set_recording(on)
    }
    fn pinned_hosts(&self) -> Vec<String> {
        self.0.pinned_hosts()
    }
    fn clear_pinned(&self) {
        self.0.clear_pinned()
    }
    fn unpin_host(&self, host: &str) -> bool {
        self.0.unpin(host)
    }
    fn replay(&self, call: api::ReplayCall) -> flow::BoxFuture<'_, Result<(u64, u16), String>> {
        Box::pin(async move {
            let out = self
                .0
                .replay(net::ReplayRequest {
                    method: call.method,
                    url: call.url,
                    headers: call.headers,
                    body: call.body,
                })
                .await?;
            Ok((out.flow_id, out.status))
        })
    }
}

pub async fn run(args: RunArgs, started: Instant) -> Result<()> {
    let data_dir = args.data_dir.clone().unwrap_or_else(paths::data_dir);
    let socket = args.socket.clone().unwrap_or_else(paths::socket_path);

    let ca = Arc::new(
        CaStore::load_or_create(&data_dir, args.key_storage.into()).context("loading the CA")?,
    );
    let store = store::Store::open(&data_dir).context("opening the store")?;
    let ids = Arc::new(IdGen::starting_after(store.last_id().unwrap_or(0)));
    let hub = api::Hub::new();
    let fanout = Fanout {
        hub: hub.clone(),
        store: store.clone(),
    };

    // The in-app SDK listener. Created turned off: it accepts connections on
    // the LAN only once someone enables it.
    let sdk = if args.no_sdk {
        None
    } else {
        let mut opts = sdk::SdkOptions::new(data_dir.clone());
        opts.port = args.sdk_port;
        let sdk_sink = Arc::new(Fanout {
            hub: hub.clone(),
            store: store.clone(),
        });
        match sdk::Sdk::new(opts, sdk_sink, store.clone(), ids.clone()) {
            Ok(s) => Some(Arc::new(s)),
            Err(e) => {
                tracing::warn!("SDK listener unavailable: {e}");
                None
            }
        }
    };
    // The listener follows the remembered setting (on by default). `--sdk` forces it on for this run.
    if let Some(s) = &sdk {
        let started = if args.sdk { s.set_enabled(true).await } else { s.start_saved().await };
        if let Err(e) = started {
            tracing::warn!("could not start the SDK listener on port {}: {e}", args.sdk_port);
        }
    }
    let sink = Arc::new(ProxySink {
        inner: fanout,
        sdk: sdk.clone(),
    });

    let mut cfg = net::ProxyConfig::new(args.listen);
    cfg.insecure_upstream = args.insecure_upstream;
    cfg.body_cap = args.body_cap_mb.max(1) * 1024 * 1024;
    // The person's rules: mocks, remaps, rewrites and Map Local answer or
    // change requests through the interceptor seam, breakpoints hold them at
    // the gate, and the SSL list decides which CONNECTs are decrypted.
    let rules = Arc::new(rules::RuleSet::load(&data_dir));
    let mut chain = InterceptorChain::empty();
    chain.push_request(rules.clone());
    chain.push_response(rules.clone());
    let proxy = net::Proxy::new(cfg, ca.clone(), sink, ids, chain);
    proxy.set_connect_policy(rules.clone());
    proxy.set_gate(rules.clone());
    proxy.set_recording(!args.no_record);

    let listener = tokio::net::TcpListener::bind(args.listen)
        .await
        .with_context(|| format!("binding {}", args.listen))?;
    let listen = listener.local_addr()?;

    // The MCP server for local AI clients. A port that is taken is not fatal:
    // the proxy and the app work without it.
    let mut mcp_listener = None;
    let mut mcp_endpoint = None;
    let mut mcp_token = String::new();
    if !args.no_mcp {
        match mcp::load_or_create_token(&data_dir) {
            Ok(token) => match mcp::bind(args.mcp_listen).await {
                Ok(l) => {
                    let addr = l.local_addr()?;
                    mcp_endpoint = Some(api::McpEndpoint {
                        url: format!("http://{addr}/mcp"),
                        token: token.clone(),
                    });
                    mcp_token = token;
                    mcp_listener = Some(l);
                }
                Err(e) => tracing::warn!("MCP server not started on {}: {e}", args.mcp_listen),
            },
            Err(e) => tracing::warn!("MCP server not started: could not make a token: {e}"),
        }
    }

    let ctx = Arc::new(api::ApiContext {
        store: store.clone(),
        hub,
        control: Arc::new(ProxyControl(proxy.clone())),
        rules: rules.clone(),
        composer: Arc::new(api::compose::Composer::load(&data_dir)),
        approvals: Arc::new(api::Approvals::default()),
        mcp: mcp_endpoint.clone(),
        sdk: sdk.clone(),
        info: api::CoreInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            listen: listen.to_string(),
            data_dir: data_dir.display().to_string(),
            insecure_upstream: args.insecure_upstream,
            ca_path: ca.cert_path().display().to_string(),
            ca_sha256: ca.sha256().to_string(),
            ca_subject: ca.subject().to_string(),
            ca_not_after: ca.not_after().to_string(),
            ca_pem: ca.cert_pem().to_string(),
            ca_spki_sha256: ca.spki_sha256().to_string(),
            ca_key_storage: ca.key_storage().as_str().to_string(),
            started,
        },
    });

    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let proxy_task = {
        let proxy = proxy.clone();
        let mut rx = stop_rx.clone();
        tokio::spawn(async move {
            proxy
                .serve(listener, async move {
                    let _ = rx.wait_for(|v| *v).await;
                })
                .await
        })
    };
    let mcp_task = mcp_listener.map(|listener| {
        let mut rx = stop_rx.clone();
        let ctx = ctx.clone();
        let token = mcp_token.clone();
        tokio::spawn(async move {
            mcp::serve(ctx, listener, token, async move {
                let _ = rx.wait_for(|v| *v).await;
            })
            .await
        })
    });
    let api_task = {
        let mut rx = stop_rx.clone();
        let socket = socket.clone();
        tokio::spawn(async move {
            api::serve(ctx, &socket, async move {
                let _ = rx.wait_for(|v| *v).await;
            })
            .await
        })
    };

    // Give the API a moment to fail fast (socket owned by another core).
    tokio::time::sleep(std::time::Duration::from_millis(15)).await;
    if api_task.is_finished() {
        return match api_task.await? {
            Err(e) => Err(e.into()),
            Ok(()) => Ok(()),
        };
    }

    eprintln!(
        "nekod {} ready in {} ms\n  proxy   {}\n  api     {}\n  mcp     {}\n  sdk     {}\n  ca      {} ({})\n  data    {}\n  record  {}",
        env!("CARGO_PKG_VERSION"),
        started.elapsed().as_millis(),
        listen,
        socket.display(),
        mcp_endpoint
            .as_ref()
            .map(|m| m.url.as_str())
            .unwrap_or("off"),
        match &sdk {
            None => "unavailable".to_string(),
            Some(s) if s.is_enabled() => format!("listening on port {}", s.local_port().unwrap_or(args.sdk_port)),
            Some(_) => format!("off (turned off in settings; port {})", args.sdk_port),
        },
        ca.cert_path().display(),
        ca.key_storage().as_str(),
        data_dir.display(),
        if args.no_record { "paused" } else { "on" },
    );

    wait_for_signal().await;
    tracing::info!("shutting down");
    let _ = stop_tx.send(true);
    if let Some(s) = &sdk {
        // Stop listening, but keep the remembered setting: on again at the next start.
        let _ = s.shutdown().await;
    }
    let _ = proxy_task.await;
    let _ = api_task.await;
    if let Some(t) = mcp_task {
        let _ = t.await;
    }
    store.close();
    Ok(())
}

async fn wait_for_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
    let mut int = signal(SignalKind::interrupt()).expect("SIGINT handler");
    tokio::select! {
        _ = term.recv() => {}
        _ = int.recv() => {}
    }
}
