//! nekod: the headless capture core.

mod client;
mod paths;
mod run;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "nekod", version, about = "HTTP/HTTPS debugging proxy core")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the proxy and the client API.
    Run(run::RunArgs),
    /// List captured flows from the local store.
    Flows(FlowsArgs),
    /// Export captured flows.
    Export(ExportArgs),
    /// Import a HAR file into the running core.
    ImportHar {
        file: PathBuf,
        #[arg(long)]
        socket: Option<PathBuf>,
    },
    /// Show how to connect a local AI client (Claude Code) to the MCP server.
    Mcp(McpArgs),
    /// Turn SDK connections on or off, and see their state (needs a running core).
    Sdk {
        #[command(subcommand)]
        action: SdkAction,
    },
    /// Paired devices and pairing requests (needs a running core).
    Devices {
        #[command(subcommand)]
        action: DevicesAction,
    },
    /// Inspect the proxy CA.
    Ca {
        #[command(subcommand)]
        action: CaAction,
    },
}

#[derive(clap::Args, Clone)]
struct SocketArg {
    /// API socket path.
    #[arg(long)]
    socket: Option<PathBuf>,
    /// Machine-readable output.
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
enum SdkAction {
    /// Start accepting SDK connections on the LAN.
    Enable(SocketArg),
    /// Stop accepting SDK connections and close the open ones.
    Disable(SocketArg),
    /// Port, certificate fingerprint, adb and discovery state, drop counters.
    Status(SocketArg),
    /// Change a remembered setting: `nekod sdk set lan off`, `nekod sdk set auto-accept-local on`.
    Set {
        #[arg(value_parser = ["lan", "auto-accept-local"])]
        key: String,
        #[arg(value_parser = ["on", "off"])]
        value: String,
        #[command(flatten)]
        s: SocketArg,
    },
}

#[derive(Subcommand)]
enum DevicesAction {
    /// Paired and pending devices.
    List(SocketArg),
    /// Approve a pairing request after checking the code matches the app's log.
    Allow {
        /// Pending request id from `devices list`.
        id: u64,
        #[command(flatten)]
        s: SocketArg,
    },
    Deny {
        id: u64,
        #[command(flatten)]
        s: SocketArg,
    },
    /// Delete a device's token; it has to pair again.
    Revoke {
        device_id: String,
        /// The app id (see `devices list`).
        #[arg(long)]
        app: String,
        #[command(flatten)]
        s: SocketArg,
    },
    /// Remove a device and all its apps.
    Forget {
        device_id: String,
        #[command(flatten)]
        s: SocketArg,
    },
}

#[derive(clap::Args)]
struct McpArgs {
    /// Address the running core serves MCP on.
    #[arg(long, default_value = mcp::DEFAULT_ADDR)]
    listen: std::net::SocketAddr,
    /// Print only the endpoint URL and token, as JSON.
    #[arg(long)]
    json: bool,
    #[arg(long)]
    data_dir: Option<PathBuf>,
}

#[derive(clap::Args)]
struct FlowsArgs {
    /// Text and operators: method:POST status:5xx host:example.com is:slow
    query: Option<String>,
    #[arg(short, long, default_value_t = 50)]
    limit: u32,
    /// Only errors (status >= 400 or failed).
    #[arg(long)]
    errors: bool,
    /// Only flows from one source. `sdk` includes merged ones.
    #[arg(long, value_parser = ["sdk", "proxy", "merged"])]
    source: Option<String>,
    /// One JSON object per line.
    #[arg(long)]
    json: bool,
    #[arg(long)]
    data_dir: Option<PathBuf>,
}

#[derive(clap::Args)]
struct ExportArgs {
    /// HAR 1.2 (the only format for now).
    #[arg(long, default_value_t = true)]
    har: bool,
    #[arg(long, default_value = "har")]
    kind: String,
    /// Write to a file instead of stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Limit to a search, same syntax as `flows`.
    #[arg(long)]
    query: Option<String>,
    /// Comma-separated flow ids.
    #[arg(long, value_delimiter = ',')]
    ids: Vec<u64>,
    /// Deprecated: unredacted exports are refused.
    #[arg(long)]
    no_redact: bool,
    #[arg(long)]
    data_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
enum CaAction {
    /// Print the path of the CA certificate.
    Path {
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
    /// Print the CA certificate (PEM).
    Print {
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
    /// Print the SHA-256 fingerprint.
    Fingerprint {
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let started = Instant::now();
    let cli = Cli::parse();
    match cli.command {
        Command::Run(args) => {
            init_tracing();
            let workers = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(2)
                .clamp(2, 4);
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(workers)
                .max_blocking_threads(32)
                .enable_all()
                .build()?
                .block_on(run::run(args, started))
        }
        Command::Flows(a) => flows(a),
        Command::Export(a) => export(a),
        Command::ImportHar { file, socket } => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                use std::io::Read;
                let mut data = Vec::new();
                std::fs::File::open(file)?
                    .take(store::HAR_IMPORT_LIMIT as u64 + 1)
                    .read_to_end(&mut data)?;
                anyhow::ensure!(data.len() <= store::HAR_IMPORT_LIMIT, "HAR exceeds 64 MiB");
                let mut c =
                    client::Client::connect(&socket.unwrap_or_else(paths::socket_path)).await?;
                let result = c
                    .call(api::pb::client_message::Body::ImportHar(
                        api::pb::ImportHarRequest { data },
                    ))
                    .await?;
                if let api::pb::server_message::Body::ImportedHar(r) = result {
                    println!("Imported {} flows", r.count);
                }
                Ok(())
            }),
        Command::Ca { action } => ca(action),
        Command::Sdk { action } => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(sdk_cmd(action)),
        Command::Devices { action } => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(devices_cmd(action)),
        Command::Mcp(a) => mcp_info(a),
    }
}

fn mcp_info(a: McpArgs) -> Result<()> {
    let dir = a.data_dir.unwrap_or_else(paths::data_dir);
    let token = mcp::load_or_create_token(&dir).context("reading the MCP token")?;
    let url = format!("http://{}/mcp", a.listen);
    if a.json {
        println!("{}", serde_json::json!({ "url": url, "token": token }));
        return Ok(());
    }
    println!("MCP endpoint  {url}");
    println!("Token file    {}", dir.join("mcp.token").display());
    println!();
    println!("Connect Claude Code (run once):");
    println!(
        "  claude mcp add --transport http proxy {url} --header \"Authorization: Bearer {token}\""
    );
    println!();
    println!("Start `nekod run` (or the desktop app) first. Reads are always allowed;");
    println!("changes wait for approval in the app unless you choose otherwise in Settings.");
    Ok(())
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("nekod=info,warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time()
        .init();
}

fn open_store(dir: Option<PathBuf>) -> Result<store::Store> {
    let dir = dir.unwrap_or_else(paths::data_dir);
    if !dir.join("flows.db").exists() {
        bail!(
            "no capture data in {} (run `nekod run` first)",
            dir.display()
        );
    }
    store::Store::open(&dir).context("opening the store")
}

fn flows(a: FlowsArgs) -> Result<()> {
    let store = open_store(a.data_dir)?;
    let mut text = a.query.unwrap_or_default();
    if let Some(src) = &a.source {
        text = format!("source:{src} {text}");
    }
    let q = store::Query {
        text,
        kind: if a.errors {
            store::Kind::Errors
        } else {
            store::Kind::All
        },
        ..Default::default()
    };
    let page = store.search(&q, 0, a.limit.max(1))?;
    // Show the newest when the result is longer than the limit.
    let items = if page.total > page.items.len() as u64 {
        store
            .search(&q, (page.total as u32).saturating_sub(a.limit), a.limit)?
            .items
    } else {
        page.items
    };
    let mut out = std::io::stdout().lock();
    for s in &items {
        if a.json {
            let v = serde_json::json!({
                "id": s.id, "started_at_ms": s.started_at_ms, "method": s.method,
                "scheme": s.scheme, "host": s.host, "path": s.path, "status": s.status,
                "protocol": s.protocol, "duration_ms": s.duration_ms, "req_size": s.req_size,
                "res_size": s.res_size, "app": s.app, "flags": s.flags, "error": s.error,
                "source": match s.source {
                    flow::Source::Proxy => "proxy",
                    flow::Source::Sdk => "sdk",
                    flow::Source::Merged => "merged",
                },
                "device_id": s.device_id, "app_id": s.app_id, "name": s.name,
                "screen": s.screen, "route_template": s.route_template,
            });
            writeln!(out, "{v}")?;
        } else {
            let status = if s.status == 0 {
                "-".to_string()
            } else {
                s.status.to_string()
            };
            let time = if s.duration_ms > 0.0 {
                format!("{:.0} ms", s.duration_ms)
            } else {
                "-".into()
            };
            writeln!(
                out,
                "{:>6}  {:<7} {:>4}  {:>8}  {:>9}  {}{}",
                s.id,
                s.method,
                status,
                time,
                size(s.res_size),
                s.host,
                s.path
            )?;
        }
    }
    if !a.json {
        writeln!(out, "{} of {} flows", items.len(), page.total)?;
    }
    Ok(())
}

fn size(b: u64) -> String {
    match b {
        0 => "-".into(),
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.1} KB", b as f64 / 1024.0),
        b => format!("{:.1} MB", b as f64 / 1048576.0),
    }
}

fn export(a: ExportArgs) -> Result<()> {
    let store = open_store(a.data_dir)?;
    let ids = if a.ids.is_empty() {
        let q = store::Query {
            text: a.query.unwrap_or_default(),
            ..Default::default()
        };
        store.ids(&q, 10_001)?
    } else {
        a.ids
    };
    anyhow::ensure!(ids.len() <= 10_000, "export limited to 10,000 flows");
    let mut total = 0usize;
    let mut recs = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(r) = store.get(id)? {
            total += store::export_weight(&r);
            anyhow::ensure!(
                total <= 32 * 1024 * 1024,
                "export source data exceeds 32 MiB"
            );
            recs.push(r);
        }
    }
    anyhow::ensure!(!a.no_redact, "exports always redact credentials");
    let artifact =
        store::export(&a.kind, &recs, env!("CARGO_PKG_VERSION")).map_err(anyhow::Error::msg)?;
    let bytes = artifact.data;
    match a.output {
        Some(p) => {
            std::fs::write(&p, &bytes).with_context(|| format!("writing {}", p.display()))?;
            eprintln!("wrote {} entries to {}", recs.len(), p.display());
        }
        None => {
            let mut out = std::io::stdout().lock();
            out.write_all(&bytes)?;
            out.write_all(b"\n")?;
        }
    }
    Ok(())
}

fn ca(action: CaAction) -> Result<()> {
    let (dir, what) = match action {
        CaAction::Path { data_dir } => (data_dir, 0),
        CaAction::Print { data_dir } => (data_dir, 1),
        CaAction::Fingerprint { data_dir } => (data_dir, 2),
    };
    let dir = dir.unwrap_or_else(paths::data_dir);
    // Creates the CA on first use so `ca path` works before the first run.
    let ca = tls::CaStore::load_or_create(&dir, tls::KeyStorage::Auto).context("loading the CA")?;
    match what {
        0 => println!("{}", ca.cert_path().display()),
        1 => print!("{}", ca.cert_pem()),
        _ => println!("{}", ca.sha256()),
    }
    Ok(())
}

async fn connect(s: &SocketArg) -> Result<client::Client> {
    client::Client::connect(&s.socket.clone().unwrap_or_else(paths::socket_path)).await
}

fn print_sdk_status(st: &api::pb::SdkStatus, json: bool) {
    if json {
        let v = serde_json::json!({
            "available": st.available, "enabled": st.enabled, "port": st.port,
            "fingerprint": st.fingerprint, "desktop_name": st.desktop_name, "health": st.health,
            "checks": st.checks.iter().map(|c| serde_json::json!({"id": c.id, "level": c.level, "title": c.title, "detail": c.detail, "fix": c.fix})).collect::<Vec<_>>(),
            "listening": st.listening, "adb": st.adb_state, "adb_devices": st.adb_devices,
            "discovery": st.discovery_state, "sessions": st.sessions,
            "dropped_events": st.stats.as_ref().map(|s| s.dropped_events),
            "dropped_chunks": st.stats.as_ref().map(|s| s.dropped_chunks),
        });
        println!("{v}");
        return;
    }
    if !st.available {
        println!("This core runs without the SDK listener.");
        return;
    }
    println!(
        "SDK connections  {}  ({})",
        if st.enabled { "on" } else { "off" },
        st.health
    );
    println!("Port             {}", st.port);
    println!(
        "Wi-Fi phones     {}",
        if st.lan {
            "allowed (they need an Allow)"
        } else {
            "off (this computer only)"
        }
    );
    println!(
        "Auto-accept      {}",
        if st.auto_accept_local {
            "connections from this computer"
        } else {
            "off, every device asks"
        }
    );
    println!("Fingerprint      {}", st.fingerprint);
    for l in &st.listening {
        println!("Listening on     {l}");
    }
    println!("adb              {}", st.adb_label);
    println!(
        "Discovery        {}{}",
        st.discovery_state,
        if st.discovery_error.is_empty() {
            String::new()
        } else {
            format!(" ({})", st.discovery_error)
        }
    );
    println!("Sessions         {}", st.sessions);
    // Only what needs a person; the rest is the happy path.
    for c in st
        .checks
        .iter()
        .filter(|c| matches!(c.level.as_str(), "warn" | "error"))
    {
        println!(
            "{:<16} {}",
            if c.level == "error" {
                "Problem"
            } else {
                "Attention"
            },
            c.title
        );
        if !c.detail.is_empty() {
            println!("{:<16} {}", "", c.detail);
        }
        if !c.fix.is_empty() {
            println!("{:<16} fix: {}", "", c.fix);
        }
    }
    if let Some(s) = &st.stats {
        println!(
            "Calls            {} (dropped: {} events, {} chunks, {} calls)",
            s.calls, s.dropped_events, s.dropped_chunks, s.dropped_calls
        );
    }
}

async fn sdk_cmd(action: SdkAction) -> Result<()> {
    use api::pb::{client_message::Body as Req, server_message::Body as Res};
    let (s, req) = match action {
        SdkAction::Enable(s) => (
            s,
            Req::SdkSetEnabled(api::pb::SdkSetEnabledRequest { enabled: true }),
        ),
        SdkAction::Disable(s) => (
            s,
            Req::SdkSetEnabled(api::pb::SdkSetEnabledRequest { enabled: false }),
        ),
        SdkAction::Status(s) => (s, Req::SdkStatus(api::pb::SdkStatusRequest {})),
        SdkAction::Set { key, value, s } => {
            // Read the current state first, so only the named setting changes.
            let mut c = connect(&s).await?;
            let Res::SdkStatus(cur) = c.call(Req::SdkStatus(api::pb::SdkStatusRequest {})).await?
            else {
                bail!("unexpected answer from the core")
            };
            let on = value == "on";
            let (lan, auto) = if key == "lan" {
                (on, cur.auto_accept_local)
            } else {
                (cur.lan, on)
            };
            (
                s,
                Req::SdkSetSettings(api::pb::SdkSetSettingsRequest {
                    enabled: cur.enabled,
                    lan,
                    auto_accept_local: auto,
                }),
            )
        }
    };
    let mut c = connect(&s).await?;
    match c.call(req).await? {
        Res::SdkStatus(st) => {
            print_sdk_status(&st, s.json);
            Ok(())
        }
        _ => bail!("unexpected answer from the core"),
    }
}

async fn devices_cmd(action: DevicesAction) -> Result<()> {
    use api::pb::{
        client_message::Body as Req, device_action_request::Kind, server_message::Body as Res,
    };
    let act = |kind: Kind, pending_id: u64, device_id: String, app_id: String| {
        Req::DeviceAction(api::pb::DeviceActionRequest {
            kind: kind as i32,
            pending_id,
            device_id,
            app_id,
        })
    };
    let (s, req) = match action {
        DevicesAction::List(s) => (
            s,
            Req::DevicesSubscribe(api::pb::DevicesSubscribeRequest {}),
        ),
        DevicesAction::Allow { id, s } => (s, act(Kind::Allow, id, String::new(), String::new())),
        DevicesAction::Deny { id, s } => (s, act(Kind::Deny, id, String::new(), String::new())),
        DevicesAction::Revoke { device_id, app, s } => (s, act(Kind::Revoke, 0, device_id, app)),
        DevicesAction::Forget { device_id, s } => {
            (s, act(Kind::Forget, 0, device_id, String::new()))
        }
    };
    let mut c = connect(&s).await?;
    match c.call(req).await? {
        Res::Devices(d) => {
            if s.json {
                let v = serde_json::json!({
                    "enabled": d.enabled,
                    "pending": d.pending.iter().map(|p| serde_json::json!({
                        "id": p.id, "device_id": p.device_id, "app_id": p.app_id,
                        "device_name": p.device_name, "code": p.code, "repair": p.repair,
                    })).collect::<Vec<_>>(),
                    "devices": d.devices.iter().map(|x| serde_json::json!({
                        "device_id": x.device_id, "app_id": x.app_id, "device_name": x.device_name,
                        "sdk": x.sdk_name, "sdk_version": x.sdk_version, "status": x.status,
                        "connection": x.connection, "flows": x.flows,
                    })).collect::<Vec<_>>(),
                });
                println!("{v}");
                return Ok(());
            }
            if !d.enabled {
                println!("SDK connections are off. Run `nekod sdk enable`.");
            }
            for p in &d.pending {
                println!(
                    "PENDING #{}  {} ({}) wants to pair as {}  code {} {}",
                    p.id,
                    p.device_name,
                    p.device_model,
                    p.app_id,
                    &p.code[..p.code.len().min(3)],
                    p.code.get(3..).unwrap_or("")
                );
                println!("             Check the same code is in the app's log, then `nekod devices allow {}`.", p.id);
            }
            for x in &d.devices {
                let status = match api::pb::DeviceStatus::try_from(x.status).unwrap_or_default() {
                    api::pb::DeviceStatus::DeviceConnected => "connected",
                    api::pb::DeviceStatus::DeviceIdle => "idle",
                    api::pb::DeviceStatus::DeviceDenied => "denied",
                    api::pb::DeviceStatus::DeviceRevoked => "revoked",
                };
                println!(
                    "{:<10} {:<18} {:<28} {:<8} {:<9} {} flows   id {}",
                    status, x.device_name, x.app_id, x.sdk_name, x.connection, x.flows, x.device_id
                );
            }
            if d.devices.is_empty() && d.pending.is_empty() {
                println!("No devices yet.");
            }
            Ok(())
        }
        Res::Ok(_) => {
            println!("done");
            Ok(())
        }
        _ => bail!("unexpected answer from the core"),
    }
}
