//! The only way the app talks to the core: the client API over a Unix
//! socket. `RealBackend` owns the connection and supervises `proxyd`;
//! `DemoBackend` serves fixtures so the UI can run (and be screenshotted)
//! without a core.

use futures_util::{SinkExt, StreamExt};
use proto::pb::{self, client_message::Body as Req, server_message::Body as Res};
use proto::prost::Message as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::sync::mpsc as tmpsc;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;

pub type Tag = u64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnState {
    Starting,
    Connected,
    Reconnecting { attempt: u32 },
    Failed(String),
}

pub enum Event {
    Conn(ConnState),
    Response(Tag, Res),
    Batch(pb::FlowBatch),
    Resync,
    /// A message was held at a breakpoint, or released.
    Pause(pb::PauseNotice),
    /// An AI client asked for approval, or a request was answered.
    Approval(pb::ApprovalNotice),
}

pub trait Backend: Send {
    fn request(&self, body: Req) -> Tag;
    fn drain(&self) -> Vec<Event>;
}

// ------------------------------------------------------------------ real

#[derive(Clone, Debug)]
pub struct CoreOptions {
    pub socket: PathBuf,
    /// Path to `proxyd`. `None` means only connect to an existing core.
    pub proxyd: Option<PathBuf>,
    pub listen: String,
}

pub struct RealBackend {
    next: AtomicU64,
    cmd: tmpsc::UnboundedSender<(Tag, Req)>,
    events: Mutex<mpsc::Receiver<Event>>,
    stop: Arc<tokio::sync::Notify>,
}

impl RealBackend {
    pub fn start(opts: CoreOptions, ctx: egui::Context) -> RealBackend {
        let (cmd, cmd_rx) = tmpsc::unbounded_channel();
        let (ev_tx, ev_rx) = mpsc::channel();
        let stop = Arc::new(tokio::sync::Notify::new());
        let stop2 = stop.clone();
        std::thread::Builder::new()
            .name("core-link".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .expect("runtime");
                rt.block_on(async move {
                    tokio::select! {
                        _ = manage(opts, cmd_rx, ev_tx, ctx) => {}
                        _ = stop2.notified() => {}
                    }
                });
                // Dropping the runtime drops the supervised child (kill_on_drop).
            })
            .expect("core-link thread");
        RealBackend {
            next: AtomicU64::new(1),
            cmd,
            events: Mutex::new(ev_rx),
            stop,
        }
    }
}

impl Drop for RealBackend {
    fn drop(&mut self) {
        self.stop.notify_one();
        // Give the runtime a moment to reap the child.
        std::thread::sleep(Duration::from_millis(60));
    }
}

impl Backend for RealBackend {
    fn request(&self, body: Req) -> Tag {
        let tag = self.next.fetch_add(1, Ordering::Relaxed);
        let _ = self.cmd.send((tag, body));
        tag
    }

    fn drain(&self) -> Vec<Event> {
        let rx = self.events.lock().unwrap();
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    }
}

async fn manage(
    opts: CoreOptions,
    mut cmd_rx: tmpsc::UnboundedReceiver<(Tag, Req)>,
    ev: mpsc::Sender<Event>,
    ctx: egui::Context,
) {
    let emit = |e: Event| {
        let _ = ev.send(e);
        ctx.request_repaint();
    };
    let mut child: Option<tokio::process::Child> = None;
    let mut attempt: u32 = 0;
    let mut spawn_failures: u32 = 0;
    emit(Event::Conn(ConnState::Starting));

    loop {
        match connect(&opts.socket).await {
            Ok(ws) => {
                attempt = 0;
                emit(Event::Conn(ConnState::Connected));
                session(ws, &mut cmd_rx, &ev, &ctx).await;
                emit(Event::Conn(ConnState::Reconnecting { attempt: 1 }));
                // Anything queued for the dead connection is stale.
                while cmd_rx.try_recv().is_ok() {}
            }
            Err(_) => {
                attempt += 1;
                let exited = match child.as_mut() {
                    Some(c) => c.try_wait().ok().flatten().is_some(),
                    None => true,
                };
                if exited {
                    if let Some(path) = &opts.proxyd {
                        match spawn_core(path, &opts) {
                            Ok(c) => {
                                child = Some(c);
                                spawn_failures = 0;
                            }
                            Err(e) => {
                                spawn_failures += 1;
                                if spawn_failures >= 3 {
                                    emit(Event::Conn(ConnState::Failed(format!(
                                        "Could not start {}: {e}",
                                        path.display()
                                    ))));
                                }
                            }
                        }
                    } else if attempt >= 3 {
                        emit(Event::Conn(ConnState::Failed(
                            "proxyd was not found and nothing is listening on the API socket"
                                .into(),
                        )));
                    }
                }
                if attempt > 1 && !matches!(spawn_failures, 3..) {
                    emit(Event::Conn(ConnState::Reconnecting { attempt }));
                }
                let wait = match attempt {
                    0..=3 => 120,
                    4..=8 => 500,
                    _ => 2000,
                };
                tokio::time::sleep(Duration::from_millis(wait)).await;
                while cmd_rx.try_recv().is_ok() {}
            }
        }
    }
}

fn spawn_core(path: &PathBuf, opts: &CoreOptions) -> std::io::Result<tokio::process::Child> {
    let mut cmd = tokio::process::Command::new(path);
    cmd.arg("run")
        .arg("--listen")
        .arg(&opts.listen)
        .arg("--socket")
        .arg(&opts.socket)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    // If the app is killed outright, the core goes with it.
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    cmd.spawn()
}

type Ws = tokio_tungstenite::WebSocketStream<UnixStream>;

