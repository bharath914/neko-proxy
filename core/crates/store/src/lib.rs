//! Persistence. Flows land in SQLite (WAL) with bodies in content-addressed
//! zstd blobs next to it. Writes happen on one dedicated thread so the data
//! path only ever pushes onto a queue.

mod blobs;
mod har;
mod query;
mod schema;

pub use har::to_har;
pub use query::{matches_summary, DeviceRow, HostRow, Kind, Page, Query};

use blobs::BlobStore;
use bytes::Bytes;
use flow::{
    flags, BodyState, FlowEvent, FlowId, FlowRecord, FlowSink, FlowState, FlowSummary, Header,
    SdkInfo, SdkTimings, Source, Timings,
};
use rusqlite::{params, Connection, OpenFlags};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Beyond this many queued flows, new flows are stored without bodies so a
/// stalled disk cannot grow memory without bound.
const MAX_PENDING_WITH_BODIES: usize = 4096;
const BATCH_MAX: usize = 256;
const BATCH_WINDOW: Duration = Duration::from_millis(20);
const FTS_BODY_LIMIT: usize = 64 * 1024;

/// Which body of a flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyPart {
    Request,
    Response,
}

enum Msg {
    Flow(Arc<FlowRecord>),
    /// A body that arrived after its flow was stored (lazy SDK bodies).
    Body {
        id: FlowId,
        part: BodyPart,
        data: bytes::Bytes,
        state: BodyState,
        truncated: bool,
    },
    /// Link an SDK record and a proxy record, or unlink with `other = 0`.
    Merge {
        id: FlowId,
        other: FlowId,
    },
    Clear(mpsc::Sender<u64>),
    Flush(mpsc::Sender<()>),
    Shutdown,
}

struct Shared {
    dir: PathBuf,
    blobs: BlobStore,
    pending: Mutex<HashMap<FlowId, Arc<FlowRecord>>>,
    reader: Mutex<Connection>,
}

