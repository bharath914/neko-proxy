//! The ingest service: owns the listener, the device registry, pairing
//! prompts and live sessions, and is the handle the rest of the core uses.

use crate::adb::{AdbStatus, AdbWatcher};
use crate::assembler::{CallTable, Env, FetchOutcome, Identity, Observer, Stats, StatsSnapshot};
use crate::cert::IngestCert;
use crate::config::DeviceConfig;
use crate::devices::{DeviceFacts, Devices, PairState};
use crate::discovery::{Advertiser, DiscoveryStatus};
use crate::limits::Limits;
use crate::merge::{DeviceLookup, Merger};
use crate::pairing::{Decision, PendingPrompt, Prompts};
use crate::wire::{self, body_chunk::Part, envelope::Msg, Envelope};
use flow::{FlowId, FlowRecord, FlowSink, IdGen, Source};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use store::{BodyPart, Store};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch, Notify};

/// 8898 is taken by the MCP server, see docs/sdk/docs/DEVIATIONS.md.
pub const DEFAULT_PORT: u16 = 8897;

#[derive(Debug, thiserror::Error)]
pub enum SdkError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("device registry: {0}")]
    Devices(#[from] crate::devices::DevicesError),
    #[error("tls: {0}")]
    Tls(#[from] rustls::Error),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FetchError {
    #[error("no such flow")]
    NotFound,
    #[error("this flow was not captured by the SDK")]
    NotSdk,
    #[error("the device is not connected")]
    Offline,
    #[error("the device did not answer in time")]
    Timeout,
    #[error("the device connection was lost")]
    Lost,
}

/// What happens to a pairing prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptPolicy {
    /// Wait for a person (the app, or the CLI).
    Manual,
    /// Approve at once (`sdk-test-core --auto-allow`).
    AutoAllow,
    AutoDeny,
}

/// Sees every message a session delivers, after authentication. For tests
/// and recording.
pub trait Tap: Send + Sync + 'static {
    fn received(&self, who: &Identity, msg: &Msg);
    /// Session-level happenings: `session_start`, `pair_prompt`. Default: ignored.
    fn event(&self, _name: &str, _detail: serde_json::Value) {}
}

pub struct SdkOptions {
    pub data_dir: PathBuf,
    pub port: u16,
    pub bind: IpAddr,
    pub desktop_name: String,
    pub limits: Limits,
    pub advertise: bool,
    pub watch_adb: bool,
    pub prompt_policy: PromptPolicy,
    /// Pushed to every device instead of its stored config.
    pub config_override: Option<DeviceConfig>,
    pub tap: Option<Arc<dyn Tap>>,
}

impl SdkOptions {
    pub fn new(data_dir: PathBuf) -> SdkOptions {
        SdkOptions {
            data_dir,
            port: DEFAULT_PORT,
            bind: IpAddr::from([0, 0, 0, 0]),
            desktop_name: hostname(),
            limits: Limits::default(),
            advertise: true,
            watch_adb: true,
            prompt_policy: PromptPolicy::Manual,
            config_override: None,
            tap: None,
        }
    }
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "desktop".into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnStatus {
    Connected,
    Idle,
    Denied,
    Revoked,
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub facts: DeviceFacts,
    pub status: ConnStatus,
    /// "emulator", "usb", "wifi", "simulator" or empty.
    pub connection: String,
    pub remote_ip: String,
    pub last_seen_ms: i64,
    pub flows: u64,
    pub dropped: u64,
    pub config: DeviceConfig,
}

/// Everything the Devices screen shows. Replaced as a whole on any change.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub rev: u64,
    pub enabled: bool,
    pub devices: Vec<DeviceInfo>,
    pub pending: Vec<PendingPrompt>,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub enabled: bool,
    pub port: u16,
    pub fingerprint: String,
    pub desktop_name: String,
    pub listening: Vec<String>,
    pub adb: AdbStatus,
    pub discovery: DiscoveryStatus,
    pub sessions: usize,
    pub stats: StatsSnapshot,
}

pub(crate) struct SessionEntry {
    pub who: Identity,
    pub remote_ip: String,
    pub out: mpsc::Sender<Envelope>,
    pub kill: Arc<Notify>,
    pub table: Arc<Mutex<CallTable>>,
    pub dropped: Arc<AtomicU64>,
    pub offset_ms: Arc<AtomicI64>,
}

