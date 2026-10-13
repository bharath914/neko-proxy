//! proxyd: the headless capture core.

mod paths;
mod run;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "proxyd", version, about = "HTTP/HTTPS debugging proxy core")]
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
    /// Show how to connect a local AI client (Claude Code) to the MCP server.
    Mcp(McpArgs),
    /// Inspect the proxy CA.
    Ca {
        #[command(subcommand)]
        action: CaAction,
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
    /// Write to a file instead of stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Limit to a search, same syntax as `flows`.
    #[arg(long)]
    query: Option<String>,
    /// Comma-separated flow ids.
    #[arg(long, value_delimiter = ',')]
    ids: Vec<u64>,
    /// Keep Authorization, cookies and tokens as captured.
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
        Command::Ca { action } => ca(action),
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
    println!("Start `proxyd run` (or the desktop app) first. Reads are always allowed;");
    println!("changes wait for approval in the app unless you choose otherwise in Settings.");
    Ok(())
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("proxyd=info,warn"));
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
            "no capture data in {} (run `proxyd run` first)",
            dir.display()
        );
    }
    store::Store::open(&dir).context("opening the store")
}

fn flows(a: FlowsArgs) -> Result<()> {
    let store = open_store(a.data_dir)?;
    let q = store::Query {
        text: a.query.unwrap_or_default(),
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
        store.ids(&q, 1_000_000)?
    } else {
        a.ids
    };
    let mut recs = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(r) = store.get(id)? {
            recs.push(r);
        }
    }
    let har = store::to_har(&recs, !a.no_redact, env!("CARGO_PKG_VERSION"));
    let bytes = serde_json::to_vec_pretty(&har)?;
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