#[derive(Clone)]
pub struct Store {
    shared: Arc<Shared>,
    tx: mpsc::Sender<Msg>,
    writer: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl Store {
    /// Open or create the store under `dir`.
    pub fn open(dir: &Path) -> Result<Store> {
        std::fs::create_dir_all(dir)?;
        let db_path = dir.join("flows.db");
        let write_conn = Connection::open(&db_path)?;
        schema::init(&write_conn)?;
        let reader = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        reader.pragma_update(None, "query_only", true)?;
        reader.busy_timeout(Duration::from_secs(5))?;

        let blobs = BlobStore::new(dir.join("blobs"))?;
        let shared = Arc::new(Shared {
            dir: dir.to_path_buf(),
            blobs,
            pending: Mutex::new(HashMap::new()),
            reader: Mutex::new(reader),
        });
        let (tx, rx) = mpsc::channel();
        let s2 = shared.clone();
        let handle = std::thread::Builder::new()
            .name("store-writer".into())
            .spawn(move || writer_loop(write_conn, s2, rx))
            .map_err(StoreError::Io)?;
        Ok(Store {
            shared,
            tx,
            writer: Arc::new(Mutex::new(Some(handle))),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.shared.dir
    }

    /// Highest flow id stored. Seeds the id generator.
    pub fn last_id(&self) -> Result<FlowId> {
        let c = self.shared.reader.lock().unwrap();
        Ok(
            c.query_row("SELECT COALESCE(MAX(id), 0) FROM flow", [], |r| {
                r.get::<_, i64>(0)
            })? as FlowId,
        )
    }

    /// Queue a finished flow. Never blocks.
    pub fn enqueue(&self, rec: Arc<FlowRecord>) {
        let mut pending = self.shared.pending.lock().unwrap();
        let rec = if pending.len() >= MAX_PENDING_WITH_BODIES && !rec.req_body.is_empty()
            || pending.len() >= MAX_PENDING_WITH_BODIES && !rec.res_body.is_empty()
        {
            let mut r = (*rec).clone();
            r.req_body = Default::default();
            r.res_body = Default::default();
            r.flags |= flags::REQ_TRUNCATED | flags::RES_TRUNCATED;
            Arc::new(r)
        } else {
            rec
        };
        pending.insert(rec.id, rec.clone());
        drop(pending);
        let _ = self.tx.send(Msg::Flow(rec));
    }

    /// Wait until everything queued so far is on disk.
    pub fn flush(&self) {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(Msg::Flush(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(10));
        }
    }

    /// Flush and stop the writer. Idempotent.
    pub fn close(&self) {
        let handle = self.writer.lock().unwrap().take();
        if let Some(h) = handle {
            let _ = self.tx.send(Msg::Shutdown);
            let _ = h.join();
        }
    }

    pub fn count(&self) -> Result<u64> {
        let c = self.shared.reader.lock().unwrap();
        Ok(c.query_row("SELECT COUNT(*) FROM flow", [], |r| r.get::<_, i64>(0))? as u64)
    }

    /// Remove every flow and blob. Returns how many flows were removed.
    pub fn clear(&self) -> Result<u64> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Msg::Clear(tx))
            .map_err(|_| StoreError::Io(std::io::Error::other("store writer stopped")))?;
        rx.recv_timeout(Duration::from_secs(30))
            .map_err(|_| StoreError::Io(std::io::Error::other("clear timed out")))
    }

    pub fn search(&self, q: &Query, offset: u32, limit: u32) -> Result<Page> {
        let c = self.shared.reader.lock().unwrap();
        query::run(&c, q, offset, limit)
    }

    /// Hosts that have stored traffic, newest activity first.
    pub fn hosts(&self, limit: u32) -> Result<Vec<HostRow>> {
        let c = self.shared.reader.lock().unwrap();
        query::hosts(&c, limit)
    }

    /// Stored flow counts per SDK device and app.
    pub fn devices(&self) -> Result<Vec<DeviceRow>> {
        let c = self.shared.reader.lock().unwrap();
        query::devices(&c)
    }

    /// Ids matching a query, for exports.
    pub fn ids(&self, q: &Query, limit: u32) -> Result<Vec<FlowId>> {
        Ok(self
            .search(q, 0, limit)?
            .items
            .into_iter()
            .map(|s| s.id)
            .collect())
    }

    /// Fetch one flow with headers and bodies.
    pub fn get(&self, id: FlowId) -> Result<Option<Arc<FlowRecord>>> {
        if let Some(r) = self.shared.pending.lock().unwrap().get(&id) {
            return Ok(Some(r.clone()));
        }
        let c = self.shared.reader.lock().unwrap();
        Ok(load_record(&c, &self.shared.blobs, id)?.map(Arc::new))
    }

    /// Store a body that arrived after its flow (a lazy SDK body fetched on
    /// demand). Never blocks.
    pub fn update_body(
        &self,
        id: FlowId,
        part: BodyPart,
        data: Bytes,
        state: BodyState,
        truncated: bool,
    ) {
        let _ = self.tx.send(Msg::Body {
            id,
            part,
            data,
            state,
            truncated,
        });
    }

    /// Link two records as a merged pair (`id` is the SDK record, `other` the
    /// proxy record), or unlink `id` and whatever it pointed at with
    /// `other = 0`. The records themselves are untouched.
    pub fn set_merge(&self, id: FlowId, other: FlowId) {
        let _ = self.tx.send(Msg::Merge { id, other });
    }

    /// Summaries for a set of ids, in id order.
    pub fn summaries(&self, ids: &[FlowId]) -> Result<Vec<FlowSummary>> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(r) = self.get(*id)? {
                out.push(r.summary());
            }
        }
        Ok(out)
    }
}

/// Plug the store into the data path.
impl FlowSink for Store {
    fn publish(&self, event: FlowEvent) {
        if let FlowEvent::Finished(rec) = event {
            self.enqueue(rec);
        }
    }
}