#[derive(Default)]
struct Gate {
    total: usize,
    per_ip: HashMap<IpAddr, usize>,
}

/// Holds a slot for a connection that has not authenticated.
pub(crate) struct GateGuard {
    inner: Arc<Inner>,
    ip: IpAddr,
}

impl Drop for GateGuard {
    fn drop(&mut self) {
        let mut g = self.inner.gate.lock().unwrap();
        g.total = g.total.saturating_sub(1);
        if let Some(n) = g.per_ip.get_mut(&self.ip) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                g.per_ip.remove(&self.ip);
            }
        }
    }
}

type FinishedHook = Arc<dyn Fn(&Arc<FlowRecord>) + Send + Sync>;

struct Running {
    stop: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
    addrs: Vec<SocketAddr>,
    _advertiser: Option<Advertiser>,
}

pub(crate) struct Inner {
    pub opts: SdkOptions,
    pub cert: Mutex<IngestCert>,
    pub devices: Devices,
    pub prompts: Prompts,
    pub env: Env,
    pub sessions: Mutex<HashMap<u64, SessionEntry>>,
    pub tables: Mutex<HashMap<(String, String), Arc<Mutex<CallTable>>>>,
    counts: Mutex<HashMap<(String, String), u64>>,
    gate: Mutex<Gate>,
    next_session: AtomicU64,
    enabled: AtomicBool,
    running: Mutex<Option<Running>>,
    changed: watch::Sender<Arc<Snapshot>>,
    rev: AtomicU64,
    recent_ips: Mutex<HashMap<String, HashMap<String, Instant>>>,
    merger: Merger,
    hooks: Mutex<Vec<FinishedHook>>,
    adb: Mutex<Option<AdbWatcher>>,
}

#[derive(Clone)]
pub struct Sdk {
    pub(crate) inner: Arc<Inner>,
}

struct Lookup(std::sync::Weak<Inner>);

impl DeviceLookup for Lookup {
    fn device_for_ip(&self, ip: &str) -> Option<String> {
        let inner = self.0.upgrade()?;
        let seen = inner.recent_ips.lock().unwrap();
        let devs = seen.get(ip)?;
        let live: Vec<&String> = devs
            .iter()
            .filter(|(_, t)| t.elapsed() < Duration::from_secs(600))
            .map(|(d, _)| d)
            .collect();
        (live.len() == 1).then(|| live[0].clone())
    }
}

struct Obs(std::sync::Weak<Inner>);

impl Observer for Obs {
    fn finished(&self, rec: &Arc<FlowRecord>) {
        if let Some(inner) = self.0.upgrade() {
            if let Some(i) = &rec.sdk {
                *inner
                    .counts
                    .lock()
                    .unwrap()
                    .entry((i.device_id.clone(), i.app_id.clone()))
                    .or_default() += 1;
            }
            inner.merger.observe(rec);
            let hooks: Vec<FinishedHook> = inner.hooks.lock().unwrap().clone();
            for h in hooks {
                h(rec);
            }
            inner.notify();
        }
    }
}

