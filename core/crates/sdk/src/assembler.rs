//! Turns the call events of one device and app into flows.
//!
//! A flow exists from `CallStart` and shows live; `CallComplete` or
//! `CallFailed` finalize it. Chunks are ordered per call and part, and
//! anything out of order, duplicated or over a limit is dropped and counted.
//! `device_id`, `app_id` and `source` come from the authenticated session,
//! never from a payload.

use crate::limits::Limits;
use crate::wire::{self, body_chunk::Part, Encoding};
use bytes::Bytes;
use flow::{
    flags, BodyState, FlowEvent, FlowId, FlowRecord, FlowSink, FlowState, IdGen, SdkInfo,
    SdkTimings, Source, Timings,
};
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use store::{BodyPart, Store};
use tokio::sync::oneshot;

/// Counters shared by every session. Drops are visible in `sdk.status()`.
#[derive(Default, Debug)]
pub struct Stats {
    pub calls: AtomicU64,
    pub events: AtomicU64,
    pub dropped_chunks: AtomicU64,
    pub dropped_calls: AtomicU64,
    pub dropped_events: AtomicU64,
    pub protocol_violations: AtomicU64,
    pub rejected_connections: AtomicU64,
    pub incomplete_calls: AtomicU64,
}

impl Stats {
    pub fn snapshot(&self) -> StatsSnapshot {
        let g = |a: &AtomicU64| a.load(Ordering::Relaxed);
        StatsSnapshot {
            calls: g(&self.calls),
            events: g(&self.events),
            dropped_chunks: g(&self.dropped_chunks),
            dropped_calls: g(&self.dropped_calls),
            dropped_events: g(&self.dropped_events),
            protocol_violations: g(&self.protocol_violations),
            rejected_connections: g(&self.rejected_connections),
            incomplete_calls: g(&self.incomplete_calls),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatsSnapshot {
    pub calls: u64,
    pub events: u64,
    pub dropped_chunks: u64,
    pub dropped_calls: u64,
    pub dropped_events: u64,
    pub protocol_violations: u64,
    pub rejected_connections: u64,
    pub incomplete_calls: u64,
}

/// Who is talking, as established by the session.
#[derive(Clone, Debug, Default)]
pub struct Identity {
    pub device_id: String,
    pub app_id: String,
    pub device_name: String,
    pub device_model: String,
    pub os_version: String,
    pub app_name: String,
    pub app_version: String,
    pub build: String,
    pub sdk_name: String,
    pub sdk_version: String,
    pub connection: String,
}

/// Notified of every finalized SDK flow (for merging).
pub trait Observer: Send + Sync + 'static {
    fn finished(&self, rec: &Arc<FlowRecord>);
}

/// What a lazy body fetch produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchOutcome {
    Body { data: Bytes, truncated: bool },
    /// The device no longer has it.
    Gone,
}

#[derive(Clone)]
pub struct Env {
    pub ids: Arc<IdGen>,
    pub sink: Arc<dyn FlowSink>,
    pub store: Store,
    pub limits: Limits,
    pub stats: Arc<Stats>,
    pub observer: Option<Arc<dyn Observer>>,
}

#[derive(Default)]
struct Acc {
    buf: Vec<u8>,
    next_seq: u32,
    closed: bool,
    truncated: bool,
    /// A gap was seen; nothing after it can be trusted.
    broken: bool,
    dropped: bool,
}

struct Call {
    rec: FlowRecord,
    info: SdkInfo,
    req: Acc,
    res: Acc,
    last_activity: Instant,
    response_seen: bool,
}

struct Done {
    flow_id: FlowId,
}

struct Fetch {
    acc: Acc,
    waiters: Vec<oneshot::Sender<FetchOutcome>>,
}

/// Calls of one device and app. Shared by its sessions, so a reconnect
/// within the grace period carries on with the same calls.
#[derive(Default)]
pub struct CallTable {
    calls: HashMap<String, Call>,
    done: HashMap<String, Done>,
    done_order: VecDeque<String>,
    fetches: HashMap<(String, i32), Fetch>,
}

impl CallTable {
    pub fn in_flight(&self) -> usize {
        self.calls.len()
    }