async fn connect(path: &PathBuf) -> Result<Ws, Box<dyn std::error::Error + Send + Sync>> {
    let stream = UnixStream::connect(path).await?;
    let cfg = WebSocketConfig::default()
        .max_message_size(Some(256 << 20))
        .max_frame_size(Some(256 << 20));
    let (ws, _) =
        tokio_tungstenite::client_async_with_config("ws://localhost/", stream, Some(cfg)).await?;
    Ok(ws)
}

async fn session(
    ws: Ws,
    cmd_rx: &mut tmpsc::UnboundedReceiver<(Tag, Req)>,
    ev: &mpsc::Sender<Event>,
    ctx: &egui::Context,
) {
    let (mut sink, mut stream) = ws.split();
    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => {
                let Some((tag, body)) = cmd else { return };
                let msg = pb::ClientMessage { id: tag, body: Some(body) };
                if sink.send(Message::binary(msg.encode_to_vec())).await.is_err() {
                    return;
                }
            }
            frame = stream.next() => {
                let Some(Ok(frame)) = frame else { return };
                let Message::Binary(data) = frame else {
                    if matches!(frame, Message::Close(_)) { return }
                    continue;
                };
                let Ok(m) = pb::ServerMessage::decode(&data[..]) else { continue };
                let event = match (m.id, m.body) {
                    (_, None) => continue,
                    (0, Some(Res::Batch(b))) => Event::Batch(b),
                    (0, Some(Res::Resync(_))) => Event::Resync,
                    (0, Some(Res::PauseNotice(n))) => Event::Pause(n),
                    (0, Some(Res::ApprovalNotice(n))) => Event::Approval(n),
                    (id, Some(body)) => Event::Response(id, body),
                };
                let _ = ev.send(event);
                ctx.request_repaint();
            }
        }
    }
}

// ------------------------------------------------------------------ demo

/// Fixture backend: a believable capture with no core behind it.
pub struct DemoBackend {
    inner: Mutex<DemoState>,
    next: AtomicU64,
}

struct DemoState {
    flows: Vec<pb::FlowDetail>,
    recording: bool,
    queue: Vec<Event>,
    live: bool,
    last_tick: std::time::Instant,
    next_id: u64,
    ssl: pb::SslSettings,
    map_local: Vec<pb::MapLocalRule>,
    pinned: Vec<String>,
    next_rule: u64,
    rules: Vec<pb::Rule>,
    paused: Vec<pb::PausedFlow>,
    approvals: Vec<pb::Approval>,
    mcp_writes: i32,
}

impl DemoBackend {
    pub fn new(count: usize, live: bool) -> DemoBackend {
        let flows = (1..=count as u64).map(demo_flow).collect::<Vec<_>>();
        DemoBackend {
            inner: Mutex::new(DemoState {
                flows,
                recording: true,
                queue: vec![Event::Conn(ConnState::Connected)],
                live,
                last_tick: std::time::Instant::now(),
                next_id: count as u64 + 1,
                ssl: pb::SslSettings::default(),
                map_local: Vec::new(),
                pinned: vec!["pay.example.com".into()],
                next_rule: 1,
                rules: demo_rules_v2(),
                paused: demo_paused(),
                approvals: demo_approvals(),
                mcp_writes: 0,
            }),
            next: AtomicU64::new(1),
        }
    }
}

fn hdr(n: &str, v: &str) -> pb::Header {
    pb::Header {
        name: n.into(),
        value: v.into(),
    }
}

fn json_lines(pairs: &[(&str, &str)]) -> Vec<pb::JsonLine> {
    use pb::JsonTokenKind as K;
    let tok = |kind: K, text: &str| pb::JsonToken {
        kind: kind as i32,
        text: text.into(),
    };
    let mut out = vec![pb::JsonLine {
        depth: 0,
        tokens: vec![tok(K::Punct, "{")],
        fold_end: pairs.len() as i32 + 1,
    }];
    for (i, (k, v)) in pairs.iter().enumerate() {
        let comma = i + 1 < pairs.len();
        let mut tokens = vec![tok(K::Key, &format!("\"{k}\"")), tok(K::Punct, ": ")];
        let kind = if v.starts_with('"') {
            K::String
        } else if *v == "true" || *v == "false" || *v == "null" {
            K::Literal
        } else {
            K::Number
        };
        tokens.push(tok(kind, v));
        if comma {
            tokens.push(tok(K::Punct, ","));
        }
        out.push(pb::JsonLine {
            depth: 1,
            tokens,
            fold_end: -1,
        });
    }
    out.push(pb::JsonLine {
        depth: 0,
        tokens: vec![tok(K::Punct, "}")],
        fold_end: -1,
    });
    out
}


/// Turn pretty-printed JSON (two-space indent, one value per line) into the
/// outline the core would send. Fixture use only.
fn json_from_pretty(text: &str) -> Vec<pb::JsonLine> {
    use pb::JsonTokenKind as K;
    let tok = |kind: K, text: &str| pb::JsonToken {
        kind: kind as i32,
        text: text.into(),
    };
    let value = |v: &str| -> pb::JsonToken {
        let kind = if v.starts_with('"') {
            K::String
        } else if matches!(v, "true" | "false" | "null") {
            K::Literal
        } else if v == "{" || v == "}" || v == "[" || v == "]" || v == "{}" || v == "[]" {
            K::Punct
        } else {
            K::Number
        };
        tok(kind, v)
    };
    let mut out: Vec<pb::JsonLine> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    for raw in text.lines() {
        let trimmed = raw.trim_start();
        let depth = ((raw.len() - trimmed.len()) / 2) as u32;
        let mut tokens = Vec::new();
        let mut rest = trimmed;
        if rest.starts_with('"') {
            if let Some(i) = rest[1..].find("\": ") {
                let close = i + 1;
                tokens.push(tok(K::Key, &rest[..=close]));
                tokens.push(tok(K::Punct, ": "));
                rest = &rest[close + 3..];
            }
        }
        let (val, comma) = match rest.strip_suffix(',') {
            Some(v) => (v, true),
            None => (rest, false),
        };
        let closes = val.starts_with('}') || val.starts_with(']');
        let opens = val.ends_with('{') || val.ends_with('[');
        if closes {
            if let Some(o) = open.pop() {
                let here = out.len() as i32;
                out[o].fold_end = here;
            }
        }
        tokens.push(value(val));
        if comma {
            tokens.push(tok(K::Punct, ","));
        }
        if opens {
            open.push(out.len());
        }
        out.push(pb::JsonLine {
            depth,
            tokens,
            fold_end: -1,
        });
    }
    out
}