impl Inner {
    pub(crate) fn now_ms() -> i64 {
        crate::assembler::unix_ms()
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub(crate) fn next_session_id(&self) -> u64 {
        self.next_session.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub(crate) fn enter_gate(self: &Arc<Self>, ip: IpAddr) -> Option<GateGuard> {
        let mut g = self.gate.lock().unwrap();
        let l = &self.opts.limits;
        if g.total >= l.max_handshaking
            || g.per_ip.get(&ip).copied().unwrap_or(0) >= l.max_handshaking_per_ip
        {
            return None;
        }
        g.total += 1;
        *g.per_ip.entry(ip).or_default() += 1;
        Some(GateGuard {
            inner: self.clone(),
            ip,
        })
    }

    pub(crate) fn table_for(&self, device: &str, app: &str) -> Arc<Mutex<CallTable>> {
        self.tables
            .lock()
            .unwrap()
            .entry((device.to_string(), app.to_string()))
            .or_default()
            .clone()
    }

    pub(crate) fn note_ip(&self, ip: &str, device: &str) {
        self.recent_ips
            .lock()
            .unwrap()
            .entry(ip.to_string())
            .or_default()
            .insert(device.to_string(), Instant::now());
    }

    pub(crate) fn effective_config(&self, device: &str, app: &str) -> DeviceConfig {
        match &self.opts.config_override {
            Some(c) => c.clone().sanitized(),
            None => self.devices.config(device, app).unwrap_or_default(),
        }
    }

    pub(crate) fn stats(&self) -> &Arc<Stats> {
        &self.env.stats
    }

    /// Rebuild and publish the snapshot.
    pub(crate) fn notify(&self) {
        let rev = self.rev.fetch_add(1, Ordering::Relaxed) + 1;
        let snap = self.build_snapshot(rev);
        self.changed.send_replace(Arc::new(snap));
    }

    fn build_snapshot(&self, rev: u64) -> Snapshot {
        let rows = self.devices.list().unwrap_or_default();
        let sessions = self.sessions.lock().unwrap();
        let counts = self.counts.lock().unwrap();
        let mut devices = Vec::with_capacity(rows.len());
        for r in rows {
            let key = (r.facts.device_id.clone(), r.facts.app_id.clone());
            let live = sessions
                .values()
                .find(|s| s.who.device_id == key.0 && s.who.app_id == key.1);
            let status = match (live.is_some(), r.state) {
                (_, PairState::Denied) => ConnStatus::Denied,
                (_, PairState::Revoked) => ConnStatus::Revoked,
                (true, _) => ConnStatus::Connected,
                (false, _) => ConnStatus::Idle,
            };
            devices.push(DeviceInfo {
                connection: live.map(|s| s.who.connection.clone()).unwrap_or_default(),
                remote_ip: live.map(|s| s.remote_ip.clone()).unwrap_or_default(),
                dropped: live.map(|s| s.dropped.load(Ordering::Relaxed)).unwrap_or(0),
                flows: counts.get(&key).copied().unwrap_or(0),
                last_seen_ms: if live.is_some() {
                    Self::now_ms()
                } else {
                    r.last_seen_ms
                },
                config: r.config,
                facts: r.facts,
                status,
            });
        }
        Snapshot {
            rev,
            enabled: self.enabled.load(Ordering::Relaxed),
            devices,
            pending: self.prompts.list(),
        }
    }

    /// Called when a session ends: after the grace period, calls still open
    /// are marked incomplete unless the device came back.
    pub(crate) fn schedule_grace(self: &Arc<Self>, device: String, app: String) {
        let me = self.clone();
        let grace = self.opts.limits.grace;
        tokio::spawn(async move {
            tokio::time::sleep(grace).await;
            let back = me
                .sessions
                .lock()
                .unwrap()
                .values()
                .any(|s| s.who.device_id == device && s.who.app_id == app);
            if back {
                return;
            }
            let table = me.table_for(&device, &app);
            let mut t = table.lock().unwrap();
            let n = me
                .env
                .fail_open_calls(&mut t, "connection to the app was lost");
            if n > 0 {
                tracing::info!("{n} calls from {device}/{app} marked incomplete");
            }
        });
    }
}

impl Sdk {
    /// Create the service, disabled. Nothing listens until [`Sdk::set_enabled`].
    pub fn new(
        opts: SdkOptions,
        sink: Arc<dyn FlowSink>,
        store: Store,
        ids: Arc<IdGen>,
    ) -> Result<Sdk, SdkError> {
        std::fs::create_dir_all(&opts.data_dir)?;
        let cert = IngestCert::load_or_create(&opts.data_dir)?;
        let devices = Devices::open(&opts.data_dir.join("sdk-devices.db"))?;
        let counts = store
            .devices()
            .unwrap_or_default()
            .into_iter()
            .map(|r| ((r.device_id, r.app_id), r.flows))
            .collect();
        let (changed, _) = watch::channel(Arc::new(Snapshot::default()));
        let limits = opts.limits.clone();
        let inner = Arc::new_cyclic(|weak: &std::sync::Weak<Inner>| {
            let env = Env {
                ids,
                sink: sink.clone(),
                store: store.clone(),
                limits,
                stats: Arc::new(Stats::default()),
                observer: Some(Arc::new(Obs(weak.clone()))),
            };
            Inner {
                merger: Merger::new(Arc::new(Lookup(weak.clone())), sink, store),
                opts,
                cert: Mutex::new(cert),
                devices,
                prompts: Prompts::default(),
                env,
                sessions: Mutex::new(HashMap::new()),
                tables: Mutex::new(HashMap::new()),
                counts: Mutex::new(counts),
                gate: Mutex::new(Gate::default()),
                next_session: AtomicU64::new(0),
                enabled: AtomicBool::new(false),
                running: Mutex::new(None),
                changed,
                rev: AtomicU64::new(0),
                recent_ips: Mutex::new(HashMap::new()),
                hooks: Mutex::new(Vec::new()),
                adb: Mutex::new(None),
            }
        });
        inner.notify();
        Ok(Sdk { inner })
    }

    /// Start or stop accepting SDK connections. Stopping closes every session.
    pub async fn set_enabled(&self, on: bool) -> Result<(), SdkError> {
        let inner = &self.inner;
        if on == inner.enabled.load(Ordering::Relaxed)
            && on == inner.running.lock().unwrap().is_some()
        {
            return Ok(());
        }
        if on {
            let tls = inner.cert.lock().unwrap().server_config()?;
            let acceptor = tokio_rustls::TlsAcceptor::from(tls);
            let listener =
                TcpListener::bind(SocketAddr::new(inner.opts.bind, inner.opts.port)).await?;
            let local = listener.local_addr()?;
            let addrs = listening_addrs(local);
            let advertiser = if inner.opts.advertise {
                Some(Advertiser::start(
                    local.port(),
                    &inner.cert.lock().unwrap().fingerprint_short(),
                    &inner.opts.desktop_name,
                ))
            } else {
                None
            };
            if inner.opts.watch_adb {
                *inner.adb.lock().unwrap() = Some(AdbWatcher::start(local.port()));
            }
            let (stop_tx, stop_rx) = watch::channel(false);
            let me = inner.clone();
            let task = tokio::spawn(accept_loop(me, listener, acceptor, stop_rx));
            *inner.running.lock().unwrap() = Some(Running {
                stop: stop_tx,
                task,
                addrs,
                _advertiser: advertiser,
            });
            inner.enabled.store(true, Ordering::Relaxed);
        } else {
            inner.enabled.store(false, Ordering::Relaxed);
            let running = inner.running.lock().unwrap().take();
            if let Some(r) = running {
                let _ = r.stop.send(true);
                let _ = r.task.await;
            }
            *inner.adb.lock().unwrap() = None;
            let sessions: Vec<_> = inner
                .sessions
                .lock()
                .unwrap()
                .values()
                .map(|s| s.kill.clone())
                .collect();
            for k in sessions {
                k.notify_one();
            }
        }
        inner.notify();
        Ok(())
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.enabled.load(Ordering::Relaxed)
    }

    /// The port actually bound (useful when configured as 0).
    pub fn local_port(&self) -> Option<u16> {
        self.inner
            .running
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|r| r.addrs.first().map(|a| a.port()))
    }

    pub fn status(&self) -> Status {
        let i = &self.inner;
        let fingerprint = i.cert.lock().unwrap().fingerprint_hex();
        let (listening, port, discovery) = {
            let running = i.running.lock().unwrap();
            match running.as_ref() {
                Some(r) => (
                    r.addrs.iter().map(|a| a.to_string()).collect(),
                    r.addrs.first().map(|a| a.port()).unwrap_or(i.opts.port),
                    match &r._advertiser {
                        Some(a) => a.status(),
                        None => DiscoveryStatus::Off,
                    },
                ),
                None => (Vec::new(), i.opts.port, DiscoveryStatus::Off),
            }
        };
        let adb = i
            .adb
            .lock()
            .unwrap()
            .as_ref()
            .map(|a| a.status())
            .unwrap_or(AdbStatus::Off);
        Status {
            enabled: i.enabled.load(Ordering::Relaxed),
            port,
            fingerprint,
            desktop_name: i.opts.desktop_name.clone(),
            listening,
            adb,
            discovery,
            sessions: i.sessions.lock().unwrap().len(),
            stats: i.env.stats.snapshot(),
        }
    }

    pub fn fingerprint_hex(&self) -> String {
        self.inner.cert.lock().unwrap().fingerprint_hex()
    }

    /// Latest devices and pending prompts; wakes on every change.
    pub fn subscribe(&self) -> watch::Receiver<Arc<Snapshot>> {
        self.inner.changed.subscribe()
    }

    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.inner.changed.borrow().clone()
    }