    /// The flow id of a call this table knows, finished or not.
    pub fn flow_of(&self, call_id: &str) -> Option<FlowId> {
        self.calls
            .get(call_id)
            .map(|c| c.rec.id)
            .or_else(|| self.done.get(call_id).map(|d| d.flow_id))
    }

    /// Register interest in a lazy body. Returns the receiver and whether a
    /// `BodyFetch` has to be sent (false if one is already outstanding).
    pub fn begin_fetch(
        &mut self,
        call_id: &str,
        part: Part,
    ) -> (oneshot::Receiver<FetchOutcome>, bool) {
        let (tx, rx) = oneshot::channel();
        let key = (call_id.to_string(), part as i32);
        match self.fetches.get_mut(&key) {
            Some(f) => {
                f.waiters.push(tx);
                (rx, false)
            }
            None => {
                self.fetches.insert(
                    key,
                    Fetch {
                        acc: Acc::default(),
                        waiters: vec![tx],
                    },
                );
                (rx, true)
            }
        }
    }

    pub fn cancel_fetch(&mut self, call_id: &str, part: Part) {
        self.fetches.remove(&(call_id.to_string(), part as i32));
    }

    /// Drop every outstanding fetch; waiters see the channel close.
    pub fn abandon_fetches(&mut self) {
        self.fetches.clear();
    }
}

pub fn inflate(data: &[u8], limit: usize) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::DeflateDecoder::new(data)
        .take(limit as u64 + 1)
        .read_to_end(&mut out)
        .ok()?;
    (out.len() <= limit).then_some(out)
}

fn clip(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s[..end].to_string()
    }
}

fn clean_headers(h: &[wire::Header], limits: &Limits) -> (Vec<(String, String)>, bool) {
    let mut out = Vec::with_capacity(h.len().min(limits.max_headers));
    let mut bytes = 0usize;
    let mut cut = false;
    for x in h {
        if out.len() >= limits.max_headers || bytes + x.name.len() + x.value.len() > limits.max_header_bytes {
            cut = true;
            break;
        }
        bytes += x.name.len() + x.value.len();
        out.push((x.name.clone(), x.value.clone()));
    }
    (out, cut)
}

struct ParsedUrl {
    scheme: String,
    host: String,
    port: u16,
    path: String,
    query: String,
}

fn parse_url(url: &str) -> Option<ParsedUrl> {
    let uri: http::Uri = url.parse().ok()?;
    let scheme = uri.scheme_str()?.to_ascii_lowercase();
    let host = uri.host()?.trim_matches(|c| c == '[' || c == ']').to_string();
    let port = uri.port_u16().unwrap_or(match scheme.as_str() {
        "https" | "wss" => 443,
        _ => 80,
    });
    let path = if uri.path().is_empty() { "/" } else { uri.path() }.to_string();
    Some(ParsedUrl {
        scheme,
        host,
        port,
        path,
        query: uri.query().unwrap_or("").to_string(),
    })
}

fn ms_to_us(v: i32) -> u64 {
    if v > 0 {
        v as u64 * 1000
    } else {
        0
    }
}

fn sdk_timings(t: &wire::Timings) -> SdkTimings {
    SdkTimings {
        dns: t.dns,
        connect: t.connect,
        tls: t.tls,
        request_sent: t.request_sent,
        ttfb: t.ttfb,
        download: t.download,
    }
}

fn timings_us(t: &wire::Timings, total_ms: i64) -> Timings {
    Timings {
        dns_us: ms_to_us(t.dns),
        connect_us: ms_to_us(t.connect),
        tls_us: ms_to_us(t.tls),
        send_us: ms_to_us(t.request_sent),
        wait_us: ms_to_us(t.ttfb),
        receive_us: ms_to_us(t.download),
        total_us: total_ms.max(0) as u64 * 1000,
        reused: t.connect == 0 && t.tls <= 0,
    }
}

/// Session-side view needed to correct times and label drops.
pub struct Clock<'a> {
    /// Milliseconds to add to a device timestamp to get core time.
    pub offset_ms: i64,
    pub now_ms: &'a dyn Fn() -> i64,
}