fn demo_order_json() -> String {
    let mut s = String::from("{\n  \"page\": 2,\n  \"per_page\": 20,\n  \"total\": 184,\n  \"has_more\": true,\n  \"items\": [\n");
    let names = [
        ("Aurora USB-C Charger 65W", 2499, "in_stock"),
        ("Nimbus Braided Cable 2m", 799, "in_stock"),
        ("Pebble Power Bank 20k", 3299, "low_stock"),
        ("Halo Wireless Pad", 1899, "backorder"),
    ];
    for i in 0..14 {
        let (n, price, stock) = names[i % names.len()];
        s.push_str("    {\n");
        s.push_str(&format!("      \"sku\": \"SKU-{:04}\",\n", 1042 + i * 7));
        s.push_str(&format!("      \"title\": \"{n}\",\n"));
        s.push_str(&format!("      \"price\": {},\n", price + (i as i32) * 10));
        s.push_str(&format!("      \"rating\": {:.1},\n", 3.6 + (i % 5) as f32 * 0.3));
        s.push_str(&format!("      \"stock\": \"{stock}\",\n"));
        s.push_str("      \"tags\": [\n");
        s.push_str("        \"charging\",\n");
        s.push_str("        \"usb-c\"\n");
        s.push_str("      ],\n");
        s.push_str("      \"promo\": null\n");
        s.push_str(if i == 13 { "    }\n" } else { "    },\n" });
    }
    s.push_str("  ],\n  \"cursor\": \"eyJwIjoyfQ\"\n}");
    s
}

const DEMO_PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Getting started · Example Docs</title>
  <link rel="stylesheet" href="/assets/docs.css">
  <script src="/assets/app.js" defer></script>
</head>
<body class="guide">
  <h1>Getting started</h1>
  <p>Welcome to <strong>Example Cloud</strong>. This guide takes you from an empty account to a first
     request in about five minutes. Read the <a href="/docs/concepts">concepts</a> page if you want the
     <em>why</em> behind each step.</p>
  <h2>Before you begin</h2>
  <ul>
    <li>Create an account and verify your email address.</li>
    <li>Install the <code>example</code> CLI (version 2.4 or later).</li>
    <li>Have an API key ready &mdash; see <a href="/docs/keys">API keys</a>.</li>
  </ul>
  <h2>Make your first request</h2>
  <ol>
    <li>Export your key as <code>EXAMPLE_KEY</code>.</li>
    <li>Run the command below.</li>
    <li>Check that the response status is <strong>200</strong>.</li>
  </ol>
  <pre>curl https://api.example.com/v1/me \
  -H "Authorization: Bearer $EXAMPLE_KEY"</pre>
  <blockquote>Keys are shown once. Store them in a secrets manager, never in source control.</blockquote>
  <img src="/img/diagram-flow.png" alt="Request flow from client to edge to origin">
  <hr>
  <p>Need help? Email <a href="mailto:support@example.com">support@example.com</a> &copy; 2026 Example, Inc.</p>
</body>
</html>"#;