    pub fn pending(&self) -> Vec<PendingPrompt> {
        self.inner.prompts.list()
    }

    pub fn allow(&self, pending_id: u64) -> bool {
        let ok = self
            .inner
            .prompts
            .decide(pending_id, Decision::Allow, &self.inner.opts.limits);
        self.inner.notify();
        ok
    }

    pub fn deny(&self, pending_id: u64) -> bool {
        let ok = self
            .inner
            .prompts
            .decide(pending_id, Decision::Deny, &self.inner.opts.limits);
        self.inner.notify();
        ok
    }

    /// Delete the token and drop the session. The device has to pair again.
    pub fn revoke(&self, device_id: &str, app_id: &str) -> bool {
        let ok = self
            .inner
            .devices
            .revoke(device_id, app_id)
            .unwrap_or(false);
        self.kill_sessions(device_id, Some(app_id));
        self.inner.notify();
        ok
    }

    /// Remove a device and all its apps.
    pub fn forget(&self, device_id: &str) -> bool {
        let n = self.inner.devices.forget(device_id).unwrap_or(0);
        self.kill_sessions(device_id, None);
        self.inner.notify();
        n > 0
    }

    fn kill_sessions(&self, device_id: &str, app_id: Option<&str>) {
        let kills: Vec<_> = self
            .inner
            .sessions
            .lock()
            .unwrap()
            .values()
            .filter(|s| s.who.device_id == device_id && app_id.is_none_or(|a| s.who.app_id == a))
            .map(|s| s.kill.clone())
            .collect();
        for k in kills {
            k.notify_one();
        }
    }