pub fn unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl Env {
    fn publish(&self, e: FlowEvent) {
        self.sink.publish(e);
    }

    fn drop_chunk(&self) {
        self.stats.dropped_chunks.fetch_add(1, Ordering::Relaxed);
    }

    pub fn call_start(
        &self,
        table: &mut CallTable,
        who: &Identity,
        offset_ms: i64,
        m: wire::CallStart,
    ) {
        let l = &self.limits;
        if m.call_id.is_empty()
            || m.call_id.len() > 64
            || m.url.len() > l.max_url_len
            || table.calls.contains_key(&m.call_id)
            || table.done.contains_key(&m.call_id)
        {
            self.stats.dropped_calls.fetch_add(1, Ordering::Relaxed);
            return;
        }
        if table.calls.len() >= l.max_inflight_calls {
            self.stats.dropped_calls.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let Some(url) = parse_url(&m.url) else {
            self.stats.dropped_calls.fetch_add(1, Ordering::Relaxed);
            return;
        };
        let (req_headers, cut) = clean_headers(&m.headers, l);
        let now = unix_ms();
        // A device clock that is far in the future would sort its calls
        // after everything; trust the corrected start only if it is sane.
        let started = {
            let t = m.started_at_ms.saturating_add(offset_ms);
            if (t - now).abs() > 24 * 3600 * 1000 {
                now
            } else {
                t
            }
        };
        let mut f = flags::REQ_TRUNCATED * m.request_body_truncated as u32;
        if cut {
            f |= flags::DROPPED;
        }
        let info = SdkInfo {
            device_id: who.device_id.clone(),
            app_id: who.app_id.clone(),
            call_id: m.call_id.clone(),
            name: clip(&m.name, l.max_field_len),
            tag: clip(&m.tag, l.max_field_len),
            owner: clip(&m.owner, l.max_field_len),
            screen: clip(&m.screen, l.max_field_len),
            route_template: clip(&m.route_template, l.max_field_len),
            client_stack: clip(&m.client_stack, 32),
            device_name: who.device_name.clone(),
            device_model: who.device_model.clone(),
            os_version: who.os_version.clone(),
            app_name: who.app_name.clone(),
            app_version: who.app_version.clone(),
            build: who.build.clone(),
            sdk_name: who.sdk_name.clone(),
            sdk_version: who.sdk_version.clone(),
            connection: who.connection.clone(),
            ..Default::default()
        };
        let id = self.ids.next();
        let rec = FlowRecord {
            id,
            started_at_ms: started,
            scheme: url.scheme,
            host: url.host,
            port: url.port,
            method: clip(&m.method, 16).to_ascii_uppercase(),
            path: url.path,
            query: url.query,
            protocol: protocol_label(&m.http_version),
            app: who.app_id.clone(),
            state: FlowState::Pending,
            flags: f,
            req_headers,
            req_size: m.request_body_size,
            source: Source::Sdk,
            sdk: Some(Arc::new(info.clone())),
            ..Default::default()
        };
        self.stats.calls.fetch_add(1, Ordering::Relaxed);
        self.publish(FlowEvent::Started(rec.summary()));
        table.calls.insert(
            m.call_id,
            Call {
                rec,
                info,
                req: Acc::default(),
                res: Acc::default(),
                last_activity: Instant::now(),
                response_seen: false,
            },
        );
    }

    pub fn body_chunk(&self, table: &mut CallTable, m: wire::BodyChunk) {
        let part = Part::try_from(m.part).unwrap_or(Part::Request);
        if let Some(call) = table.calls.get_mut(&m.call_id) {
            call.last_activity = Instant::now();
            let acc = match part {
                Part::Request => &mut call.req,
                Part::Response => &mut call.res,
            };
            self.append(acc, &m);
            return;
        }
        // A chunk for a finished call is the answer to a lazy fetch.
        let key = (m.call_id.clone(), part as i32);
        if let Some(f) = table.fetches.get_mut(&key) {
            self.append(&mut f.acc, &m);
            if f.acc.closed {
                let f = table.fetches.remove(&key).unwrap();
                self.finish_fetch(table, &m.call_id, part, f);
            }
            return;
        }
        self.drop_chunk();
    }

    fn append(&self, acc: &mut Acc, m: &wire::BodyChunk) {
        if acc.closed || acc.broken {
            self.drop_chunk();
            return;
        }
        if m.seq != acc.next_seq {
            // Duplicate (already seen) or a gap; either way the stream can
            // no longer be reassembled faithfully past this point.
            self.drop_chunk();
            if m.seq > acc.next_seq {
                acc.broken = true;
                acc.truncated = true;
            }
            return;
        }
        acc.next_seq += 1;
        let cap = self.limits.max_body_per_call;
        let data: std::borrow::Cow<[u8]> = match Encoding::try_from(m.encoding).unwrap_or(Encoding::None) {
            Encoding::None => std::borrow::Cow::Borrowed(&m.data),
            Encoding::Deflate => match inflate(&m.data, self.limits.max_chunk_inflated) {
                Some(v) => v.into(),
                None => {
                    self.drop_chunk();
                    acc.broken = true;
                    acc.truncated = true;
                    return;
                }
            },
        };
        let room = cap.saturating_sub(acc.buf.len());
        if data.len() > room {
            acc.buf.extend_from_slice(&data[..room]);
            acc.truncated = true;
            acc.dropped = true;
            self.drop_chunk();
        } else {
            acc.buf.extend_from_slice(&data);
        }
        if m.truncated {
            acc.truncated = true;
        }
        if m.last || m.truncated {
            acc.closed = true;
        }
    }

    fn finish_fetch(&self, table: &mut CallTable, call_id: &str, part: Part, f: Fetch) {
        let Some(flow_id) = table.done.get(call_id).map(|d| d.flow_id) else {
            return;
        };
        let data = Bytes::from(f.acc.buf);
        self.store.update_body(
            flow_id,
            match part {
                Part::Request => BodyPart::Request,
                Part::Response => BodyPart::Response,
            },
            data.clone(),
            BodyState::Present,
            f.acc.truncated,
        );
        for w in f.waiters {
            let _ = w.send(FetchOutcome::Body {
                data: data.clone(),
                truncated: f.acc.truncated,
            });
        }
    }

    pub fn body_fetch_result(&self, table: &mut CallTable, m: wire::BodyFetchResult) {
        if m.available {
            return; // chunks follow
        }
        let part = Part::try_from(m.part).unwrap_or(Part::Request);
        let key = (m.call_id.clone(), part as i32);
        let Some(f) = table.fetches.remove(&key) else {
            return;
        };
        if let Some(d) = table.done.get(&m.call_id) {
            self.store.update_body(
                d.flow_id,
                match part {
                    Part::Request => BodyPart::Request,
                    Part::Response => BodyPart::Response,
                },
                Bytes::new(),
                BodyState::Gone,
                false,
            );
        }
        for w in f.waiters {
            let _ = w.send(FetchOutcome::Gone);
        }
    }

    pub fn response_start(&self, table: &mut CallTable, m: wire::ResponseStart) {
        let Some(call) = table.calls.get_mut(&m.call_id) else {
            self.stats.dropped_events.fetch_add(1, Ordering::Relaxed);
            return;
        };
        call.last_activity = Instant::now();
        let (headers, cut) = clean_headers(&m.headers, &self.limits);
        call.rec.status = m.status.min(999) as u16;
        call.rec.res_headers = headers;
        call.rec.res_size = m.response_body_size;
        if !m.http_version.is_empty() {
            call.rec.protocol = protocol_label(&m.http_version);
        }
        if cut {
            call.rec.flags |= flags::DROPPED;
        }
        call.rec.state = FlowState::Receiving;
        call.response_seen = true;
        self.publish(FlowEvent::Updated(call.rec.summary()));
    }

    pub fn call_complete(&self, table: &mut CallTable, offset_ms: i64, m: wire::CallComplete) {
        let Some(mut call) = table.calls.remove(&m.call_id) else {
            self.stats.dropped_events.fetch_add(1, Ordering::Relaxed);
            return;
        };
        let ended = m.ended_at_ms.saturating_add(offset_ms);
        let total = (ended - call.rec.started_at_ms).max(0);
        let t = m.timings.unwrap_or(wire::Timings {
            dns: -1,
            connect: -1,
            tls: -1,
            request_sent: -1,
            ttfb: -1,
            download: -1,
        });
        call.rec.timings = timings_us(&t, total);
        call.info.timings = Some(sdk_timings(&t));
        call.info.from_cache = m.from_cache;
        call.info.req_redacted = m.request_redacted;
        call.info.res_redacted = m.response_redacted;
        if m.request_redacted || m.response_redacted {
            call.rec.flags |= flags::REDACTED;
        }
        call.info.req_body = body_state(
            m.request_body_available,
            m.request_body_skipped,
            call.req.buf.is_empty(),
            m.request_body_size,
            &call.req,
        );
        call.info.res_body = body_state(
            m.response_body_available,
            m.response_body_skipped,
            call.res.buf.is_empty(),
            m.response_body_size,
            &call.res,
        );
        if m.request_body_size > 0 {
            call.rec.req_size = m.request_body_size;
        } else if call.rec.req_size == 0 {
            call.rec.req_size = call.req.buf.len() as u64;
        }
        if m.response_body_size > 0 {
            call.rec.res_size = m.response_body_size;
        } else if call.rec.res_size == 0 {
            call.rec.res_size = call.res.buf.len() as u64;
        }
        if call.req.truncated {
            call.rec.flags |= flags::REQ_TRUNCATED;
        }
        if call.res.truncated {
            call.rec.flags |= flags::RES_TRUNCATED;
        }
        if call.req.dropped || call.res.dropped || call.req.broken || call.res.broken {
            call.rec.flags |= flags::DROPPED;
        }
        call.rec.req_body = Bytes::from(std::mem::take(&mut call.req.buf));
        call.rec.res_body = Bytes::from(std::mem::take(&mut call.res.buf));
        call.rec.state = FlowState::Complete;
        self.finalize(table, call);
    }

    pub fn call_failed(&self, table: &mut CallTable, offset_ms: i64, m: wire::CallFailed) {
        let Some(mut call) = table.calls.remove(&m.call_id) else {
            self.stats.dropped_events.fetch_add(1, Ordering::Relaxed);
            return;
        };
        let ended = if m.ended_at_ms > 0 {
            m.ended_at_ms.saturating_add(offset_ms)
        } else {
            unix_ms()
        };
        let total = (ended - call.rec.started_at_ms).max(0);
        let t = m.timings.unwrap_or_default();
        call.rec.timings = timings_us(&t, total);
        call.info.timings = Some(sdk_timings(&t));
        call.rec.state = FlowState::Failed;
        call.rec.flags |= flags::ERROR;
        let kind = clip(&m.kind, 16);
        let msg = clip(&m.message, 512);
        call.rec.error = if msg.is_empty() {
            kind
        } else {
            format!("{kind}: {msg}")
        };
        call.rec.req_body = Bytes::from(std::mem::take(&mut call.req.buf));
        call.rec.res_body = Bytes::from(std::mem::take(&mut call.res.buf));
        self.finalize(table, call);
    }

    fn finalize(&self, table: &mut CallTable, mut call: Call) {
        call.rec.sdk = Some(Arc::new(call.info));
        let rec = Arc::new(call.rec);
        let call_id = rec.sdk.as_ref().map(|i| i.call_id.clone()).unwrap_or_default();
        self.remember_done(table, call_id, rec.id);
        self.publish(FlowEvent::Finished(rec.clone()));
        if let Some(o) = &self.observer {
            o.finished(&rec);
        }
    }

    fn remember_done(&self, table: &mut CallTable, call_id: String, flow_id: FlowId) {
        if table.done_order.len() >= self.limits.max_completed_calls {
            if let Some(old) = table.done_order.pop_front() {
                table.done.remove(&old);
            }
        }
        table.done_order.push_back(call_id.clone());
        table.done.insert(call_id, Done { flow_id });
    }

    /// Finish every call still open, because the session is gone for good.
    pub fn fail_open_calls(&self, table: &mut CallTable, reason: &str) -> usize {
        let ids: Vec<String> = table.calls.keys().cloned().collect();
        let n = ids.len();
        for id in ids {
            let Some(mut call) = table.calls.remove(&id) else {
                continue;
            };
            call.rec.state = FlowState::Failed;
            call.rec.flags |= flags::ERROR | flags::INCOMPLETE;
            call.rec.error = reason.to_string();
            call.rec.timings.total_us = (unix_ms() - call.rec.started_at_ms).max(0) as u64 * 1000;
            call.rec.req_body = Bytes::from(std::mem::take(&mut call.req.buf));
            call.rec.res_body = Bytes::from(std::mem::take(&mut call.res.buf));
            self.stats.incomplete_calls.fetch_add(1, Ordering::Relaxed);
            self.finalize(table, call);
        }
        table.abandon_fetches();
        n
    }

    /// Finish calls that have been silent for `idle`, for sessions that stay
    /// up while the SDK lost track of a call.
    pub fn sweep(&self, table: &mut CallTable, idle: std::time::Duration) -> usize {
        let stale: Vec<String> = table
            .calls
            .iter()
            .filter(|(_, c)| c.last_activity.elapsed() > idle)
            .map(|(k, _)| k.clone())
            .collect();
        let n = stale.len();
        for id in stale {
            if let Some(mut call) = table.calls.remove(&id) {
                call.rec.state = FlowState::Failed;
                call.rec.flags |= flags::ERROR | flags::INCOMPLETE;
                call.rec.error = "no update from the app".into();
                self.stats.incomplete_calls.fetch_add(1, Ordering::Relaxed);
                self.finalize(table, call);
            }
        }
        n
    }
}

fn body_state(available: bool, skipped: bool, empty: bool, size: u64, acc: &Acc) -> BodyState {
    if skipped {
        BodyState::Skipped
    } else if available || (empty && size == 0) {
        BodyState::Present
    } else if !empty && !acc.broken && acc.closed {
        BodyState::Present
    } else {
        BodyState::Lazy
    }
}

fn protocol_label(v: &str) -> String {
    match v.to_ascii_lowercase().as_str() {
        "" => "HTTP".into(),
        "h2" | "http/2" | "http/2.0" => "h2".into(),
        "h3" | "http/3" => "h3".into(),
        "http/1.1" | "http/1.0" => v.to_ascii_uppercase(),
        other => clip(other, 16),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flow::NullSink;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Collect(Mutex<Vec<FlowEvent>>);
    impl FlowSink for Collect {
        fn publish(&self, e: FlowEvent) {
            self.0.lock().unwrap().push(e);
        }
    }

    fn env(dir: &std::path::Path, sink: Arc<dyn FlowSink>) -> Env {
        Env {
            ids: Arc::new(IdGen::starting_after(0)),
            sink,
            store: Store::open(dir).unwrap(),
            limits: Limits::default(),
            stats: Arc::new(Stats::default()),
            observer: None,
        }
    }

    fn who() -> Identity {
        Identity {
            device_id: "dev".into(),
            app_id: "com.app".into(),
            ..Default::default()
        }
    }

    fn start(id: &str) -> wire::CallStart {
        wire::CallStart {
            call_id: id.into(),
            started_at_ms: unix_ms(),
            method: "post".into(),
            url: "https://api.test/v1/cart/7?x=1".into(),
            headers: vec![wire::Header {
                name: "Accept".into(),
                value: "application/json".into(),
            }],
            client_stack: "okhttp".into(),
            name: "Apply coupon".into(),
            ..Default::default()
        }
    }

    fn chunk(id: &str, part: Part, seq: u32, data: &[u8], last: bool) -> wire::BodyChunk {
        wire::BodyChunk {
            call_id: id.into(),
            part: part as i32,
            seq,
            data: data.to_vec(),
            encoding: Encoding::None as i32,
            last,
            truncated: false,
        }
    }

    fn finished(c: &Collect) -> Vec<Arc<FlowRecord>> {
        c.0.lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                FlowEvent::Finished(r) => Some(r.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn happy_path_assembles_one_flow() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(Collect::default());
        let env = env(dir.path(), sink.clone());
        let mut t = CallTable::default();
        env.call_start(&mut t, &who(), 0, start("c1"));
        env.body_chunk(&mut t, chunk("c1", Part::Request, 0, b"{\"a\":", false));
        env.body_chunk(&mut t, chunk("c1", Part::Request, 1, b"1}", true));
        env.response_start(
            &mut t,
            wire::ResponseStart {
                call_id: "c1".into(),
                status: 200,
                headers: vec![wire::Header {
                    name: "Content-Type".into(),
                    value: "application/json".into(),
                }],
                http_version: "h2".into(),
                response_body_size: 7,
            },
        );
        env.body_chunk(&mut t, chunk("c1", Part::Response, 0, b"{\"ok\":1}", true));
        env.call_complete(
            &mut t,
            0,
            wire::CallComplete {
                call_id: "c1".into(),
                ended_at_ms: unix_ms() + 40,
                timings: Some(wire::Timings {
                    dns: 2,
                    connect: 0,
                    tls: -1,
                    request_sent: 1,
                    ttfb: 20,
                    download: 5,
                }),
                response_body_available: true,
                request_body_available: true,
                ..Default::default()
            },
        );
        let done = finished(&sink);
        assert_eq!(done.len(), 1);
        let r = &done[0];
        assert_eq!(r.method, "POST");
        assert_eq!((r.host.as_str(), r.port, r.path.as_str(), r.query.as_str()), ("api.test", 443, "/v1/cart/7", "x=1"));
        assert_eq!(r.source, Source::Sdk);
        assert_eq!(r.status, 200);
        assert_eq!(&r.req_body[..], b"{\"a\":1}");
        assert_eq!(&r.res_body[..], b"{\"ok\":1}");
        assert_eq!(r.protocol, "h2");
        assert_eq!(r.state, FlowState::Complete);
        assert!(r.timings.reused);
        assert_eq!(r.timings.dns_us, 2000);
        let sdk = r.sdk.as_ref().unwrap();
        assert_eq!(sdk.device_id, "dev");
        assert_eq!(sdk.name, "Apply coupon");
        assert_eq!(sdk.res_body, BodyState::Present);
        assert_eq!(t.in_flight(), 0);
        // Started, Updated, Finished.
        assert_eq!(sink.0.lock().unwrap().len(), 3);
    }

    #[test]
    fn duplicate_and_out_of_order_chunks_are_dropped_and_counted() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(Collect::default());
        let env = env(dir.path(), sink.clone());
        let mut t = CallTable::default();
        env.call_start(&mut t, &who(), 0, start("c"));
        env.body_chunk(&mut t, chunk("c", Part::Response, 0, b"AAA", false));
        env.body_chunk(&mut t, chunk("c", Part::Response, 0, b"AAA", false)); // duplicate
        env.body_chunk(&mut t, chunk("c", Part::Response, 2, b"CCC", false)); // gap
        env.body_chunk(&mut t, chunk("c", Part::Response, 1, b"BBB", true)); // after the gap
        env.call_complete(&mut t, 0, wire::CallComplete { call_id: "c".into(), ..Default::default() });
        let r = &finished(&sink)[0];
        assert_eq!(&r.res_body[..], b"AAA");
        assert!(r.flags & flags::RES_TRUNCATED != 0);
        assert!(r.flags & flags::DROPPED != 0);
        assert_eq!(env.stats.snapshot().dropped_chunks, 3);
    }

    #[test]
    fn bodies_are_capped_whatever_the_sdk_sends() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(Collect::default());
        let mut e = env(dir.path(), sink.clone());
        e.limits.max_body_per_call = 10;
        let mut t = CallTable::default();
        e.call_start(&mut t, &who(), 0, start("c"));
        e.body_chunk(&mut t, chunk("c", Part::Response, 0, &[1; 8], false));
        e.body_chunk(&mut t, chunk("c", Part::Response, 1, &[2; 8], true));
        e.call_complete(&mut t, 0, wire::CallComplete { call_id: "c".into(), ..Default::default() });
        let r = &finished(&sink)[0];
        assert_eq!(r.res_body.len(), 10);
        assert!(r.flags & flags::RES_TRUNCATED != 0);
    }

    #[test]
    fn deflate_chunks_inflate_and_bombs_are_refused() {
        use flate2::{write::DeflateEncoder, Compression};
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(Collect::default());
        let mut e = env(dir.path(), sink.clone());
        e.limits.max_chunk_inflated = 1000;
        let mut t = CallTable::default();
        e.call_start(&mut t, &who(), 0, start("c"));
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::fast());
        enc.write_all(b"hello hello hello hello").unwrap();
        let mut ch = chunk("c", Part::Response, 0, &enc.finish().unwrap(), false);
        ch.encoding = Encoding::Deflate as i32;
        e.body_chunk(&mut t, ch);
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::best());
        enc.write_all(&vec![0u8; 100_000]).unwrap();
        let mut bomb = chunk("c", Part::Response, 1, &enc.finish().unwrap(), true);
        bomb.encoding = Encoding::Deflate as i32;
        e.body_chunk(&mut t, bomb);
        e.call_complete(&mut t, 0, wire::CallComplete { call_id: "c".into(), ..Default::default() });
        let r = &finished(&sink)[0];
        assert_eq!(&r.res_body[..], b"hello hello hello hello");
        assert!(r.flags & flags::DROPPED != 0);
    }

    #[test]
    fn limits_on_calls_and_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = env(dir.path(), Arc::new(NullSink));
        e.limits.max_inflight_calls = 2;
        let mut t = CallTable::default();
        e.call_start(&mut t, &who(), 0, start("a"));
        e.call_start(&mut t, &who(), 0, start("a")); // duplicate id
        e.call_start(&mut t, &who(), 0, start("b"));
        e.call_start(&mut t, &who(), 0, start("c")); // over the limit
        let mut bad = start("d");
        bad.url = "not a url".into();
        e.call_start(&mut t, &who(), 0, bad);
        assert_eq!(t.in_flight(), 2);
        assert_eq!(e.stats.snapshot().dropped_calls, 3);
        // Events for calls that were never accepted are counted, not fatal.
        e.call_complete(&mut t, 0, wire::CallComplete { call_id: "zzz".into(), ..Default::default() });
        e.response_start(&mut t, wire::ResponseStart { call_id: "zzz".into(), ..Default::default() });
        assert_eq!(e.stats.snapshot().dropped_events, 2);
    }

    #[test]
    fn failure_and_session_loss_mark_flows() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(Collect::default());
        let e = env(dir.path(), sink.clone());
        let mut t = CallTable::default();
        e.call_start(&mut t, &who(), 0, start("a"));
        e.call_start(&mut t, &who(), 0, start("b"));
        e.call_failed(
            &mut t,
            0,
            wire::CallFailed {
                call_id: "a".into(),
                kind: "timeout".into(),
                message: "read timed out".into(),
                ..Default::default()
            },
        );
        assert_eq!(e.fail_open_calls(&mut t, "connection to the app was lost"), 1);
        let done = finished(&sink);
        let a = done.iter().find(|r| r.error.starts_with("timeout")).unwrap();
        assert_eq!(a.state, FlowState::Failed);
        assert!(a.flags & flags::ERROR != 0);
        let b = done.iter().find(|r| r.flags & flags::INCOMPLETE != 0).unwrap();
        assert_eq!(b.error, "connection to the app was lost");
    }

    #[test]
    fn lazy_bodies_are_fetched_and_stored_later() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(Collect::default());
        let e = env(dir.path(), sink.clone());
        let mut t = CallTable::default();
        e.call_start(&mut t, &who(), 0, start("c"));
        e.call_complete(
            &mut t,
            0,
            wire::CallComplete {
                call_id: "c".into(),
                response_body_available: false,
                response_body_size: 5_000_000,
                ..Default::default()
            },
        );
        let r = finished(&sink).remove(0);
        assert_eq!(r.sdk.as_ref().unwrap().res_body, BodyState::Lazy);
        assert_eq!(r.res_size, 5_000_000);
        e.store.enqueue(r.clone());

        let (mut rx, send) = t.begin_fetch("c", Part::Response);
        assert!(send);
        let (_rx2, send2) = t.begin_fetch("c", Part::Response);
        assert!(!send2, "a second request for the same body does not ask the device again");
        e.body_chunk(&mut t, chunk("c", Part::Response, 0, b"big ", false));
        e.body_chunk(&mut t, chunk("c", Part::Response, 1, b"body", true));
        match rx.try_recv().unwrap() {
            FetchOutcome::Body { data, truncated } => {
                assert_eq!(&data[..], b"big body");
                assert!(!truncated);
            }
            other => panic!("{other:?}"),
        }
        e.store.flush();
        let stored = e.store.get(r.id).unwrap().unwrap();
        assert_eq!(&stored.res_body[..], b"big body");
        assert_eq!(stored.sdk.as_ref().unwrap().res_body, BodyState::Present);

        // A body the device no longer has.
        let (mut rx, _) = t.begin_fetch("c", Part::Request);
        e.body_fetch_result(&mut t, wire::BodyFetchResult { call_id: "c".into(), part: Part::Request as i32, available: false });
        assert_eq!(rx.try_recv().unwrap(), FetchOutcome::Gone);
        // Unsolicited chunks for finished calls are dropped.
        e.body_chunk(&mut t, chunk("c", Part::Response, 0, b"x", true));
        assert_eq!(e.stats.snapshot().dropped_chunks, 1);
    }
}