fn demo_html_view() -> pb::BodyView {
    use pb::HtmlBlockKind as B;
    use pb::SourceTokenKind as S;
    let span = |text: &str, style: u32, href: &str| pb::HtmlSpan {
        text: text.into(),
        style,
        href: href.into(),
    };
    let block = |kind: B, level: u32, marker: &str, spans: Vec<pb::HtmlSpan>| pb::HtmlBlock {
        kind: kind as i32,
        level,
        marker: marker.into(),
        spans,
    };
    let html = vec![
        block(B::HtmlHeading, 1, "", vec![span("Getting started", 0, "")]),
        block(
            B::HtmlParagraph,
            0,
            "",
            vec![
                span("Welcome to ", 0, ""),
                span("Example Cloud", 1, ""),
                span(". This guide takes you from an empty account to a first request in about five minutes. Read the ", 0, ""),
                span("concepts", 8, "/docs/concepts"),
                span(" page if you want the ", 0, ""),
                span("why", 2, ""),
                span(" behind each step.", 0, ""),
            ],
        ),
        block(B::HtmlHeading, 2, "", vec![span("Before you begin", 0, "")]),
        block(B::HtmlListItem, 0, "•", vec![span("Create an account and verify your email address.", 0, "")]),
        block(
            B::HtmlListItem,
            0,
            "•",
            vec![span("Install the ", 0, ""), span("example", 4, ""), span(" CLI (version 2.4 or later).", 0, "")],
        ),
        block(
            B::HtmlListItem,
            0,
            "•",
            vec![span("Have an API key ready — see ", 0, ""), span("API keys", 8, "/docs/keys"), span(".", 0, "")],
        ),
        block(B::HtmlHeading, 2, "", vec![span("Make your first request", 0, "")]),
        block(
            B::HtmlListItem,
            0,
            "1.",
            vec![span("Export your key as ", 0, ""), span("EXAMPLE_KEY", 4, ""), span(".", 0, "")],
        ),
        block(B::HtmlListItem, 0, "2.", vec![span("Run the command below.", 0, "")]),
        block(
            B::HtmlListItem,
            0,
            "3.",
            vec![span("Check that the response status is ", 0, ""), span("200", 1, ""), span(".", 0, "")],
        ),
        block(
            B::HtmlCode,
            0,
            "",
            vec![span("curl https://api.example.com/v1/me \\\n  -H \"Authorization: Bearer $EXAMPLE_KEY\"", 4, "")],
        ),
        block(
            B::HtmlQuote,
            1,
            "",
            vec![span("Keys are shown once. Store them in a secrets manager, never in source control.", 0, "")],
        ),
        block(
            B::HtmlImage,
            0,
            "",
            vec![span("Request flow from client to edge to origin", 0, "/img/diagram-flow.png")],
        ),
        block(B::HtmlRule, 0, "", vec![]),
        block(
            B::HtmlParagraph,
            0,
            "",
            vec![
                span("Need help? Email ", 0, ""),
                span("support@example.com", 8, "mailto:support@example.com"),
                span(" © 2026 Example, Inc.", 0, ""),
            ],
        ),
    ];
    // Source: tokenise the fixture with a tiny scanner (tags, attributes,
    // quoted values, text); the real core does this in `decode`.
    let mut source = Vec::new();
    for line in DEMO_PAGE.lines() {
        let mut tokens: Vec<pb::SourceToken> = Vec::new();
        let t = |kind: S, text: &str| pb::SourceToken {
            kind: kind as i32,
            text: text.into(),
        };
        let mut rest = line;
        while !rest.is_empty() {
            if let Some(i) = rest.find('<') {
                if i > 0 {
                    tokens.push(t(S::SrcPlain, &rest[..i]));
                }
                let tail = &rest[i..];
                let end = tail.find('>').map(|e| e + 1).unwrap_or(tail.len());
                let tag = &tail[..end];
                if tag.starts_with("<!") {
                    tokens.push(t(S::SrcComment, tag));
                } else {
                    let (open, body) = if let Some(b) = tag.strip_prefix("</") {
                        ("</", b)
                    } else {
                        ("<", &tag[1..])
                    };
                    let body = body.trim_end_matches('>');
                    tokens.push(t(S::SrcPunct, open));
                    let mut parts = body.splitn(2, ' ');
                    tokens.push(t(S::SrcTag, parts.next().unwrap_or("")));
                    if let Some(attrs) = parts.next() {
                        for (n, a) in attrs.split(' ').enumerate() {
                            tokens.push(t(S::SrcPlain, " "));
                            match a.split_once('=') {
                                Some((k, v)) => {
                                    tokens.push(t(S::SrcAttr, k));
                                    tokens.push(t(S::SrcPunct, "="));
                                    tokens.push(t(S::SrcValue, v));
                                }
                                None => tokens.push(t(S::SrcAttr, a)),
                            }
                            let _ = n;
                        }
                    }
                    tokens.push(t(S::SrcPunct, ">"));
                }
                rest = &tail[end..];
            } else {
                let mut plain = rest;
                while let Some(a) = plain.find('&') {
                    if let Some(sc) = plain[a..].find(';') {
                        if a > 0 {
                            tokens.push(t(S::SrcPlain, &plain[..a]));
                        }
                        tokens.push(t(S::SrcEntity, &plain[a..a + sc + 1]));
                        plain = &plain[a + sc + 1..];
                        continue;
                    }
                    break;
                }
                if !plain.is_empty() {
                    tokens.push(t(S::SrcPlain, plain));
                }
                rest = "";
            }
        }
        source.push(pb::SourceLine { tokens });
    }
    pb::BodyView {
        kind: pb::BodyKind::Html as i32,
        content_type: "text/html; charset=utf-8".into(),
        content_encoding: "br".into(),
        wire_size: 2_410,
        decoded_size: DEMO_PAGE.len() as u64,
        text: DEMO_PAGE.into(),
        html,
        html_title: "Getting started · Example Docs".into(),
        source,
        ..Default::default()
    }
}

const DEMO_ROWS: &[(&str, &str, &str, u32, u64, f64, &str)] = &[
    ("GET", "api.example.com", "/v1/me", 200, 1228, 84.0, "json"),
    (
        "POST",
        "auth.example.com",
        "/oauth/token",
        401,
        196,
        142.0,
        "json",
    ),
    (
        "GET",
        "api.example.com",
        "/v1/items?page=2&per_page=20",
        200,
        18432,
        236.0,
        "json",
    ),
    (
        "GET",
        "cdn.example.com",
        "/img/banner-12.webp",
        200,
        94210,
        61.0,
        "image",
    ),
    (
        "POST",
        "payments.example.com",
        "/v1/charge",
        500,
        88,
        1840.0,
        "json",
    ),
    (
        "POST",
        "analytics.example.net",
        "/collect",
        204,
        0,
        38.0,
        "",
    ),
    ("GET", "api.example.com", "/v1/config", 304, 0, 47.0, ""),
    (
        "PUT",
        "api.example.com",
        "/v1/cart/77",
        200,
        612,
        128.0,
        "json",
    ),
    (
        "DELETE",
        "api.example.com",
        "/v1/cart/77/coupon",
        404,
        74,
        92.0,
        "json",
    ),
    ("CONNECT", "pay.example.com", "", 0, 0, 0.0, ""),
    (
        "PATCH",
        "api.example.com",
        "/v1/me/settings",
        200,
        240,
        101.0,
        "json",
    ),
    (
        "GET",
        "docs.example.com",
        "/guides/getting-started",
        200,
        7210,
        412.0,
        "html",
    ),
];

