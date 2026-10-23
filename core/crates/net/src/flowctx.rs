//! Bookkeeping for one flow while it is on the wire.

use crate::body::{BodyEnd, Capture};
use crate::connector::ConnInfo;
use crate::procinfo::AppResolver;
use flow::{flags, FlowEvent, FlowId, FlowRecord, FlowSink, FlowState, Timings};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

/// Name of the local app behind a connection, filled in off the data path.
#[derive(Default)]
pub struct AppSlot(OnceLock<String>, String);

impl AppSlot {
    pub fn lookup(
        resolver: Arc<AppResolverHandle>,
        client_ip: std::net::IpAddr,
        client_port: u16,
        proxy_port: u16,
    ) -> Arc<AppSlot> {
        let slot = Arc::new(AppSlot(OnceLock::new(), client_ip.to_string()));
        let s = slot.clone();
        tokio::task::spawn_blocking(move || {
            if let Some(name) = resolver.0.resolve(client_port, proxy_port) {
                let _ = s.0.set(name);
            }
        });
        slot
    }

    pub fn none() -> Arc<AppSlot> {
        Arc::new(AppSlot::default())
    }

    /// A slot whose app name is already known (replays carry "neko").
    pub fn named(name: &str) -> Arc<AppSlot> {
        let slot = AppSlot::default();
        let _ = slot.0.set(name.to_string());
        Arc::new(slot)
    }

    pub fn get(&self) -> String {
        self.0.get().cloned().unwrap_or_default()
    }

    /// Address the client connected from; empty for replays.
    pub fn client_ip(&self) -> &str {
        &self.1
    }
}

pub struct AppResolverHandle(pub AppResolver);

struct State {
    rec: FlowRecord,
    dispatched_at: Option<Instant>,
    headers_at: Option<Instant>,
    setup_us: u64,
    dns_us: u64,
    connect_us: u64,
    tls_us: u64,
    reused: bool,
}

pub struct FlowCtx {
    sink: Arc<dyn FlowSink>,
    started: Instant,
    state: Mutex<State>,
    pub req_cap: Arc<Capture>,
    pub res_cap: Arc<Capture>,
    finalized: AtomicBool,
    app: Arc<AppSlot>,
}

pub struct NewFlow {
    pub id: FlowId,
    pub started_at_ms: i64,
    pub scheme: &'static str,
    pub host: String,
    pub port: u16,
    pub method: String,
    pub path: String,
    pub query: String,
    pub req_headers: Vec<(String, String)>,
    pub flags: u32,
    pub protocol: &'static str,
}

impl FlowCtx {
    pub fn start(
        sink: Arc<dyn FlowSink>,
        new: NewFlow,
        app: Arc<AppSlot>,
        body_cap: usize,
    ) -> Arc<FlowCtx> {
        let rec = FlowRecord {
            id: new.id,
            started_at_ms: new.started_at_ms,
            scheme: new.scheme.to_string(),
            host: new.host,
            port: new.port,
            method: new.method,
            path: new.path,
            query: new.query,
            protocol: new.protocol.to_string(),
            req_headers: new.req_headers,
            flags: new.flags,
            state: FlowState::Pending,
            client_ip: app.client_ip().to_string(),
            ..Default::default()
        };
        let ctx = Arc::new(FlowCtx {
            sink,
            started: Instant::now(),
            state: Mutex::new(State {
                rec,
                dispatched_at: None,
                headers_at: None,
                setup_us: 0,
                dns_us: 0,
                connect_us: 0,
                tls_us: 0,
                reused: true,
            }),
            req_cap: Capture::new(body_cap),
            res_cap: Capture::new(body_cap),
            finalized: AtomicBool::new(false),
            app,
        });
        let summary = ctx.state.lock().unwrap().rec.summary();
        ctx.sink.publish(FlowEvent::Started(summary));
        ctx
    }

    pub fn summary(&self) -> flow::FlowSummary {
        self.state.lock().unwrap().rec.summary()
    }

    /// Mark the flow (edited, held at a breakpoint, replayed) and tell
    /// subscribers.
    pub fn add_flags(&self, extra: u32) {
        let summary = {
            let mut s = self.state.lock().unwrap();
            if s.rec.flags & extra == extra {
                return;
            }
            s.rec.flags |= extra;
            s.rec.summary()
        };
        self.sink.publish(FlowEvent::Updated(summary));
    }

