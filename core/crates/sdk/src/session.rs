//! One connection from a device: TLS, hello, pairing, then calls.
//!
//! The state machine is linear: `Hello` first, then either a token that
//! checks out or the pairing exchange, then authenticated traffic. Nothing
//! that carries call data is read before authentication, and anything that
//! does not fit the state closes the connection.

use crate::assembler::Identity;
use crate::devices::{DeviceFacts, TokenCheck};
use crate::frame::{self, FrameError};
use crate::pairing::{self, Decision, PendingPrompt, PromptError};
use crate::service::{GateGuard, Inner, PromptPolicy, SessionEntry};
use crate::wire::{self, envelope::Msg, hello_ack::State, Envelope};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch, Notify};

/// Why a session ended.
enum End {
    /// Normal, or not worth an error frame.
    Quiet(&'static str),
    /// Tell the peer, count it, log it.
    Violation(String),
    Frame(FrameError),
    Error(wire::error::Code, String),
}

type Result<T> = std::result::Result<T, End>;

struct Conn {
    inner: Arc<Inner>,
    out: mpsc::Sender<Envelope>,
    rx: mpsc::Receiver<std::result::Result<Envelope, FrameError>>,
    kill: Arc<Notify>,
    stop: watch::Receiver<bool>,
}

impl Conn {
    fn send(&self, msg: Msg) -> Result<()> {
        self.out
            .try_send(Envelope { msg: Some(msg) })
            .map_err(|_| End::Quiet("peer is not reading fast enough"))
    }