fn demo_flow(id: u64) -> pb::FlowDetail {
    let (m, h, p, status, size, ms, ct) = DEMO_ROWS[((id - 1) as usize) % DEMO_ROWS.len()];
    let jitter = ((id * 7) % 31) as f64;
    let ms = if ms > 0.0 { ms + jitter } else { 0.0 };
    let pinned = m == "CONNECT";
    let failed = status == 0 && !pinned;
    let summary = pb::FlowSummary {
        id,
        started_at_ms: 1_760_000_000_000 + id as i64 * 850,
        duration_ms: ms,
        scheme: "https".into(),
        host: h.into(),
        port: 443,
        method: m.into(),
        path: p.into(),
        status,
        protocol: if pinned {
            "tunnel".into()
        } else if id % 5 == 0 {
            "HTTP/1.1".into()
        } else {
            "h2".into()
        },
        app: ["Chromium", "curl", "firefox", "node"][(id % 4) as usize].into(),
        req_size: if m == "POST" || m == "PUT" { 96 } else { 0 },
        res_size: size,
        state: if failed {
            pb::FlowState::Failed
        } else {
            pb::FlowState::Complete
        } as i32,
        flags: if pinned { 3 } else { 0 },
        content_type: ct.into(),
        error: if pinned {
            "The client rejected the certificate (unknown_ca). This host is tunneled from now on."
                .into()
        } else {
            String::new()
        },
    };
    let fresh = id % 3 != 0;
    let timings = pb::Timings {
        dns_ms: if fresh { ms * 0.06 } else { 0.0 },
        connect_ms: if fresh { ms * 0.09 } else { 0.0 },
        tls_ms: if fresh { ms * 0.14 } else { 0.0 },
        send_ms: ms * 0.02,
        wait_ms: ms * 0.5,
        receive_ms: ms * 0.17,
        total_ms: ms,
        reused_connection: !fresh,
    };
    let body = |pairs: &[(&str, &str)]| pb::BodyView {
        kind: pb::BodyKind::Json as i32,
        content_type: "application/json".into(),
        content_encoding: "gzip".into(),
        wire_size: 180,
        decoded_size: 312,
        text: {
            let mut t = String::from("{\n");
            for (i, (k, v)) in pairs.iter().enumerate() {
                let comma = if i + 1 < pairs.len() { "," } else { "" };
                t.push_str(&format!("  \"{k}\": {v}{comma}\n"));
            }
            t.push('}');
            t
        },
        json: json_lines(pairs),
        ..Default::default()
    };
    let res_body = match (status, ct) {
        (200, "html") => demo_html_view(),
        (200, "json") if p.starts_with("/v1/items") => {
            let text = demo_order_json();
            pb::BodyView {
                kind: pb::BodyKind::Json as i32,
                content_type: "application/json".into(),
                content_encoding: "gzip".into(),
                wire_size: 3_120,
                decoded_size: text.len() as u64,
                json: json_from_pretty(&text),
                text,
                ..Default::default()
            }
        }
        (_, "image") => pb::BodyView {
            kind: pb::BodyKind::Binary as i32,
            content_type: "image/webp".into(),
            wire_size: size,
            decoded_size: size,
            note: "Binary data, 92 KB".into(),
            ..Default::default()
        },
        (401, _) => body(&[
            ("error", "\"invalid_grant\""),
            ("error_description", "\"Refresh token expired\""),
        ]),
        (500, _) => body(&[("error", "\"gateway_timeout\""), ("retryable", "true")]),
        (404, _) => body(&[("error", "\"not_found\"")]),
        (200, "json") => body(&[
            ("id", "\"u_1842\""),
            ("name", "\"Asha Rao\""),
            ("plan", "\"pro\""),
            ("verified", "true"),
            ("credits", "1499"),
            ("referrer", "null"),
        ]),
        _ => pb::BodyView::default(),
    };
    let url = format!("https://{h}{p}");
    pb::FlowDetail {
        curl: format!("curl \\\n  '{url}' \\\n  -H 'Accept: application/json' \\\n  -H 'User-Agent: okhttp/4.12.0' \\\n  --compressed"),
        url,
        request_headers: vec![
            hdr("Host", h),
            hdr("User-Agent", "okhttp/4.12.0"),
            hdr("Accept", "application/json"),
            hdr("Accept-Encoding", "gzip"),
            hdr("Authorization", "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1XzE4NDIifQ.sig"),
        ],
        response_headers: vec![
            hdr(
                "Content-Type",
                if ct == "html" {
                    "text/html; charset=utf-8"
                } else {
                    "application/json; charset=utf-8"
                },
            ),
            hdr("Content-Encoding", if ct == "html" { "br" } else { "gzip" }),
            hdr("Cache-Control", "no-store"),
            hdr("Server", "envoy"),
            hdr("X-Request-Id", "9f2c1b7e-5a04-4d1e-8a52-6e1f0c0a77d1"),
        ],
        timings: Some(timings),
        request_body: Some(if m == "POST" || m == "PUT" {
            pb::BodyView {
                kind: pb::BodyKind::Json as i32,
                content_type: "application/json".into(),
                wire_size: 96,
                decoded_size: 96,
                text: "{\n  \"amount\": 1499,\n  \"currency\": \"INR\",\n  \"method\": \"upi\"\n}".into(),
                json: json_lines(&[("amount", "1499"), ("currency", "\"INR\""), ("method", "\"upi\"")]),
                ..Default::default()
            }
        } else {
            pb::BodyView::default()
        }),
        response_body: Some(res_body),
        summary: Some(summary),
    }
}