fn sdk_to_json(i: &SdkInfo) -> String {
    let t = i
        .timings
        .map(|t| serde_json::json!([t.dns, t.connect, t.tls, t.request_sent, t.ttfb, t.download]));
    serde_json::json!({
        "device_name": i.device_name, "device_model": i.device_model, "os_version": i.os_version,
        "app_name": i.app_name, "app_version": i.app_version, "build": i.build,
        "sdk_name": i.sdk_name, "sdk_version": i.sdk_version, "connection": i.connection,
        "from_cache": i.from_cache, "req_redacted": i.req_redacted, "res_redacted": i.res_redacted,
        "req_body": i.req_body as u8, "res_body": i.res_body as u8, "timings": t,
    })
    .to_string()
}

#[allow(clippy::too_many_arguments)]
fn sdk_from_row(
    json: &str,
    device_id: String,
    app_id: String,
    call_id: String,
    name: String,
    tag: String,
    owner: String,
    screen: String,
    route_template: String,
    client_stack: String,
) -> SdkInfo {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    let st = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let bl = |k: &str| v.get(k).and_then(|x| x.as_bool()).unwrap_or(false);
    let timings = v.get("timings").and_then(|t| t.as_array()).and_then(|a| {
        let n = |i: usize| a.get(i).and_then(|x| x.as_i64()).unwrap_or(-1) as i32;
        (a.len() == 6).then(|| SdkTimings {
            dns: n(0),
            connect: n(1),
            tls: n(2),
            request_sent: n(3),
            ttfb: n(4),
            download: n(5),
        })
    });
    SdkInfo {
        device_id,
        app_id,
        call_id,
        name,
        tag,
        owner,
        screen,
        route_template,
        client_stack,
        device_name: st("device_name"),
        device_model: st("device_model"),
        os_version: st("os_version"),
        app_name: st("app_name"),
        app_version: st("app_version"),
        build: st("build"),
        sdk_name: st("sdk_name"),
        sdk_version: st("sdk_version"),
        connection: st("connection"),
        from_cache: bl("from_cache"),
        req_redacted: bl("req_redacted"),
        res_redacted: bl("res_redacted"),
        req_body: BodyState::from_u8(v.get("req_body").and_then(|x| x.as_u64()).unwrap_or(0) as u8),
        res_body: BodyState::from_u8(v.get("res_body").and_then(|x| x.as_u64()).unwrap_or(0) as u8),
        timings,
    }
}

