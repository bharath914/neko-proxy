//! A real ingest server without the proxy or the app, for SDK integration
//! tests and CI.
//!
//!   cargo run -p sdk --bin sdk-test-core -- --port 8897 --record out.jsonl --auto-allow
//!
//! Prints one JSON line to stdout when ready: {"event":"ready","port":..,"fingerprint":".."}.

use clap::Parser;
use flow::{FlowEvent, FlowSink, IdGen};
use sdk::{DeviceConfig, Identity, PromptPolicy, Sdk, SdkOptions, Tap};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Parser)]
#[command(name = "sdk-test-core", about = "Ingest server for SDK tests")]
struct Args {
    /// Ingest port (0 picks a free one).
    #[arg(long, default_value_t = sdk::DEFAULT_PORT)]
    port: u16,
    /// Address to listen on.
    #[arg(long, default_value = "127.0.0.1")]
    bind: std::net::IpAddr,
    /// Append every received message and assembled flow as JSON lines.
    #[arg(long)]
    record: Option<PathBuf>,
    /// Approve every pairing request and print its code on stderr.
    #[arg(long)]
    auto_allow: bool,
    /// Deny every pairing request.
    #[arg(long, conflicts_with = "auto_allow")]
    deny: bool,
    /// Push this Config (JSON) to every device.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Ask for lazy bodies as soon as a call finishes.
    #[arg(long)]
    fetch_lazy: bool,
    /// Revoke a device after this many finished calls.
    #[arg(long)]
    revoke_after: Option<u64>,
    /// Where state lives (default: a fresh temporary directory).
    #[arg(long)]
    data_dir: Option<PathBuf>,
    /// Accept connections from this computer without a prompt (the product default). Off here, so --deny and the
    /// prompt paths can be tested.
    #[arg(long)]
    auto_accept_local: bool,
    /// Advertise over mDNS.
    #[arg(long)]
    advertise: bool,
}

struct Recorder(Mutex<std::fs::File>);

impl Recorder {
    fn line(&self, v: serde_json::Value) {
        let mut f = self.0.lock().unwrap();
        let _ = writeln!(f, "{v}");
        let _ = f.flush();
    }
}

struct RecTap(Arc<Recorder>);

impl Tap for RecTap {
    fn received(&self, who: &Identity, msg: &sdk::wire::envelope::Msg) {
        self.0.line(sdk::record::message(who, msg));
    }
    fn event(&self, name: &str, mut detail: serde_json::Value) {
        detail["type"] = name.into();
        self.0.line(detail);
    }
}

struct Sink {
    store: store::Store,
    rec: Option<Arc<Recorder>>,
}

impl FlowSink for Sink {
    fn publish(&self, e: FlowEvent) {
        if let FlowEvent::Finished(r) = e {
            if let Some(rec) = &self.rec {
                rec.line(sdk::record::flow(&r));
            }
            self.store.enqueue(r);
        }
    }
}

#[tokio::main]
async fn main() -> anyhow_free::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let a = Args::parse();
    let dir = match &a.data_dir {
        Some(d) => d.clone(),
        None => std::env::temp_dir().join(format!("sdk-test-core-{}", std::process::id())),
    };
    let store = store::Store::open(&dir.join("store"))?;
    let rec = match &a.record {
        Some(p) => Some(Arc::new(Recorder(Mutex::new(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)?,
        )))),
        None => None,
    };
    let mut opts = SdkOptions::new(dir.join("sdk"));
    opts.port = a.port;
    opts.bind = a.bind;
    opts.advertise = a.advertise;
    opts.watch_adb = false;
    opts.desktop_name = "sdk-test-core".into();
    opts.prompt_policy = if a.auto_allow {
        PromptPolicy::AutoAllow
    } else if a.deny {
        PromptPolicy::AutoDeny
    } else {
        PromptPolicy::Manual
    };
    if let Some(p) = &a.config {
        let cfg: DeviceConfig = serde_json::from_slice(&std::fs::read(p)?)?;
        opts.config_override = Some(cfg);
    }
    opts.tap = rec.clone().map(|r| Arc::new(RecTap(r)) as Arc<dyn Tap>);
    let sink = Arc::new(Sink {
        store: store.clone(),
        rec,
    });
    sdk::SdkSettings {
        auto_accept_local: a.auto_accept_local,
        ..Default::default()
    }
    .save(&dir.join("sdk"))?;
    let sdk = Sdk::new(
        opts,
        sink,
        store.clone(),
        Arc::new(IdGen::starting_after(0)),
    )?;

    if a.fetch_lazy {
        let s = sdk.clone();
        sdk.on_finished(move |r| {
            let Some(i) = &r.sdk else { return };
            for (state, part) in [
                (i.req_body, store::BodyPart::Request),
                (i.res_body, store::BodyPart::Response),
            ] {
                if state == flow::BodyState::Lazy {
                    let (s, id) = (s.clone(), r.id);
                    tokio::spawn(async move {
                        let _ = s.fetch_body(id, part).await;
                    });
                }
            }
        });
    }
    if let Some(n) = a.revoke_after {
        let s = sdk.clone();
        let count = Arc::new(std::sync::atomic::AtomicU64::new(0));
        sdk.on_finished(move |r| {
            if count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1 == n {
                if let Some(i) = &r.sdk {
                    s.revoke(&i.device_id, &i.app_id);
                }
            }
        });
    }

    sdk.set_enabled(true).await?;
    let st = sdk.status();
    println!(
        "{}",
        serde_json::json!({"event": "ready", "port": sdk.local_port(), "fingerprint": st.fingerprint})
    );
    let _ = std::io::stdout().flush();
    tokio::signal::ctrl_c().await?;
    sdk.set_enabled(false).await?;
    store.close();
    Ok(())
}

mod anyhow_free {
    pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
}