fn demo_matches(f: &pb::FlowSummary, kind: i32, text: &str, hosts: &[String]) -> bool {
    if !hosts.is_empty() && !hosts.iter().any(|h| h.eq_ignore_ascii_case(&f.host)) {
        return false;
    }
    let kind_ok = match kind {
        1 => f.status >= 400 || f.state == pb::FlowState::Failed as i32 || f.flags & 4 != 0,
        2 => f.duration_ms >= 1000.0,
        3 => f.flags & 1 != 0,
        _ => true,
    };
    if !kind_ok {
        return false;
    }
    let t = text.to_lowercase();
    t.split_whitespace().all(|w| {
        f.host.to_lowercase().contains(w)
            || f.path.to_lowercase().contains(w)
            || f.method.to_lowercase().contains(w)
            || f.status.to_string() == w
    })
}

fn demo_decrypts(ssl: &pb::SslSettings, host: &str) -> bool {
    let matches = |list: &[String]| {
        list.iter().any(|p| {
            let p = p.to_ascii_lowercase();
            match p.strip_prefix("*.") {
                Some(suffix) => host.ends_with(&format!(".{suffix}")),
                None => p == host || p == "*",
            }
        })
    };
    if matches(&ssl.exclude) {
        return false;
    }
    ssl.mode == pb::SslMode::SslAll as i32 || matches(&ssl.hosts)
}

fn demo_rules(s: &DemoState) -> Res {
    Res::Rules(pb::RulesResponse {
        ssl: Some(s.ssl.clone()),
        map_local: s.map_local.clone(),
        pinned_hosts: s.pinned.clone(),
        rules: s.rules.clone(),
        mcp_writes: s.mcp_writes,
    })
}

fn demo_rules_v2() -> Vec<pb::Rule> {
    vec![
        pb::Rule {
            id: "r10".into(),
            name: "Checkout is down".into(),
            enabled: true,
            matcher: Some(pb::RuleMatch {
                host: "api.example.com".into(),
                path: "/v1/checkout".into(),
                method: "POST".into(),
                ..Default::default()
            }),
            action: Some(pb::rule::Action::Mock(pb::MockAction {
                status: 503,
                headers: vec![hdr("retry-after", "30")],
                body: "{\"error\":\"unavailable\"}".into(),
                delay_ms: 300,
            })),
        },
        pb::Rule {
            id: "r11".into(),
            name: "Hit staging".into(),
            enabled: false,
            matcher: Some(pb::RuleMatch {
                host: "api.example.com".into(),
                ..Default::default()
            }),
            action: Some(pb::rule::Action::MapRemote(pb::MapRemoteAction {
                scheme: "https".into(),
                host: "staging.example.com".into(),
                port: 0,
            })),
        },
        pb::Rule {
            id: "r12".into(),
            name: "Pause on login".into(),
            enabled: true,
            matcher: Some(pb::RuleMatch {
                host: "api.example.com".into(),
                path: "/v1/login".into(),
                ..Default::default()
            }),
            action: Some(pb::rule::Action::Breakpoint(pb::BreakpointAction {
                phase: pb::breakpoint_action::Phase::Request as i32,
                timeout_s: 0,
            })),
        },
    ]
}

fn demo_paused() -> Vec<pb::PausedFlow> {
    vec![pb::PausedFlow {
        flow_id: 9001,
        rule_id: "r12".into(),
        phase: pb::MessagePhase::PhaseRequest as i32,
        summary: Some(pb::FlowSummary {
            id: 9001,
            scheme: "https".into(),
            host: "api.example.com".into(),
            port: 443,
            method: "POST".into(),
            path: "/v1/login".into(),
            app: "firefox".into(),
            state: pb::FlowState::Pending as i32,
            flags: 512,
            ..Default::default()
        }),
        method: "POST".into(),
        path: "/v1/login".into(),
        status: 0,
        headers: vec![
            hdr("content-type", "application/json"),
            hdr("accept", "application/json"),
            hdr("user-agent", "Mozilla/5.0"),
        ],
        body: b"{\"email\":\"ann@example.com\",\"password\":\"correct horse\"}".to_vec(),
        since_ms: 0,
        timeout_s: 60,
    }]
}

fn demo_approvals() -> Vec<pb::Approval> {
    vec![pb::Approval {
        id: "a1".into(),
        tool: "create_mock".into(),
        title: "Mock 503 for POST api.example.com/v1/checkout".into(),
        detail: "{\n  \"host\": \"api.example.com\",\n  \"method\": \"POST\",\n  \"path\": \"/v1/checkout\",\n  \"status\": 503,\n  \"json\": { \"error\": \"unavailable\" }\n}".into(),
        requested_ms: 0,
    }]
}

fn demo_mcp(s: &DemoState) -> Res {
    Res::McpInfo(pb::McpInfo {
        enabled: true,
        url: "http://127.0.0.1:8898/mcp".into(),
        token: "3f1a9c0b7d2e4a68b5c1d0e9f8a7b6c5d4e3f2a1b0c9d8e7f6a5b4c3d2e1f0a9".into(),
        command: "claude mcp add --transport http proxy http://127.0.0.1:8898/mcp --header \"Authorization: Bearer 3f1a9c0b7d2e4a68b5c1d0e9f8a7b6c5d4e3f2a1b0c9d8e7f6a5b4c3d2e1f0a9\"".into(),
        writes: s.mcp_writes,
    })
}