/// Load one stored flow with headers, bodies and SDK context.
fn load_record(conn: &Connection, blobs: &BlobStore, id: FlowId) -> Result<Option<FlowRecord>> {
    let row = conn.query_row(
        "SELECT id, started_at, scheme, host, port, method, path, query, status, protocol, app,
                req_size, res_size, req_blob, res_blob, req_headers, res_headers, timings_json,
                flags, state, error, source, merged_with, client_ip, sdk_json, device_id, app_id,
                sdk_call_id, name, tag, owner, screen, route_template, client_stack
         FROM flow WHERE id = ?1",
        [id as i64],
        |r| {
            let source = Source::parse(&r.get::<_, String>(21)?);
            let sdk_json: String = r.get(24)?;
            let sdk = if source != Source::Proxy || !sdk_json.is_empty() {
                Some(Arc::new(sdk_from_row(
                    &sdk_json,
                    r.get(25)?,
                    r.get(26)?,
                    r.get(27)?,
                    r.get(28)?,
                    r.get(29)?,
                    r.get(30)?,
                    r.get(31)?,
                    r.get(32)?,
                    r.get(33)?,
                )))
            } else {
                None
            };
            Ok((
                FlowRecord {
                    id: r.get::<_, i64>(0)? as u64,
                    started_at_ms: r.get(1)?,
                    scheme: r.get(2)?,
                    host: r.get(3)?,
                    port: r.get::<_, i64>(4)? as u16,
                    method: r.get(5)?,
                    path: r.get(6)?,
                    query: r.get(7)?,
                    status: r.get::<_, i64>(8)? as u16,
                    protocol: r.get(9)?,
                    app: r.get(10)?,
                    req_size: r.get::<_, i64>(11)? as u64,
                    res_size: r.get::<_, i64>(12)? as u64,
                    flags: r.get::<_, i64>(18)? as u32,
                    state: FlowState::from_u8(r.get::<_, i64>(19)? as u8),
                    error: r.get(20)?,
                    source,
                    merged_with: r.get::<_, i64>(22)? as u64,
                    client_ip: r.get(23)?,
                    sdk,
                    ..Default::default()
                },
                r.get::<_, Option<String>>(13)?,
                r.get::<_, Option<String>>(14)?,
                r.get::<_, String>(15)?,
                r.get::<_, String>(16)?,
                r.get::<_, String>(17)?,
            ))
        },
    );
    let (mut rec, req_blob, res_blob, rh, sh, th) = match row {
        Ok(v) => v,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    rec.req_headers = parse_headers(&rh);
    rec.res_headers = parse_headers(&sh);
    rec.timings = parse_timings(&th);
    if let Some(h) = req_blob {
        rec.req_body = blobs.get(&h).unwrap_or_default().into();
    }
    if let Some(h) = res_blob {
        rec.res_body = blobs.get(&h).unwrap_or_default().into();
    }
    Ok(Some(rec))
}

fn parse_headers(json: &str) -> Vec<Header> {
    serde_json::from_str::<Vec<(String, String)>>(json).unwrap_or_default()
}

fn timings_to_json(t: &Timings) -> String {
    serde_json::json!({
        "dns": t.dns_us, "connect": t.connect_us, "tls": t.tls_us, "send": t.send_us,
        "wait": t.wait_us, "receive": t.receive_us, "total": t.total_us, "reused": t.reused
    })
    .to_string()
}

fn parse_timings(json: &str) -> Timings {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    let n = |k: &str| v.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
    Timings {
        dns_us: n("dns"),
        connect_us: n("connect"),
        tls_us: n("tls"),
        send_us: n("send"),
        wait_us: n("wait"),
        receive_us: n("receive"),
        total_us: n("total"),
        reused: v.get("reused").and_then(|x| x.as_bool()).unwrap_or(false),
    }
}

/// Text worth indexing from a body: decoded, text-like, capped.
fn index_text(content_type: &str, encoding: &str, wire: &[u8]) -> String {
    if wire.is_empty() {
        return String::new();
    }
    let view = decode::build_view(
        content_type,
        encoding,
        wire,
        wire.len() as u64,
        false,
        &decode::ViewOptions {
            text_limit: FTS_BODY_LIMIT,
            json_lines: 0,
            html_blocks: 0,
            image_limit: 0,
            decode_limit: 4 * 1024 * 1024,
        },
    );
    match view.kind {
        decode::Kind::Json | decode::Kind::Text | decode::Kind::Html | decode::Kind::Form => {
            view.text
        }
        _ => String::new(),
    }
}

fn writer_loop(mut conn: Connection, shared: Arc<Shared>, rx: mpsc::Receiver<Msg>) {
    loop {
        let first = match rx.recv() {
            Ok(m) => m,
            Err(_) => return,
        };
        let mut flows: Vec<Arc<FlowRecord>> = Vec::new();
        let mut control: Vec<Msg> = Vec::new();
        let mut shutdown = false;
        sort_msg(first, &mut flows, &mut control, &mut shutdown);
        // Gather a batch: stop on control messages so ordering is preserved.
        while flows.len() < BATCH_MAX && control.is_empty() && !shutdown {
            match rx.recv_timeout(BATCH_WINDOW) {
                Ok(m) => sort_msg(m, &mut flows, &mut control, &mut shutdown),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    shutdown = true;
                    break;
                }
            }
        }

        if !flows.is_empty() {
            if let Err(e) = write_batch(&mut conn, &shared, &flows) {
                tracing::error!("store write failed: {e}");
            }
            let mut p = shared.pending.lock().unwrap();
            for f in &flows {
                p.remove(&f.id);
            }
        }
        for m in control {
            match m {
                Msg::Clear(reply) => {
                    let removed = clear_all(&mut conn, &shared).unwrap_or_else(|e| {
                        tracing::error!("clear failed: {e}");
                        0
                    });
                    let _ = reply.send(removed);
                }
                Msg::Flush(reply) => {
                    let _ = reply.send(());
                }
                Msg::Body {
                    id,
                    part,
                    data,
                    state,
                    truncated,
                } => {
                    if let Err(e) = apply_body(&mut conn, &shared, id, part, data, state, truncated)
                    {
                        tracing::error!("late body write failed: {e}");
                    }
                }
                Msg::Merge { id, other } => {
                    if let Err(e) = apply_merge(&conn, id, other) {
                        tracing::error!("merge write failed: {e}");
                    }
                }
                _ => {}
            }
        }
        if shutdown {
            // Drain anything still queued before exiting.
            let mut rest = Vec::new();
            while let Ok(m) = rx.try_recv() {
                if let Msg::Flow(f) = m {
                    rest.push(f);
                }
            }
            if !rest.is_empty() {
                let _ = write_batch(&mut conn, &shared, &rest);
            }
            let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
            return;
        }
    }
}