    /// Store a device's settings and push them to its live session.
    pub fn set_config(&self, device_id: &str, app_id: &str, cfg: DeviceConfig) -> bool {
        let cfg = cfg.sanitized();
        let ok = self
            .inner
            .devices
            .set_config(device_id, app_id, &cfg)
            .unwrap_or(false);
        let wire = Envelope {
            msg: Some(Msg::Config(
                self.inner.effective_config(device_id, app_id).to_wire(),
            )),
        };
        for s in self.inner.sessions.lock().unwrap().values() {
            if s.who.device_id == device_id && s.who.app_id == app_id {
                let _ = s.out.try_send(wire.clone());
            }
        }
        self.inner.notify();
        ok
    }

    /// Replace the ingest certificate. Every pairing is revoked, because
    /// every SDK pins the old fingerprint.
    pub async fn regenerate_certificate(&self) -> Result<(), SdkError> {
        let was = self.is_enabled();
        if was {
            self.set_enabled(false).await?;
        }
        let dir = self.inner.cert.lock().unwrap().dir().to_path_buf();
        let new = IngestCert::regenerate(&dir)?;
        *self.inner.cert.lock().unwrap() = new;
        let _ = self.inner.devices.revoke_all();
        if was {
            self.set_enabled(true).await?;
        }
        self.inner.notify();
        Ok(())
    }