impl Backend for DemoBackend {
    fn request(&self, body: Req) -> Tag {
        let tag = self.next.fetch_add(1, Ordering::Relaxed);
        let mut s = self.inner.lock().unwrap();
        let res = match body {
            Req::Search(r) => {
                let (kind, text, hosts) = r
                    .filter
                    .map(|f| (f.kind, f.text, f.hosts))
                    .unwrap_or((0, String::new(), Vec::new()));
                let all: Vec<_> = s
                    .flows
                    .iter()
                    .filter_map(|d| d.summary.as_ref())
                    .filter(|f| demo_matches(f, kind, &text, &hosts))
                    .cloned()
                    .collect();
                let limit = if r.limit == 0 { 200 } else { r.limit } as usize;
                let off = r.offset as usize;
                Res::Search(pb::SearchResponse {
                    total: all.len() as u64,
                    offset: r.offset,
                    items: all.into_iter().skip(off).take(limit).collect(),
                })
            }
            Req::GetFlow(r) => match s
                .flows
                .iter()
                .find(|d| d.summary.as_ref().map(|x| x.id) == Some(r.id))
            {
                Some(d) => Res::Flow(d.clone()),
                None => Res::Error(pb::ErrorResponse {
                    code: "not_found".into(),
                    message: "no such flow".into(),
                }),
            },
            Req::Capture(c) => {
                match c.action {
                    1 => s.recording = true,
                    2 => s.recording = false,
                    _ => {}
                }
                Res::Capture(pb::CaptureStatus {
                    recording: s.recording,
                    flows: s.flows.len() as u64,
                })
            }
            Req::Clear(_) => {
                let removed = s.flows.len() as u64;
                s.flows.clear();
                s.queue.push(Event::Batch(pb::FlowBatch {
                    upserts: vec![],
                    cleared: true,
                    total: 0,
                }));
                Res::Cleared(pb::ClearResponse { removed })
            }
            Req::Status(_) => Res::Status(pb::StatusResponse {
                version: "0.1.0".into(),
                listen: "127.0.0.1:8899".into(),
                data_dir: "~/.local/share/proxy".into(),
                uptime_ms: 42_000,
                flows: s.flows.len() as u64,
                recording: s.recording,
                pinned_hosts: vec!["pay.example.com".into()],
                insecure_upstream: false,
                core_pid: 4242,
            }),
            Req::CaInfo(_) => Res::CaInfo(pb::CaInfo {
                cert_path: "/home/you/.local/share/proxy/ca.pem".into(),
                sha256: "9f2c1b7e5a044d1e8a526e1f0c0a77d1c3b8e5f4a2d19076b8c3e4f5a6b7c8d9".into(),
                subject: "Proxy Local CA (workstation)".into(),
                not_after: "2036-10-10T00:00:00Z".into(),
                pem: String::new(),
                key_storage: "keyring".into(),
                spki_sha256: "qYb1x1vQ0n6y1p2x0cV3m3o1f9wq3b3m1k1F5pQ0v9k=".into(),
            }),
            Req::Export(_) => Res::Export(pb::ExportResponse {
                data: b"{\"log\":{\"version\":\"1.2\",\"entries\":[]}}".to_vec(),
                filename: "proxy-demo.har".into(),
                count: s.flows.len() as u32,
            }),
            Req::Subscribe(_) => Res::Subscribed(pb::SubscribeResponse {
                total: s.flows.len() as u64,
            }),
            Req::Unsubscribe(_) => Res::Unsubscribed(pb::UnsubscribeResponse {}),
            Req::Hosts(_) => {
                let mut rows: Vec<pb::HostSummary> = Vec::new();
                for f in s.flows.iter().filter_map(|d| d.summary.as_ref()) {
                    let i = match rows.iter().position(|r| r.host == f.host) {
                        Some(i) => i,
                        None => {
                            rows.push(pb::HostSummary {
                                host: f.host.clone(),
                                tunnel_only: true,
                                ..Default::default()
                            });
                            rows.len() - 1
                        }
                    };
                    let r = &mut rows[i];
                    r.flows += 1;
                    if f.status >= 400 || f.state == pb::FlowState::Failed as i32 {
                        r.errors += 1;
                    }
                    r.last_started_ms = r.last_started_ms.max(f.started_at_ms);
                    if f.flags & 2 == 0 {
                        r.tunnel_only = false;
                    }
                }
                for r in &mut rows {
                    r.pinned = s.pinned.iter().any(|p| p == &r.host);
                    r.decrypted = demo_decrypts(&s.ssl, &r.host);
                }
                rows.sort_by(|a, b| b.last_started_ms.cmp(&a.last_started_ms));
                Res::Hosts(pb::HostsResponse { hosts: rows })
            }
            Req::Rules(_) => demo_rules(&s),
            Req::SetSsl(r) => {
                s.ssl = r.ssl.unwrap_or_default();
                demo_rules(&s)
            }
            Req::SslHost(r) => {
                let h = r.host.to_ascii_lowercase();
                s.ssl.hosts.retain(|e| e != &h);
                s.ssl.exclude.retain(|e| e != &h);
                if r.enable {
                    s.pinned.retain(|p| p != &h);
                    if s.ssl.mode == pb::SslMode::SslListed as i32 {
                        s.ssl.hosts.push(h);
                    }
                } else {
                    s.ssl.exclude.push(h);
                }
                demo_rules(&s)
            }
            Req::PutMapLocal(r) => {
                if let Some(mut rule) = r.rule {
                    if rule.id.is_empty() {
                        rule.id = format!("r{}", s.next_rule);
                        s.next_rule += 1;
                    }
                    if rule.status == 0 {
                        rule.status = 200;
                    }
                    match s.map_local.iter_mut().find(|m| m.id == rule.id) {
                        Some(slot) => *slot = rule,
                        None => s.map_local.push(rule),
                    }
                }
                demo_rules(&s)
            }
            Req::RemoveRule(r) => {
                s.map_local.retain(|m| m.id != r.id);
                demo_rules(&s)
            }
            Req::Unpin(r) => {
                if r.host.is_empty() {
                    s.pinned.clear();
                } else {
                    s.pinned.retain(|p| p != &r.host);
                }
                demo_rules(&s)
            }
            Req::PutRule(r) => {
                if let Some(mut rule) = r.rule {
                    if rule.id.is_empty() {
                        rule.id = format!("r{}", s.next_rule);
                        s.next_rule += 1;
                    }
                    match s.rules.iter_mut().find(|m| m.id == rule.id) {
                        Some(slot) => *slot = rule,
                        None => s.rules.push(rule),
                    }
                }
                demo_rules(&s)
            }
            Req::SetRuleEnabled(r) => {
                for rule in s.rules.iter_mut().filter(|x| x.id == r.id) {
                    rule.enabled = r.enabled;
                }
                for m in s.map_local.iter_mut().filter(|x| x.id == r.id) {
                    m.enabled = r.enabled;
                }
                demo_rules(&s)
            }
            Req::Watch(_) | Req::Paused(_) => Res::Paused(pb::PausedList {
                items: s.paused.clone(),
            }),
            Req::Resume(r) => {
                s.paused.retain(|p| p.flow_id != r.flow_id);
                Res::Paused(pb::PausedList {
                    items: s.paused.clone(),
                })
            }
            Req::Replay(r) => Res::Replay(pb::ReplayResponse {
                flow_id: r.id + 10_000,
                status: 200,
            }),
            Req::Approvals(_) => Res::Approvals(pb::ApprovalsResponse {
                pending: s.approvals.clone(),
            }),
            Req::DecideApproval(r) => {
                s.approvals.retain(|a| a.id != r.id);
                Res::Approvals(pb::ApprovalsResponse {
                    pending: s.approvals.clone(),
                })
            }
            Req::SetMcpWrites(r) => {
                s.mcp_writes = r.writes;
                demo_mcp(&s)
            }
            Req::McpInfo(_) => demo_mcp(&s),
        };
        s.queue.push(Event::Response(tag, res));
        tag
    }