fn sort_msg(m: Msg, flows: &mut Vec<Arc<FlowRecord>>, control: &mut Vec<Msg>, shutdown: &mut bool) {
    match m {
        Msg::Flow(f) => flows.push(f),
        Msg::Shutdown => *shutdown = true,
        other => control.push(other),
    }
}

fn write_batch(conn: &mut Connection, shared: &Shared, flows: &[Arc<FlowRecord>]) -> Result<()> {
    // Blobs first, outside the transaction.
    let mut prepared = Vec::with_capacity(flows.len());
    for f in flows {
        let req_blob = shared.blobs.put(&f.req_body)?;
        let res_blob = shared.blobs.put(&f.res_body)?;
        let body_text = {
            let mut t = index_text(
                f.request_header("content-type"),
                f.request_header("content-encoding"),
                &f.req_body,
            );
            let r = index_text(
                f.response_header("content-type"),
                f.response_header("content-encoding"),
                &f.res_body,
            );
            if !t.is_empty() && !r.is_empty() {
                t.push('\n');
            }
            t.push_str(&r);
            t
        };
        prepared.push((req_blob, res_blob, body_text));
    }
    let tx = conn.transaction()?;
    {
        let mut ins = tx.prepare_cached(
            "INSERT OR REPLACE INTO flow(id, started_at, duration_ms, scheme, host, port, method,
                path, query, status, protocol, app, req_size, res_size, req_blob, res_blob,
                req_headers, res_headers, timings_json, flags, state, error, content_type,
                source, device_id, app_id, tag, name, owner, screen, route_template, client_stack,
                sdk_call_id, merged_with, client_ip, sdk_json)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,
                ?24,?25,?26,?27,?28,?29,?30,?31,?32,?33,?34,?35,?36)",
        )?;
        let mut del_sdk_fts = tx.prepare_cached("DELETE FROM flow_sdk_fts WHERE rowid = ?1")?;
        let mut ins_sdk_fts = tx.prepare_cached(
            "INSERT INTO flow_sdk_fts(rowid, name, tag, owner, screen, route_template, app_id, device)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        )?;
        let mut del_fts = tx.prepare_cached("DELETE FROM flow_fts WHERE rowid = ?1")?;
        let mut ins_fts = tx.prepare_cached(
            "INSERT INTO flow_fts(rowid, host, path, req_headers, res_headers, body_text)
             VALUES (?1,?2,?3,?4,?5,?6)",
        )?;
        for (f, (req_blob, res_blob, body_text)) in flows.iter().zip(prepared) {
            let s = f.summary();
            let sdk = f.sdk.as_deref();
            let sdk_str = |g: fn(&SdkInfo) -> &str| sdk.map(g).unwrap_or("").to_string();
            let req_h = serde_json::to_string(&f.req_headers).unwrap_or_else(|_| "[]".into());
            let res_h = serde_json::to_string(&f.res_headers).unwrap_or_else(|_| "[]".into());
            ins.execute(params![
                f.id as i64,
                f.started_at_ms,
                s.duration_ms,
                f.scheme,
                f.host,
                f.port as i64,
                f.method,
                f.path,
                f.query,
                f.status as i64,
                f.protocol,
                f.app,
                f.req_size as i64,
                f.res_size as i64,
                req_blob,
                res_blob,
                req_h,
                res_h,
                timings_to_json(&f.timings),
                f.flags as i64,
                f.state as u8 as i64,
                f.error,
                s.content_type,
                f.source.as_str(),
                sdk_str(|i| &i.device_id),
                sdk_str(|i| &i.app_id),
                sdk_str(|i| &i.tag),
                sdk_str(|i| &i.name),
                sdk_str(|i| &i.owner),
                sdk_str(|i| &i.screen),
                sdk_str(|i| &i.route_template),
                sdk_str(|i| &i.client_stack),
                sdk_str(|i| &i.call_id),
                f.merged_with as i64,
                f.client_ip,
                sdk.map(sdk_to_json).unwrap_or_default(),
            ])?;
            del_sdk_fts.execute([f.id as i64])?;
            if let Some(i) = sdk {
                ins_sdk_fts.execute(params![
                    f.id as i64,
                    i.name,
                    i.tag,
                    i.owner,
                    i.screen,
                    i.route_template,
                    i.app_id,
                    format!("{} {}", i.device_name, i.device_model),
                ])?;
            }
            del_fts.execute([f.id as i64])?;
            let path_q = if f.query.is_empty() {
                f.path.clone()
            } else {
                format!("{}?{}", f.path, f.query)
            };
            ins_fts.execute(params![
                f.id as i64,
                f.host,
                path_q,
                req_h,
                res_h,
                body_text
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn clear_all(conn: &mut Connection, shared: &Shared) -> Result<u64> {
    let removed: i64 = conn.query_row("SELECT COUNT(*) FROM flow", [], |r| r.get(0))?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM flow", [])?;
    tx.execute("DELETE FROM flow_fts", [])?;
    tx.execute("DELETE FROM flow_sdk_fts", [])?;
    tx.commit()?;
    shared.pending.lock().unwrap().clear();
    shared.blobs.wipe()?;
    let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
    Ok(removed as u64)
}

/// Replace one body of an already stored flow and re-index it.
fn apply_body(
    conn: &mut Connection,
    shared: &Shared,
    id: FlowId,
    part: BodyPart,
    data: Bytes,
    state: BodyState,
    truncated: bool,
) -> Result<()> {
    let Some(mut rec) = load_record(conn, &shared.blobs, id)? else {
        return Ok(());
    };
    let mut sdk = rec.sdk.as_deref().cloned().unwrap_or_default();
    match part {
        BodyPart::Request => {
            rec.req_body = data;
            sdk.req_body = state;
            if truncated {
                rec.flags |= flags::REQ_TRUNCATED;
            }
        }
        BodyPart::Response => {
            rec.res_body = data;
            sdk.res_body = state;
            if truncated {
                rec.flags |= flags::RES_TRUNCATED;
            }
        }
    }
    rec.sdk = Some(Arc::new(sdk));
    write_batch(conn, shared, &[Arc::new(rec)])
}

fn apply_merge(conn: &Connection, id: FlowId, other: FlowId) -> Result<()> {
    if other == 0 {
        // Unlink both halves of whatever `id` was part of.
        let prev: i64 = conn
            .query_row(
                "SELECT merged_with FROM flow WHERE id = ?1",
                [id as i64],
                |r| r.get(0),
            )
            .unwrap_or(0);
        for x in [id as i64, prev] {
            if x == 0 {
                continue;
            }
            conn.execute(
                "UPDATE flow SET merged_with = 0, source = CASE WHEN source = 'merged' THEN 'sdk' ELSE source END WHERE id = ?1",
                [x],
            )?;
        }
        return Ok(());
    }
    conn.execute(
        "UPDATE flow SET merged_with = ?2, source = 'merged' WHERE id = ?1",
        [id as i64, other as i64],
    )?;
    conn.execute(
        "UPDATE flow SET merged_with = ?2 WHERE id = ?1",
        [other as i64, id as i64],
    )?;
    Ok(())
}