    /// The request was changed (breakpoint edit) before it went upstream:
    /// record what was actually sent.
    pub fn request_edited(
        &self,
        method: String,
        path: String,
        query: String,
        headers: Vec<(String, String)>,
    ) {
        let summary = {
            let mut s = self.state.lock().unwrap();
            s.rec.method = method;
            s.rec.path = path;
            s.rec.query = query;
            s.rec.req_headers = headers;
            s.rec.flags |= flags::EDITED;
            s.rec.summary()
        };
        self.sink.publish(FlowEvent::Updated(summary));
    }

    pub fn dispatched(&self) {
        self.state.lock().unwrap().dispatched_at = Some(Instant::now());
    }

    /// Response headers arrived. `conn` carries the setup costs when this was
    /// the first response on a fresh connection.
    pub fn response_headers(
        &self,
        status: u16,
        protocol: &'static str,
        headers: Vec<(String, String)>,
        conn: Option<&ConnInfo>,
        extra_flags: u32,
    ) {
        let summary = {
            let mut s = self.state.lock().unwrap();
            s.headers_at = Some(Instant::now());
            if let Some(c) = conn {
                s.dns_us = c.dns_us;
                s.connect_us = c.connect_us;
                s.tls_us = c.tls_us;
                s.setup_us = c.dns_us + c.connect_us + c.tls_us;
                s.reused = false;
            }
            s.rec.status = status;
            s.rec.protocol = protocol.to_string();
            s.rec.res_headers = headers;
            s.rec.flags |= extra_flags;
            s.rec.state = FlowState::Receiving;
            s.rec.app = self.app.get();
            s.rec.summary()
        };
        self.sink.publish(FlowEvent::Updated(summary));
    }

    /// Called when the response body ends, fails or is dropped.
    pub fn finish_body(&self, end: BodyEnd) {
        match end {
            BodyEnd::Complete => self.finalize(None),
            BodyEnd::Error(e) => self.finalize(Some(format!("Response stream failed: {e}"))),
            BodyEnd::Aborted => self.finalize(Some(
                "The client closed the connection before the response finished".into(),
            )),
        }
    }

    /// Upstream could not be reached or answered with garbage.
    pub fn fail(&self, error: String) {
        self.finalize(Some(error));
    }

    /// A tunnel or upgraded connection ended. Sizes are bytes moved in each
    /// direction; the flow is complete unless `error` is set.
    pub fn finish_stream(&self, up_bytes: u64, down_bytes: u64, error: Option<String>) {
        {
            let mut s = self.state.lock().unwrap();
            s.rec.req_size = up_bytes;
            s.rec.res_size = down_bytes;
        }
        self.finalize_inner(error, true);
    }

    fn finalize(&self, error: Option<String>) {
        self.finalize_inner(error, false);
    }

    fn finalize_inner(&self, error: Option<String>, stream: bool) {
        if self.finalized.swap(true, Ordering::AcqRel) {
            return;
        }
        let now = Instant::now();
        let req = self.req_cap.take();
        let res = self.res_cap.take();
        let mut s = self.state.lock().unwrap();
        let total = now.saturating_duration_since(self.started).as_micros() as u64;

        let dispatched = s.dispatched_at.unwrap_or(self.started);
        let ready = dispatched + std::time::Duration::from_micros(s.setup_us);
        let req_end = req.ended_at.filter(|_| req.size > 0);
        let send_us = req_end
            .map(|e| e.saturating_duration_since(ready).as_micros() as u64)
            .unwrap_or(0);
        let (wait_us, receive_us) = match s.headers_at {
            Some(h) => {
                let wait_from = req_end.unwrap_or(ready).max(ready);
                (
                    h.saturating_duration_since(wait_from).as_micros() as u64,
                    now.saturating_duration_since(h).as_micros() as u64,
                )
            }
            None => (0, 0),
        };
        s.rec.timings = Timings {
            dns_us: s.dns_us,
            connect_us: s.connect_us,
            tls_us: s.tls_us,
            send_us,
            wait_us,
            receive_us,
            total_us: total,
            reused: s.reused,
        };
        if !stream {
            s.rec.req_size = req.size;
            s.rec.res_size = res.size;
        }
        s.rec.req_body = req.bytes;
        s.rec.res_body = res.bytes;
        if req.truncated {
            s.rec.flags |= flags::REQ_TRUNCATED;
        }
        if res.truncated {
            s.rec.flags |= flags::RES_TRUNCATED;
        }
        if s.rec.app.is_empty() {
            s.rec.app = self.app.get();
        }
        match error {
            Some(e) => {
                s.rec.error = e;
                s.rec.flags |= flags::ERROR;
                s.rec.state = FlowState::Failed;
            }
            None => s.rec.state = FlowState::Complete,
        }
        let rec = Arc::new(std::mem::take(&mut s.rec));
        drop(s);
        self.sink.publish(FlowEvent::Finished(rec));
    }
}