    fn drain(&self) -> Vec<Event> {
        let mut s = self.inner.lock().unwrap();
        if s.live && s.recording && s.last_tick.elapsed() > Duration::from_millis(700) {
            s.last_tick = std::time::Instant::now();
            let id = s.next_id;
            s.next_id += 1;
            let d = demo_flow(id);
            let summary = d.summary.clone().unwrap();
            s.flows.push(d);
            let total = s.flows.len() as u64;
            s.queue.push(Event::Batch(pb::FlowBatch {
                upserts: vec![summary],
                cleared: false,
                total,
            }));
        }
        std::mem::take(&mut s.queue)
    }
}

#[cfg(test)]
mod e2e {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Instant;

    fn wait_for<T>(be: &dyn Backend, mut f: impl FnMut(Event) -> Option<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            for ev in be.drain() {
                if let Some(v) = f(ev) {
                    return v;
                }
            }
            assert!(Instant::now() < deadline, "timed out waiting for the core");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// The app's backend starts a real core, sees live traffic and reads it back.
    #[test]
    fn supervises_core_and_reads_traffic() {
        let Some(proxyd) = crate::platform::find_proxyd() else {
            eprintln!("proxyd not built; skipping");
            return;
        };
        let dir = std::env::temp_dir().join(format!("proxy-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let socket = PathBuf::from(format!(
            "/run/user/{}/proxy-e2e-{}/api.sock",
            unsafe { libc::getuid() },
            std::process::id()
        ));
        std::env::set_var("PROXY_DATA_DIR", &dir);

        // A tiny origin.
        let origin = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin_port = origin.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in origin.incoming().flatten() {
                let mut s = stream;
                let mut buf = [0u8; 2048];
                let _ = s.read(&mut buf);
                let body = r#"{"hello":"world"}"#;
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            }
        });

        let be = RealBackend::start(
            CoreOptions {
                socket: socket.clone(),
                proxyd: Some(proxyd),
                listen: "127.0.0.1:0".into(),
            },
            egui::Context::default(),
        );
        wait_for(&be, |e| {
            matches!(e, Event::Conn(ConnState::Connected)).then_some(())
        });

        let tag = be.request(Req::Status(pb::StatusRequest {}));
        let listen = wait_for(&be, |e| match e {
            Event::Response(t, Res::Status(s)) if t == tag => Some(s.listen),
            _ => None,
        });

        let status = std::process::Command::new("curl")
            .args(["-s", "-o", "/dev/null", "-x", &format!("http://{listen}")])
            .arg(format!("http://127.0.0.1:{origin_port}/hello?x=1"))
            .status()
            .unwrap();
        assert!(status.success());

        // Search sees it; detail returns the decoded JSON outline.
        let mut found = None;
        for _ in 0..50 {
            let tag = be.request(Req::Search(pb::SearchRequest {
                limit: 10,
                ..Default::default()
            }));
            let r = wait_for(&be, |e| match e {
                Event::Response(t, Res::Search(s)) if t == tag => Some(s),
                _ => None,
            });
            if let Some(f) = r.items.first() {
                if f.state == pb::FlowState::Complete as i32 {
                    found = Some(f.clone());
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let f = found.expect("flow appears in search");
        assert_eq!(f.method, "GET");
        assert_eq!(f.path, "/hello?x=1");
        assert_eq!(f.status, 200);

        let tag = be.request(Req::GetFlow(pb::GetFlowRequest { id: f.id, parts: 7 }));
        let d = wait_for(&be, |e| match e {
            Event::Response(t, Res::Flow(d)) if t == tag => Some(d),
            _ => None,
        });
        assert!(d.curl.starts_with("curl"));
        let body = d.response_body.unwrap();
        assert_eq!(body.kind, pb::BodyKind::Json as i32);
        assert_eq!(body.json.len(), 3);

        drop(be);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