    /// Next message, or the reason the session must end.
    async fn recv(&mut self, within: Duration) -> Result<Msg> {
        tokio::select! {
            biased;
            _ = self.kill.notified() => Err(End::Quiet("closed by the desktop")),
            _ = crate::service::wait_true(&mut self.stop) => Err(End::Quiet("ingest was turned off")),
            r = tokio::time::timeout(within, self.rx.recv()) => match r {
                Err(_) => Err(End::Quiet("timed out")),
                Ok(None) => Err(End::Quiet("connection closed")),
                Ok(Some(Err(FrameError::Closed))) => Err(End::Quiet("connection closed")),
                Ok(Some(Err(FrameError::Slow))) => Err(End::Quiet("idle or too slow")),
                Ok(Some(Err(e))) => Err(End::Frame(e)),
                Ok(Some(Ok(env))) => Ok(env.msg.expect("frame::decode rejects empty envelopes")),
            },
        }
    }
}

struct Bucket {
    tokens: f64,
    cap: f64,
    last: Instant,
}

impl Bucket {
    fn new(per_sec: u32) -> Bucket {
        Bucket {
            tokens: per_sec as f64,
            cap: per_sec as f64,
            last: Instant::now(),
        }
    }
    fn take(&mut self) -> bool {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + dt * self.cap).min(self.cap);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

fn violation<T>(why: impl Into<String>) -> Result<T> {
    Err(End::Violation(why.into()))
}

fn clip(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn valid_id(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '@'))
}

pub(crate) async fn run(
    inner: Arc<Inner>,
    stream: TcpStream,
    peer: SocketAddr,
    acceptor: tokio_rustls::TlsAcceptor,
    guard: GateGuard,
    stop: watch::Receiver<bool>,
) {
    let limits = inner.opts.limits.clone();
    let tls = match tokio::time::timeout(limits.handshake_timeout, acceptor.accept(stream)).await {
        Ok(Ok(t)) => t,
        _ => return,
    };
    let (mut rd, mut wr) = tokio::io::split(tls);

    let (out_tx, mut out_rx) = mpsc::channel::<Envelope>(limits.outbound_queue);
    let writer = tokio::spawn(async move {
        while let Some(env) = out_rx.recv().await {
            let buf = frame::encode(&env);
            let ok = tokio::time::timeout(Duration::from_secs(10), async {
                wr.write_all(&buf).await?;
                wr.flush().await
            })
            .await;
            if !matches!(ok, Ok(Ok(()))) {
                break;
            }
        }
        let _ = wr.shutdown().await;
    });

    let (in_tx, in_rx) = mpsc::channel(256);
    let (max_frame, idle, frame_t) = (limits.max_frame, limits.idle_timeout, limits.frame_timeout);
    let reader = tokio::spawn(async move {
        loop {
            let r = frame::read_envelope(&mut rd, max_frame, Some(idle), frame_t).await;
            let fatal = r.is_err();
            if in_tx.send(r).await.is_err() || fatal {
                break;
            }
        }
    });

    let kill = Arc::new(Notify::new());
    let mut conn = Conn {
        inner: inner.clone(),
        out: out_tx,
        rx: in_rx,
        kill: kill.clone(),
        stop,
    };

    let mut registered: Option<(u64, Identity)> = None;
    let result = session(&mut conn, peer, guard, &kill, &mut registered).await;

    let (who_str, ended) = match &registered {
        Some((_, w)) => (format!("{}/{}", w.device_id, w.app_id), true),
        None => (peer.ip().to_string(), false),
    };
    match result {
        Ok(()) => {}
        Err(End::Quiet(why)) => tracing::debug!("sdk session {who_str} ended: {why}"),
        Err(End::Violation(why)) => {
            inner
                .stats()
                .protocol_violations
                .fetch_add(1, Ordering::Relaxed);
            tracing::warn!("sdk session {who_str} closed: protocol violation: {why}");
            let _ = conn.send(Msg::Error(wire::Error {
                code: wire::error::Code::ProtocolViolation as i32,
                message: why,
                ..Default::default()
            }));
        }
        Err(End::Frame(e)) => {
            inner
                .stats()
                .protocol_violations
                .fetch_add(1, Ordering::Relaxed);
            tracing::warn!("sdk session {who_str} closed: bad frame: {e}");
            let code = match e {
                FrameError::TooLarge(..) => wire::error::Code::FrameTooLarge,
                _ => wire::error::Code::ProtocolViolation,
            };
            let _ = conn.send(Msg::Error(wire::Error {
                code: code as i32,
                message: e.to_string(),
                ..Default::default()
            }));
        }
        Err(End::Error(code, message)) => {
            tracing::info!("sdk session {who_str} refused: {message}");
            let _ = conn.send(Msg::Error(wire::Error {
                code: code as i32,
                message,
                ..Default::default()
            }));
        }
    }

    if let Some((id, who)) = registered {
        let table = {
            let mut s = inner.sessions.lock().unwrap();
            let entry = s.remove(&id);
            entry.map(|e| e.table)
        };
        if let Some(t) = table {
            t.lock().unwrap().abandon_fetches();
        }
        inner.schedule_grace(who.device_id.clone(), who.app_id.clone());
    }
    if ended {
        inner.notify();
    }
    // Let a final Error frame go out, then close.
    drop(conn);
    let _ = tokio::time::timeout(Duration::from_secs(2), writer).await;
    reader.abort();
}

async fn session(
    conn: &mut Conn,
    peer: SocketAddr,
    guard: GateGuard,
    kill: &Arc<Notify>,
    registered: &mut Option<(u64, Identity)>,
) -> Result<()> {
    let inner = conn.inner.clone();
    let limits = inner.opts.limits.clone();

    // ---- Hello
    let hello = match conn.recv(limits.handshake_timeout).await? {
        Msg::Hello(h) => h,
        _ => return violation("the first message must be Hello"),
    };
    if hello.protocol_version < wire::MIN_PROTOCOL_VERSION
        || hello.protocol_version > wire::PROTOCOL_VERSION
    {
        conn.send(Msg::Error(wire::Error {
            code: wire::error::Code::UnsupportedVersion as i32,
            message: format!(
                "protocol version {} is not supported; this desktop speaks {} to {}. Update the desktop app or the SDK.",
                hello.protocol_version,
                wire::MIN_PROTOCOL_VERSION,
                wire::PROTOCOL_VERSION
            ),
            min_protocol_version: wire::MIN_PROTOCOL_VERSION,
            max_protocol_version: wire::PROTOCOL_VERSION,
        }))?;
        return Err(End::Quiet("unsupported protocol version"));
    }
    if !valid_id(&hello.device_install_id, 64) || !valid_id(&hello.app_id, 256) {
        return violation("device_install_id or app_id is missing or malformed");
    }
    let f = limits.max_field_len;
    let who = Identity {
        device_id: hello.device_install_id.clone(),
        app_id: hello.app_id.clone(),
        device_name: clip(&hello.device_name, f),
        device_model: clip(&hello.device_model, f),
        os_version: clip(&hello.os_version, f),
        app_name: clip(&hello.app_name, f),
        app_version: clip(&hello.app_version, f),
        build: clip(&hello.build, f),
        sdk_name: clip(&hello.sdk_name, 16),
        sdk_version: clip(&hello.sdk_version, 32),
        connection: connection_kind(&hello.connection, &peer),
    };
    let facts = DeviceFacts {
        device_id: who.device_id.clone(),
        app_id: who.app_id.clone(),
        device_name: who.device_name.clone(),
        device_model: who.device_model.clone(),
        os_version: who.os_version.clone(),
        app_name: who.app_name.clone(),
        app_version: who.app_version.clone(),
        sdk_name: who.sdk_name.clone(),
        sdk_version: who.sdk_version.clone(),
    };
    let now = Inner::now_ms();
    let initial_offset = now - hello.device_time_ms;
    let session_id = format!(
        "{:016x}",
        pairing::random_bytes::<8>()
            .iter()
            .fold(0u64, |a, b| a << 8 | *b as u64)
    );
    let ack = |state: State, retry: u32, message: &str| {
        Msg::HelloAck(wire::HelloAck {
            state: state as i32,
            desktop_name: inner.opts.desktop_name.clone(),
            session_id: session_id.clone(),
            server_time_ms: Inner::now_ms(),
            retry_after_s: retry,
            message: message.into(),
        })
    };

    if !inner.is_enabled() {
        conn.send(ack(
            State::IngestDisabled,
            30,
            "SDK connections are turned off on the desktop",
        ))?;
        return Err(End::Quiet("ingest disabled"));
    }

    // ---- Token or pairing
    let check = inner
        .devices
        .check_token(&who.device_id, &who.app_id, &hello.token)
        .unwrap_or(TokenCheck::Unpaired);
    match check {
        TokenCheck::Valid => conn.send(ack(State::Authenticated, 0, ""))?,
        TokenCheck::Denied => {
            conn.send(ack(
                State::Denied,
                3600,
                "this app was denied on the desktop",
            ))?;
            return Err(End::Quiet("denied"));
        }
        TokenCheck::Invalid | TokenCheck::Unpaired => {
            conn.send(ack(State::PairingRequired, 0, ""))?;
            pair(conn, &who, &facts, &peer, check == TokenCheck::Invalid).await?;
        }
    }
    // ---- Authenticated
    let id = inner.next_session_id();
    {
        let mut sessions = inner.sessions.lock().unwrap();
        let mut superseded = Vec::new();
        for s in sessions.values() {
            if s.who.device_id == who.device_id && s.who.app_id == who.app_id {
                superseded.push(s.kill.clone());
            }
        }
        let others = sessions.len() - superseded.len();
        if others >= limits.max_sessions {
            return Err(End::Error(
                wire::error::Code::TooManySessions,
                format!(
                    "the desktop already has {} SDK sessions",
                    limits.max_sessions
                ),
            ));
        }
        for k in superseded {
            k.notify_one();
        }
        let table = inner.table_for(&who.device_id, &who.app_id);
        let offset_ms = Arc::new(AtomicI64::new(initial_offset));
        sessions.insert(
            id,
            SessionEntry {
                who: who.clone(),
                remote_ip: peer.ip().to_string(),
                out: conn.out.clone(),
                kill: kill.clone(),
                table,
                dropped: Arc::new(AtomicU64::new(0)),
                offset_ms,
            },
        );
    }
    *registered = Some((id, who.clone()));
    drop(guard);
    inner.note_ip(&peer.ip().to_string(), &who.device_id);
    let _ = inner.devices.touch(&facts, Inner::now_ms());
    let (table, dropped, offset) = {
        let s = inner.sessions.lock().unwrap();
        let e = &s[&id];
        (e.table.clone(), e.dropped.clone(), e.offset_ms.clone())
    };
    conn.send(Msg::Config(
        inner
            .effective_config(&who.device_id, &who.app_id)
            .to_wire(),
    ))?;
    let mut last_ping: i64 = Inner::now_ms();
    conn.send(Msg::Ping(wire::Ping { time_ms: last_ping }))?;
    inner.notify();
    tracing::info!(
        "sdk session {}/{} connected from {}",
        who.device_id,
        who.app_id,
        peer.ip()
    );
    if let Some(tap) = &inner.opts.tap {
        tap.event(
            "session_start",
            serde_json::json!({"device": who.device_id, "app": who.app_id, "sdk": who.sdk_name,
                "sdk_version": who.sdk_version, "device_name": who.device_name,
                "app_version": who.app_version, "connection": who.connection}),
        );
    }

    let mut bucket = Bucket::new(limits.max_events_per_sec);
    let mut overloaded_since: Option<Instant> = None;
    let mut ping_timer = tokio::time::interval(limits.ping_interval);
    ping_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ping_timer.tick().await;
    let mut sweep_timer = tokio::time::interval(Duration::from_secs(30));
    sweep_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    sweep_timer.tick().await;
    let mut last_in = Instant::now();

    loop {
        let msg = tokio::select! {
            biased;
            _ = ping_timer.tick() => {
                if last_in.elapsed() >= limits.ping_interval {
                    last_ping = Inner::now_ms();
                    conn.send(Msg::Ping(wire::Ping { time_ms: last_ping }))?;
                }
                continue;
            }
            _ = sweep_timer.tick() => {
                let mut t = table.lock().unwrap();
                inner.env.sweep(&mut t, Duration::from_secs(300));
                continue;
            }
            m = conn.recv(limits.idle_timeout + Duration::from_secs(5)) => m?,
        };
        last_in = Instant::now();
        inner.stats().events.fetch_add(1, Ordering::Relaxed);

        // Control messages are cheap and never rate limited.
        match &msg {
            Msg::Ping(p) => {
                conn.send(Msg::Pong(wire::Pong {
                    echo_ms: p.time_ms,
                    time_ms: Inner::now_ms(),
                }))?;
                continue;
            }
            Msg::Pong(p) => {
                if p.echo_ms == last_ping {
                    let t3 = Inner::now_ms();
                    offset.store((last_ping + t3) / 2 - p.time_ms, Ordering::Relaxed);
                }
                continue;
            }
            _ => {}
        }
        if !bucket.take() {
            dropped.fetch_add(1, Ordering::Relaxed);
            inner.stats().dropped_events.fetch_add(1, Ordering::Relaxed);
            let since = *overloaded_since.get_or_insert_with(Instant::now);
            if since.elapsed() > Duration::from_secs(5) {
                return Err(End::Error(
                    wire::error::Code::RateLimited,
                    "sending events faster than the desktop accepts".into(),
                ));
            }
            continue;
        }
        overloaded_since = None;

        if let Some(tap) = &inner.opts.tap {
            tap.received(&who, &msg);
        }
        let off = offset.load(Ordering::Relaxed);
        let env = &inner.env;
        match msg {
            Msg::CallStart(m) => env.call_start(&mut table.lock().unwrap(), &who, off, m),
            Msg::BodyChunk(m) => env.body_chunk(&mut table.lock().unwrap(), m),
            Msg::ResponseStart(m) => env.response_start(&mut table.lock().unwrap(), m),
            Msg::CallComplete(m) => env.call_complete(&mut table.lock().unwrap(), off, m),
            Msg::CallFailed(m) => env.call_failed(&mut table.lock().unwrap(), off, m),
            Msg::BodyFetchResult(m) => env.body_fetch_result(&mut table.lock().unwrap(), m),
            Msg::Hello(_) => return violation("duplicate Hello"),
            _ => return violation("message is not valid from a device in this state"),
        }
    }
}

/// The pairing exchange. Returns when the person allowed it.
async fn pair(
    conn: &mut Conn,
    who: &Identity,
    facts: &DeviceFacts,
    peer: &SocketAddr,
    repair: bool,
) -> Result<()> {
    let inner = conn.inner.clone();
    let limits = inner.opts.limits.clone();
    let commit = match conn.recv(limits.handshake_timeout).await? {
        Msg::PairRequest(r) => r.commit,
        _ => return violation("expected PairRequest"),
    };
    if commit.len() != 32 {
        return violation("commit must be 32 bytes");
    }
    let server_nonce = pairing::random_bytes::<{ pairing::NONCE_LEN }>();
    conn.send(Msg::PairChallenge(wire::PairChallenge {
        server_nonce: server_nonce.to_vec(),
        timeout_s: limits.pair_timeout.as_secs() as u32,
    }))?;
    let client_nonce = match conn.recv(limits.handshake_timeout).await? {
        Msg::PairReveal(r) => r.client_nonce,
        _ => return violation("expected PairReveal"),
    };
    if !pairing::verify_commit(&commit, &client_nonce) {
        return violation("the revealed nonce does not match the commitment");
    }
    let fp = inner.cert.lock().unwrap().fingerprint();
    let code = pairing::pairing_code(&fp, &client_nonce, &server_nonce);

    // A connection that comes from this computer (adb reverse, an emulator, a simulator) is the person's own
    // device or process: no prompt. The commit-reveal exchange above still ran, so the protocol is unchanged.
    if peer.ip().is_loopback() && inner.settings.lock().unwrap().auto_accept_local {
        let token = pairing::random_bytes::<32>();
        if inner.devices.pair(facts, &token, Inner::now_ms()).is_err() {
            return Err(End::Quiet("could not store the pairing"));
        }
        tracing::info!(
            "sdk pairing from {} ({}) accepted automatically: it came from this computer",
            who.device_name,
            who.app_id
        );
        if let Some(tap) = &inner.opts.tap {
            tap.event(
                "auto_accepted",
                serde_json::json!({"device": who.device_id, "app": who.app_id}),
            );
        }
        conn.send(Msg::PairResult(wire::PairResult {
            allowed: true,
            token: token.to_vec(),
            message: String::new(),
        }))?;
        inner.notify();
        return Ok(());
    }

    let now = Inner::now_ms();
    let prompt = PendingPrompt {
        device_id: who.device_id.clone(),
        app_id: who.app_id.clone(),
        device_name: who.device_name.clone(),
        device_model: who.device_model.clone(),
        os_version: who.os_version.clone(),
        app_name: who.app_name.clone(),
        sdk_name: who.sdk_name.clone(),
        sdk_version: who.sdk_version.clone(),
        connection: who.connection.clone(),
        remote_ip: peer.ip().to_string(),
        code: code.clone(),
        created_ms: now,
        expires_ms: now + limits.pair_timeout.as_millis() as i64,
        repair,
        id: 0,
    };
    let (pid, mut decision) = match inner.prompts.open(prompt, &limits) {
        Ok(x) => x,
        Err(e) => {
            let message = match e {
                PromptError::Pending => {
                    "a pairing request from this app is already waiting on the desktop"
                }
                PromptError::Cooldown => "this app was denied a moment ago; try again shortly",
                PromptError::TooMany => "too many pairing requests are waiting on the desktop",
            };
            conn.send(Msg::PairResult(wire::PairResult {
                allowed: false,
                token: Vec::new(),
                message: message.into(),
            }))?;
            return Err(End::Quiet("pairing prompt refused"));
        }
    };
    tracing::info!(
        "pairing request from {} ({}, {}): code {} {}",
        who.device_name,
        who.device_model,
        who.app_id,
        &code[..3],
        &code[3..]
    );
    inner.notify();
    if let Some(tap) = &inner.opts.tap {
        tap.event(
            "pair_prompt",
            serde_json::json!({"device": who.device_id, "app": who.app_id, "code": code}),
        );
    }

    match inner.opts.prompt_policy {
        PromptPolicy::Manual => {}
        PromptPolicy::AutoAllow => {
            eprintln!(
                "pairing code for {} ({}): {} {}",
                who.device_name,
                who.app_id,
                &code[..3],
                &code[3..]
            );
            inner.prompts.decide(pid, Decision::Allow, &limits);
        }
        PromptPolicy::AutoDeny => {
            inner.prompts.decide(pid, Decision::Deny, &limits);
        }
    }

    // Wait for the person. The device may ping meanwhile.
    let deadline = tokio::time::Instant::now() + limits.pair_timeout;
    let outcome = loop {
        tokio::select! {
            biased;
            d = &mut decision => break d.ok(),
            _ = tokio::time::sleep_until(deadline) => break None,
            m = conn.recv(limits.pair_timeout) => match m {
                Ok(Msg::Ping(p)) => {
                    let _ = conn.send(Msg::Pong(wire::Pong { echo_ms: p.time_ms, time_ms: Inner::now_ms() }));
                }
                Ok(Msg::Pong(_)) => {}
                Ok(_) => {
                    inner.prompts.close(pid, false, &limits);
                    inner.notify();
                    return violation("unexpected message while waiting for the person to answer");
                }
                Err(e) => {
                    inner.prompts.close(pid, false, &limits);
                    inner.notify();
                    return Err(e);
                }
            },
        }
    };
    let result = match outcome {
        Some(Decision::Allow) => {
            let token = pairing::random_bytes::<32>();
            if inner.devices.pair(facts, &token, Inner::now_ms()).is_err() {
                inner.notify();
                return Err(End::Quiet("could not store the pairing"));
            }
            conn.send(Msg::PairResult(wire::PairResult {
                allowed: true,
                token: token.to_vec(),
                message: String::new(),
            }))?;
            Ok(())
        }
        Some(Decision::Deny) => {
            let _ = inner.devices.deny(facts, Inner::now_ms());
            let _ = conn.send(Msg::PairResult(wire::PairResult {
                allowed: false,
                token: Vec::new(),
                message: "denied on the desktop".into(),
            }));
            Err(End::Quiet("pairing denied"))
        }
        None => {
            inner.prompts.close(pid, true, &limits);
            let _ = conn.send(Msg::PairResult(wire::PairResult {
                allowed: false,
                token: Vec::new(),
                message: "nobody answered on the desktop in time".into(),
            }));
            Err(End::Quiet("pairing timed out"))
        }
    };
    inner.notify();
    result
}

fn connection_kind(hint: &str, peer: &SocketAddr) -> String {
    match hint {
        "emulator" | "usb" | "wifi" | "simulator" => hint.to_string(),
        _ if peer.ip().is_loopback() => "usb".into(),
        _ => "wifi".into(),
    }
}