    /// Ask the device for a body it kept (lazy bodies). On success the body is
    /// also stored with the flow.
    pub async fn fetch_body(
        &self,
        flow_id: FlowId,
        part: BodyPart,
    ) -> Result<FetchOutcome, FetchError> {
        let i = &self.inner;
        let rec = i
            .env
            .store
            .get(flow_id)
            .ok()
            .flatten()
            .ok_or(FetchError::NotFound)?;
        let info = rec.sdk.clone().ok_or(FetchError::NotSdk)?;
        let state = match part {
            BodyPart::Request => info.req_body,
            BodyPart::Response => info.res_body,
        };
        if state == flow::BodyState::Present {
            let data = match part {
                BodyPart::Request => rec.req_body.clone(),
                BodyPart::Response => rec.res_body.clone(),
            };
            return Ok(FetchOutcome::Body {
                data,
                truncated: false,
            });
        }
        if state == flow::BodyState::Gone || state == flow::BodyState::Skipped {
            return Ok(FetchOutcome::Gone);
        }
        let (out, table) = {
            let s = i.sessions.lock().unwrap();
            let e = s
                .values()
                .find(|s| s.who.device_id == info.device_id && s.who.app_id == info.app_id)
                .ok_or(FetchError::Offline)?;
            (e.out.clone(), e.table.clone())
        };
        let wire_part = match part {
            BodyPart::Request => Part::Request,
            BodyPart::Response => Part::Response,
        };
        let (rx, send) = table.lock().unwrap().begin_fetch(&info.call_id, wire_part);
        if send {
            let req = Envelope {
                msg: Some(Msg::BodyFetch(wire::BodyFetch {
                    call_id: info.call_id.clone(),
                    part: wire_part as i32,
                })),
            };
            if out.try_send(req).is_err() {
                table.lock().unwrap().cancel_fetch(&info.call_id, wire_part);
                return Err(FetchError::Offline);
            }
        }
        match tokio::time::timeout(i.opts.limits.fetch_timeout, rx).await {
            Ok(Ok(o)) => Ok(o),
            Ok(Err(_)) => Err(FetchError::Lost),
            Err(_) => {
                table.lock().unwrap().cancel_fetch(&info.call_id, wire_part);
                Err(FetchError::Timeout)
            }
        }
    }

    /// Feed a finished proxy flow to the merger.
    pub fn observe_proxy_flow(&self, rec: &Arc<FlowRecord>) {
        if rec.source == Source::Proxy {
            self.inner.merger.observe(rec);
        }
    }

    /// Run `f` for every SDK flow that finishes.
    pub fn on_finished(&self, f: impl Fn(&Arc<FlowRecord>) + Send + Sync + 'static) {
        self.inner.hooks.lock().unwrap().push(Arc::new(f));
    }

    pub fn stats(&self) -> StatsSnapshot {
        self.inner.env.stats.snapshot()
    }

    pub fn limits(&self) -> &Limits {
        &self.inner.opts.limits
    }

    /// Addresses a device could use, for the connection hints.
    pub fn lan_addresses(&self) -> Vec<String> {
        lan_ips().into_iter().map(|ip| ip.to_string()).collect()
    }
}

fn lan_ips() -> Vec<IpAddr> {
    if_addrs::get_if_addrs()
        .map(|v| {
            v.into_iter()
                .filter(|i| !i.is_loopback())
                .map(|i| i.ip())
                .filter(|ip| ip.is_ipv4())
                .collect()
        })
        .unwrap_or_default()
}

fn listening_addrs(local: SocketAddr) -> Vec<SocketAddr> {
    if local.ip().is_unspecified() {
        let mut v: Vec<SocketAddr> = lan_ips()
            .into_iter()
            .map(|ip| SocketAddr::new(ip, local.port()))
            .collect();
        v.push(SocketAddr::new(IpAddr::from([127, 0, 0, 1]), local.port()));
        v
    } else {
        vec![local]
    }
}

async fn accept_loop(
    inner: Arc<Inner>,
    listener: TcpListener,
    acceptor: tokio_rustls::TlsAcceptor,
    stop: watch::Receiver<bool>,
) {
    let mut stopped = stop.clone();
    loop {
        tokio::select! {
            _ = wait_true(&mut stopped) => return,
            r = listener.accept() => {
                let (stream, peer) = match r {
                    Ok(x) => x,
                    Err(e) => {
                        tracing::warn!("sdk accept failed: {e}");
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    }
                };
                let _ = stream.set_nodelay(true);
                let Some(guard) = inner.enter_gate(peer.ip()) else {
                    inner.env.stats.rejected_connections.fetch_add(1, Ordering::Relaxed);
                    continue;
                };
                let inner = inner.clone();
                let acceptor = acceptor.clone();
                let stop = stop.clone();
                tokio::spawn(crate::session::run(inner, stream, peer, acceptor, guard, stop));
            }
        }
    }
}

/// Resolves once the watched flag is true (or its sender is gone).
pub(crate) async fn wait_true(rx: &mut watch::Receiver<bool>) {
    loop {
        if *rx.borrow_and_update() {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}
