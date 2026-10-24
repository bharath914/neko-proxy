use crate::convert;
use crate::pb::{self, client_message::Body as Req, server_message::Body as Res};
use crate::{ApiContext, ApprovalEvent, HubEvent};
use flow::FlowSummary;
use futures_util::{SinkExt, StreamExt};
use prost::Message as _;
use std::collections::HashMap;
use std::future::Future;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use store::{Kind, Query};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;

/// Subscribers get batches at this cadence.
const BATCH_INTERVAL: Duration = Duration::from_millis(16);
const MAX_MESSAGE: usize = 256 * 1024 * 1024;
const DEFAULT_PAGE: u32 = 200;
const MAX_PAGE: u32 = 1000;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("another nekod is already serving {0}")]
    AlreadyRunning(String),
    #[error("socket error on {path}: {source}")]
    Socket {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Serve the API on a Unix socket until `shutdown` resolves.
pub async fn serve(
    ctx: Arc<ApiContext>,
    socket: &Path,
    shutdown: impl Future<Output = ()>,
) -> Result<(), ApiError> {
    let sock_err = |source| ApiError::Socket {
        path: socket.display().to_string(),
        source,
    };
    if let Some(dir) = socket.parent() {
        std::fs::create_dir_all(dir).map_err(sock_err)?;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    if socket.exists() {
        // A live socket means another core owns it; a dead one is stale.
        if UnixStream::connect(socket).await.is_ok() {
            return Err(ApiError::AlreadyRunning(socket.display().to_string()));
        }
        let _ = std::fs::remove_file(socket);
    }
    let listener = UnixListener::bind(socket).map_err(sock_err)?;
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600)).map_err(sock_err)?;
    tracing::info!("api listening on {}", socket.display());

    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    let ctx = ctx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = connection(ctx, stream).await {
                            tracing::debug!("api connection ended: {e}");
                        }
                    });
                }
                Err(e) => {
                    tracing::warn!("api accept failed: {e}");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            },
            _ = &mut shutdown => break,
        }
    }
    let _ = std::fs::remove_file(socket);
    Ok(())
}

type ConnResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

fn to_query(f: Option<&pb::Filter>, since_ms: i64, until_ms: i64) -> Query {
    let (text, kind, hosts, devices, apps) = match f {
        Some(f) => (
            f.text.clone(),
            match pb::filter::Kind::try_from(f.kind).unwrap_or(pb::filter::Kind::All) {
                pb::filter::Kind::All => Kind::All,
                pb::filter::Kind::Errors => Kind::Errors,
                pb::filter::Kind::Slow => Kind::Slow,
                pb::filter::Kind::Pinned => Kind::Pinned,
            },
            f.hosts.clone(),
            f.devices.clone(),
            f.apps.clone(),
        ),
        None => (String::new(), Kind::All, Vec::new(), Vec::new(), Vec::new()),
    };
    Query {
        text,
        kind,
        since_ms,
        until_ms,
        hosts,
        devices,
        apps,
        include_hidden: false,
    }
}

fn error(id: u64, code: &str, message: impl Into<String>) -> pb::ServerMessage {
    pb::ServerMessage {
        id,
        body: Some(Res::Error(pb::ErrorResponse {
            code: code.into(),
            message: message.into(),
        })),
    }
}

async fn connection(ctx: Arc<ApiContext>, stream: UnixStream) -> ConnResult {
    let cfg = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE))
        .max_frame_size(Some(MAX_MESSAGE));
    let ws = tokio_tungstenite::accept_async_with_config(stream, Some(cfg)).await?;
    let (mut sink, mut incoming) = ws.split();

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<pb::ServerMessage>();
    let writer = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            let bytes = msg.encode_to_vec();
            if sink.send(Message::binary(bytes)).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });

    let mut sub: Option<(Query, broadcast::Receiver<HubEvent>)> = None;
    // Pushed notices, once the client sends `watch`.
    let mut pause_rx: Option<broadcast::Receiver<rules::PauseEvent>> = None;
    let mut approval_rx: Option<broadcast::Receiver<ApprovalEvent>> = None;
    let mut devices_rx: Option<tokio::sync::watch::Receiver<Arc<sdk::Snapshot>>> = None;
    let mut batch: Vec<FlowSummary> = Vec::new();
    let mut batch_index: HashMap<u64, usize> = HashMap::new();
    let mut batch_cleared = false;
    let mut batch_removed: Vec<u64> = Vec::new();
    let mut ticker = tokio::time::interval(BATCH_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let result: ConnResult = loop {
        let event = async {
            match &mut sub {
                Some((_, rx)) => rx.recv().await,
                None => std::future::pending().await,
            }
        };
        let pause_event = async {
            match &mut pause_rx {
                Some(rx) => rx.recv().await,
                None => std::future::pending().await,
            }
        };
        let approval_event = async {
            match &mut approval_rx {
                Some(rx) => rx.recv().await,
                None => std::future::pending().await,
            }
        };
        let devices_event = async {
            match &mut devices_rx {
                Some(rx) => rx.changed().await.map(|_| rx.borrow_and_update().clone()),
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            frame = incoming.next() => {
                let Some(frame) = frame else { break Ok(()) };
                let frame = match frame {
                    Ok(f) => f,
                    Err(e) => break Err(e.into()),
                };
                let data = match frame {
                    Message::Binary(b) => b,
                    Message::Close(_) => break Ok(()),
                    _ => continue,
                };
                let req = match pb::ClientMessage::decode(&data[..]) {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = out_tx.send(error(0, "bad_request", format!("undecodable message: {e}")));
                        continue;
                    }
                };
                let id = req.id;
                match req.body {
                    Some(Req::Subscribe(s)) => {
                        let q = to_query(s.filter.as_ref(), 0, 0);
                        // Subscribe before counting so nothing slips between.
                        let rx = ctx.hub.subscribe();
                        batch.clear();
                        batch_index.clear();
                        batch_cleared = false;
                        sub = Some((q, rx));
                        let total = ctx.store.count().unwrap_or(0);
                        let _ = out_tx.send(pb::ServerMessage {
                            id,
                            body: Some(Res::Subscribed(pb::SubscribeResponse { total })),
                        });
                    }
                    Some(Req::Watch(_)) => {
                        // Subscribe first so nothing slips between the list and the pushes.
                        pause_rx = Some(ctx.rules.pauses().subscribe());
                        approval_rx = Some(ctx.approvals.subscribe());
                        let _ = out_tx.send(paused_list(&ctx, id));
                    }
                    Some(Req::DevicesSubscribe(_)) => match &ctx.sdk {
                        Some(sdk) => {
                            let mut rx = sdk.subscribe();
                            let snap = rx.borrow_and_update().clone();
                            devices_rx = Some(rx);
                            let _ = out_tx.send(pb::ServerMessage {
                                id,
                                body: Some(Res::Devices(crate::sdk_api::snapshot(&snap))),
                            });
                        }
                        None => {
                            let _ = out_tx.send(error(id, "unavailable", "this core runs without the SDK listener"));
                        }
                    },
                    Some(Req::Unsubscribe(_)) => {
                        sub = None;
                        batch.clear();
                        batch_index.clear();
                        let _ = out_tx.send(pb::ServerMessage {
                            id,
                            body: Some(Res::Unsubscribed(pb::UnsubscribeResponse {})),
                        });
                    }
                    Some(body) => {
                        let ctx = ctx.clone();
                        let tx = out_tx.clone();
                        tokio::spawn(async move {
                            let res = handle(ctx, id, body).await;
                            let _ = tx.send(res);
                        });
                    }
                    None => {
                        let _ = out_tx.send(error(id, "bad_request", "empty request"));
                    }
                }
            }
            ev = devices_event => {
                match ev {
                    Ok(snap) => {
                        let _ = out_tx.send(pb::ServerMessage {
                            id: 0,
                            body: Some(Res::Devices(crate::sdk_api::snapshot(&snap))),
                        });
                    }
                    Err(_) => devices_rx = None,
                }
            }
            ev = pause_event => {
                match ev {
                    Ok(rules::PauseEvent::Paused(p)) => {
                        let _ = out_tx.send(pb::ServerMessage {
                            id: 0,
                            body: Some(Res::PauseNotice(pb::PauseNotice {
                                paused: Some(convert::paused(&p)),
                                ..Default::default()
                            })),
                        });
                    }
                    Ok(rules::PauseEvent::Released { flow_id, how, .. }) => {
                        let _ = out_tx.send(pb::ServerMessage {
                            id: 0,
                            body: Some(Res::PauseNotice(pb::PauseNotice {
                                released_flow_id: flow_id,
                                how: how.to_string(),
                                ..Default::default()
                            })),
                        });
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => pause_rx = None,
                }
            }
            ev = approval_event => {
                match ev {
                    Ok(ApprovalEvent::Requested(a)) => {
                        let _ = out_tx.send(pb::ServerMessage {
                            id: 0,
                            body: Some(Res::ApprovalNotice(pb::ApprovalNotice {
                                requested: Some(convert::approval(&a)),
                                ..Default::default()
                            })),
                        });
                    }
                    Ok(ApprovalEvent::Decided { id, allowed }) => {
                        let _ = out_tx.send(pb::ServerMessage {
                            id: 0,
                            body: Some(Res::ApprovalNotice(pb::ApprovalNotice {
                                decided_id: id,
                                allowed,
                                ..Default::default()
                            })),
                        });
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => approval_rx = None,
                }
            }
            ev = event => {
                match ev {
                    Ok(HubEvent::Upsert(s)) => {
                        if let Some((q, _)) = &sub {
                            if store::matches_summary(q, &s) {
                                match batch_index.get(&s.id) {
                                    Some(&i) => batch[i] = s,
                                    None => {
                                        batch_index.insert(s.id, batch.len());
                                        batch.push(s);
                                    }
                                }
                            }
                        }
                    }
                    Ok(HubEvent::Remove(id)) => {
                        if sub.is_some() {
                            if let Some(i) = batch_index.remove(&id) {
                                batch.remove(i);
                                for v in batch_index.values_mut() {
                                    if *v > i {
                                        *v -= 1;
                                    }
                                }
                            }
                            batch_removed.push(id);
                        }
                    }
                    Ok(HubEvent::Cleared) => {
                        batch_removed.clear();
                        batch.clear();
                        batch_index.clear();
                        batch_cleared = true;
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        batch.clear();
                        batch_index.clear();
                        let _ = out_tx.send(pb::ServerMessage {
                            id: 0,
                            body: Some(Res::Resync(pb::Resync {})),
                        });
                    }
                    Err(broadcast::error::RecvError::Closed) => sub = None,
                }
            }
            _ = ticker.tick(), if !batch.is_empty() || batch_cleared || !batch_removed.is_empty() => {
                let upserts: Vec<pb::FlowSummary> = batch.iter().map(convert::summary).collect();
                batch.clear();
                batch_index.clear();
                let cleared = std::mem::take(&mut batch_cleared);
                let removed = std::mem::take(&mut batch_removed);
                let total = ctx.store.count().unwrap_or(0);
                let _ = out_tx.send(pb::ServerMessage {
                    id: 0,
                    body: Some(Res::Batch(pb::FlowBatch { upserts, cleared, total, removed })),
                });
            }
        }
    };
    drop(out_tx);
    let _ = writer.await;
    result
}

async fn handle(ctx: Arc<ApiContext>, id: u64, body: Req) -> pb::ServerMessage {
    match body {
        Req::GetFlow(r) => get_flow(ctx, id, r).await,
        Req::Search(r) => {
            let q = to_query(r.filter.as_ref(), r.since_ms, r.until_ms);
            let limit = if r.limit == 0 {
                DEFAULT_PAGE
            } else {
                r.limit.min(MAX_PAGE)
            };
            let ctx2 = ctx.clone();
            let res =
                tokio::task::spawn_blocking(move || ctx2.store.search(&q, r.offset, limit)).await;
            match res {
                Ok(Ok(page)) => pb::ServerMessage {
                    id,
                    body: Some(Res::Search(pb::SearchResponse {
                        total: page.total,
                        offset: r.offset,
                        items: page.items.iter().map(convert::summary).collect(),
                    })),
                },
                Ok(Err(e)) => error(id, "store", e.to_string()),
                Err(e) => error(id, "internal", e.to_string()),
            }
        }
        Req::Capture(r) => {
            match pb::capture_request::Action::try_from(r.action) {
                Ok(pb::capture_request::Action::Start) => {
                    // Starting fresh also gives pinned hosts another chance.
                    ctx.control.clear_pinned();
                    ctx.control.set_recording(true);
                }
                Ok(pb::capture_request::Action::Stop) => ctx.control.set_recording(false),
                _ => {}
            }
            pb::ServerMessage {
                id,
                body: Some(Res::Capture(pb::CaptureStatus {
                    recording: ctx.control.recording(),
                    flows: ctx.store.count().unwrap_or(0),
                })),
            }
        }
        Req::Export(r) => export(ctx, id, r).await,
        Req::ImportHar(r) => {
            let result = tokio::task::spawn_blocking(move || -> Result<Vec<u64>, String> {
                let records = store::from_har(&r.data, &ctx.ids)?;
                let ids = records.iter().map(|r| r.id).collect();
                for chunk in records.chunks(32) {
                    for record in chunk {
                        ctx.store.enqueue(record.clone());
                        flow::FlowSink::publish(&ctx.hub, flow::FlowEvent::Finished(record.clone()));
                    }
                    ctx.store.flush();
                }
                Ok(ids)
            }).await;
            match result {
                Ok(Ok(ids)) => pb::ServerMessage { id, body: Some(Res::ImportedHar(pb::ImportHarResponse { count: ids.len() as u32, ids })) },
                Ok(Err(e)) => error(id, "import_har", e),
                Err(e) => error(id, "internal", e.to_string()),
            }
        },
        Req::Clear(_) => {
            let ctx2 = ctx.clone();
            match tokio::task::spawn_blocking(move || ctx2.store.clear()).await {
                Ok(Ok(removed)) => {
                    ctx.control.clear_pinned();
                    ctx.hub.cleared();
                    pb::ServerMessage {
                        id,
                        body: Some(Res::Cleared(pb::ClearResponse { removed })),
                    }
                }
                Ok(Err(e)) => error(id, "store", e.to_string()),
                Err(e) => error(id, "internal", e.to_string()),
            }
        }
        Req::Status(_) => pb::ServerMessage {
            id,
            body: Some(Res::Status(pb::StatusResponse {
                version: ctx.info.version.clone(),
                listen: ctx.info.listen.clone(),
                data_dir: ctx.info.data_dir.clone(),
                uptime_ms: ctx.info.started.elapsed().as_millis() as u64,
                flows: ctx.store.count().unwrap_or(0),
                recording: ctx.control.recording(),
                pinned_hosts: ctx.control.pinned_hosts(),
                insecure_upstream: ctx.info.insecure_upstream,
                core_pid: std::process::id(),
            })),
        },
        Req::CaInfo(_) => pb::ServerMessage {
            id,
            body: Some(Res::CaInfo(pb::CaInfo {
                cert_path: ctx.info.ca_path.clone(),
                sha256: ctx.info.ca_sha256.clone(),
                subject: ctx.info.ca_subject.clone(),
                not_after: ctx.info.ca_not_after.clone(),
                pem: ctx.info.ca_pem.clone(),
                key_storage: ctx.info.ca_key_storage.clone(),
                spki_sha256: ctx.info.ca_spki_sha256.clone(),
            })),
        },
        Req::Hosts(r) => {
            let limit = if r.limit == 0 { 500 } else { r.limit.min(2000) };
            let ctx2 = ctx.clone();
            match tokio::task::spawn_blocking(move || ctx2.store.hosts(limit)).await {
                Ok(Ok(rows)) => {
                    let pinned = ctx.control.pinned_hosts();
                    let hosts = rows
                        .into_iter()
                        .map(|h| pb::HostSummary {
                            pinned: pinned.iter().any(|p| p.eq_ignore_ascii_case(&h.host)),
                            decrypted: ctx.rules.decrypts(&h.host, 443),
                            tunnel_only: h.tunnels == h.flows,
                            host: h.host,
                            flows: h.flows,
                            errors: h.errors,
                            last_started_ms: h.last_started_ms,
                            mapped: h.mapped,
                        })
                        .collect();
                    pb::ServerMessage {
                        id,
                        body: Some(Res::Hosts(pb::HostsResponse { hosts })),
                    }
                }
                Ok(Err(e)) => error(id, "store", e.to_string()),
                Err(e) => error(id, "internal", e.to_string()),
            }
        }
        Req::Rules(_) => rules_response(&ctx, id),
        Req::SetSsl(r) => {
            let s = r.ssl.unwrap_or_default();
            let mode = match pb::SslMode::try_from(s.mode).unwrap_or(pb::SslMode::SslAll) {
                pb::SslMode::SslAll => rules::SslMode::All,
                pb::SslMode::SslListed => rules::SslMode::Listed,
            };
            match ctx.rules.set_ssl(rules::SslSettings {
                mode,
                hosts: s.hosts,
                exclude: s.exclude,
            }) {
                Ok(_) => rules_response(&ctx, id),
                Err(e) => error(id, "invalid_rule", e.to_string()),
            }
        }
        Req::SslHost(r) => {
            let done = if r.enable {
                // A host the person asks to decrypt gets another chance.
                ctx.control.unpin_host(&r.host);
                ctx.rules.enable_host(&r.host)
            } else {
                ctx.rules.disable_host(&r.host)
            };
            match done {
                Ok(_) => rules_response(&ctx, id),
                Err(e) => error(id, "invalid_rule", e.to_string()),
            }
        }
        Req::PutMapLocal(r) => {
            let Some(m) = r.rule else {
                return error(id, "bad_request", "missing rule");
            };
            let rule = rules::MapLocalRule {
                id: m.id,
                enabled: m.enabled,
                host: m.host,
                port: m.port.min(65535) as u16,
                path: m.path,
                query: m.query,
                file: m.file.into(),
                status: if m.status == 0 {
                    200
                } else {
                    m.status.min(599) as u16
                },
                content_type: m.content_type,
            };
            match ctx.rules.put_map_local(rule) {
                Ok(_) => rules_response(&ctx, id),
                Err(e) => error(id, "invalid_rule", e.to_string()),
            }
        }
        Req::RemoveRule(r) => match ctx.rules.remove(&r.id) {
            Ok(_) => rules_response(&ctx, id),
            Err(e) => error(id, "invalid_rule", e.to_string()),
        },
        Req::Unpin(r) => {
            if r.host.is_empty() {
                ctx.control.clear_pinned();
            } else {
                ctx.control.unpin_host(&r.host);
            }
            rules_response(&ctx, id)
        }
        Req::PutRule(r) => {
            let Some(rule) = r.rule else {
                return error(id, "bad_request", "missing rule");
            };
            let rule = match convert::rule_from_pb(rule) {
                Ok(r) => r,
                Err(e) => return error(id, "invalid_rule", e),
            };
            match ctx.rules.put_rule(rule) {
                Ok(_) => rules_response(&ctx, id),
                Err(e) => error(id, "invalid_rule", e.to_string()),
            }
        }
        Req::SetRuleEnabled(r) => match ctx.rules.set_enabled(&r.id, r.enabled) {
            Ok(true) => rules_response(&ctx, id),
            Ok(false) => error(id, "not_found", format!("no rule {}", r.id)),
            Err(e) => error(id, "invalid_rule", e.to_string()),
        },
        Req::Paused(_) => paused_list(&ctx, id),
        Req::Resume(r) => {
            let verdict = if r.abort {
                flow::Verdict::Abort(if r.abort_reason.is_empty() {
                    "stopped by the person".into()
                } else {
                    r.abort_reason
                })
            } else {
                flow::Verdict::Continue(flow::Edits {
                    method: r.method,
                    path_and_query: r.path,
                    status: r.status.map(|s| s.min(599) as u16),
                    headers: r
                        .replace_headers
                        .then(|| r.headers.into_iter().map(|h| (h.name, h.value)).collect()),
                    body: r.body.map(bytes::Bytes::from),
                })
            };
            if ctx.rules.pauses().resume(r.flow_id, verdict) {
                paused_list(&ctx, id)
            } else {
                error(
                    id,
                    "not_found",
                    format!(
                        "flow {} is not held at a breakpoint (it may have timed out)",
                        r.flow_id
                    ),
                )
            }
        }
        Req::Replay(r) => {
            let edits = crate::ReplayEdits {
                method: r.method,
                url: r.url,
                headers: r
                    .replace_headers
                    .then(|| r.headers.into_iter().map(|h| (h.name, h.value)).collect()),
                body: r.body.map(bytes::Bytes::from),
            };
            match crate::replay_flow(&ctx, r.id, edits).await {
                Ok(done) => pb::ServerMessage {
                    id,
                    body: Some(Res::Replay(pb::ReplayResponse {
                        flow_id: done.flow_id,
                        status: done.status as u32,
                    })),
                },
                Err(e) => error(id, "replay", e),
            }
        }
        Req::Approvals(_) => approvals_response(&ctx, id),
        Req::DecideApproval(r) => {
            if ctx.approvals.decide(&r.id, r.allow) {
                approvals_response(&ctx, id)
            } else {
                error(
                    id,
                    "not_found",
                    "that request was already answered or expired",
                )
            }
        }
        Req::SetMcpWrites(r) => {
            match ctx
                .rules
                .set_mcp_writes(convert::mcp_writes_from_pb(r.writes))
            {
                Ok(_) => mcp_info(&ctx, id),
                Err(e) => error(id, "invalid_rule", e.to_string()),
            }
        }
        Req::McpInfo(_) => mcp_info(&ctx, id),
        Req::SdkStatus(_) => pb::ServerMessage {
            id,
            body: Some(Res::SdkStatus(crate::sdk_api::status(ctx.sdk.as_deref()).await)),
        },
        Req::SdkSetEnabled(r) => {
            let Some(sdk) = &ctx.sdk else {
                return error(id, "unavailable", "this core runs without the SDK listener");
            };
            match sdk.set_enabled(r.enabled).await {
                Ok(()) => pb::ServerMessage {
                    id,
                    body: Some(Res::SdkStatus(crate::sdk_api::status(Some(sdk)).await)),
                },
                Err(e) => error(id, "sdk", format!("could not change SDK connections: {e}")),
            }
        }
        Req::SdkSetSettings(r) => {
            let Some(sdk) = &ctx.sdk else {
                return error(id, "unavailable", "this core runs without the SDK listener");
            };
            let res = sdk
                .update_settings(|s| {
                    s.enabled = r.enabled;
                    s.lan = r.lan;
                    s.auto_accept_local = r.auto_accept_local;
                })
                .await;
            match res {
                Ok(_) => pb::ServerMessage {
                    id,
                    body: Some(Res::SdkStatus(crate::sdk_api::status(Some(sdk)).await)),
                },
                Err(e) => error(id, "sdk", format!("could not change SDK settings: {e}")),
            }
        }
        Req::RegenerateSdkCert(_) => {
            let Some(sdk) = &ctx.sdk else {
                return error(id, "unavailable", "this core runs without the SDK listener");
            };
            match sdk.regenerate_certificate().await {
                Ok(()) => pb::ServerMessage {
                    id,
                    body: Some(Res::SdkStatus(crate::sdk_api::status(Some(sdk)).await)),
                },
                Err(e) => error(id, "sdk", e.to_string()),
            }
        }
        Req::DeviceAction(r) => {
            let Some(sdk) = &ctx.sdk else {
                return error(id, "unavailable", "this core runs without the SDK listener");
            };
            use pb::device_action_request::Kind;
            let ok = match Kind::try_from(r.kind).unwrap_or(Kind::Allow) {
                Kind::Allow => sdk.allow(r.pending_id),
                Kind::Deny => sdk.deny(r.pending_id),
                Kind::Revoke => sdk.revoke(&r.device_id, &r.app_id),
                Kind::Forget => sdk.forget(&r.device_id),
            };
            if ok {
                pb::ServerMessage {
                    id,
                    body: Some(Res::Ok(pb::OkResponse {})),
                }
            } else {
                error(
                    id,
                    "not_found",
                    "nothing to do: it may have expired or been answered already",
                )
            }
        }
        Req::SetDeviceConfig(r) => {
            let Some(sdk) = &ctx.sdk else {
                return error(id, "unavailable", "this core runs without the SDK listener");
            };
            let cfg = r
                .config
                .as_ref()
                .map(crate::sdk_api::config_from_pb)
                .unwrap_or_default();
            if sdk.set_config(&r.device_id, &r.app_id, cfg) {
                pb::ServerMessage {
                    id,
                    body: Some(Res::Ok(pb::OkResponse {})),
                }
            } else {
                error(id, "not_found", "that device is not known")
            }
        }
        Req::FetchBody(r) => {
            let Some(sdk) = &ctx.sdk else {
                return error(id, "unavailable", "this core runs without the SDK listener");
            };
            let part = if r.response {
                store::BodyPart::Response
            } else {
                store::BodyPart::Request
            };
            match sdk.fetch_body(r.flow_id, part).await {
                Ok(_) => {
                    let store = ctx.store.clone();
                    let _ = tokio::task::spawn_blocking(move || store.flush()).await;
                    let parts = if r.response {
                        PART_OVERVIEW | PART_RESPONSE_BODY
                    } else {
                        PART_OVERVIEW | PART_REQUEST_BODY
                    };
                    get_flow(
                        ctx,
                        id,
                        pb::GetFlowRequest {
                            id: r.flow_id,
                            parts,
                        },
                    )
                    .await
                }
                Err(e) => error(id, "fetch_failed", e.to_string()),
            }
        }
        Req::Watch(_) | Req::Subscribe(_) | Req::Unsubscribe(_) | Req::DevicesSubscribe(_) => {
            error(id, "internal", "handled elsewhere")
        }
    }
}

fn paused_list(ctx: &ApiContext, id: u64) -> pb::ServerMessage {
    pb::ServerMessage {
        id,
        body: Some(Res::Paused(pb::PausedList {
            items: ctx
                .rules
                .pauses()
                .list()
                .iter()
                .map(|p| convert::paused(p))
                .collect(),
        })),
    }
}

fn approvals_response(ctx: &ApiContext, id: u64) -> pb::ServerMessage {
    pb::ServerMessage {
        id,
        body: Some(Res::Approvals(pb::ApprovalsResponse {
            pending: ctx
                .approvals
                .list()
                .iter()
                .map(|a| convert::approval(a))
                .collect(),
        })),
    }
}

fn mcp_info(ctx: &ApiContext, id: u64) -> pb::ServerMessage {
    let writes = convert::mcp_writes_to_pb(ctx.rules.mcp_writes()) as i32;
    let info = match &ctx.mcp {
        Some(m) => pb::McpInfo {
            enabled: true,
            url: m.url.clone(),
            token: m.token.clone(),
            command: m.claude_command(),
            writes,
        },
        None => pb::McpInfo {
            writes,
            ..Default::default()
        },
    };
    pb::ServerMessage {
        id,
        body: Some(Res::McpInfo(info)),
    }
}

fn rules_response(ctx: &ApiContext, id: u64) -> pb::ServerMessage {
    let ssl = ctx.rules.ssl();
    pb::ServerMessage {
        id,
        body: Some(Res::Rules(pb::RulesResponse {
            ssl: Some(pb::SslSettings {
                mode: match ssl.mode {
                    rules::SslMode::All => pb::SslMode::SslAll,
                    rules::SslMode::Listed => pb::SslMode::SslListed,
                } as i32,
                hosts: ssl.hosts,
                exclude: ssl.exclude,
            }),
            map_local: ctx
                .rules
                .map_local()
                .into_iter()
                .map(|r| pb::MapLocalRule {
                    id: r.id,
                    enabled: r.enabled,
                    host: r.host,
                    port: r.port as u32,
                    path: r.path,
                    query: r.query,
                    file: r.file.display().to_string(),
                    status: r.status as u32,
                    content_type: r.content_type,
                })
                .collect(),
            pinned_hosts: ctx.control.pinned_hosts(),
            rules: ctx.rules.rules().iter().map(convert::rule_to_pb).collect(),
            mcp_writes: convert::mcp_writes_to_pb(ctx.rules.mcp_writes()) as i32,
        })),
    }
}

const PART_OVERVIEW: u32 = 1;
const PART_REQUEST_BODY: u32 = 2;
const PART_RESPONSE_BODY: u32 = 4;

fn build_detail(rec: &flow::FlowRecord, parts: u32) -> pb::FlowDetail {
    let opts = decode::ViewOptions::default();
    let view = |ct: &str, enc: &str, bytes: &[u8], size: u64, trunc: bool| -> pb::BodyView {
        convert::body(decode::build_view(ct, enc, bytes, size, trunc, &opts))
    };
    let mut d = pb::FlowDetail {
        summary: Some(convert::summary(&rec.summary())),
        url: rec.url(),
        ..Default::default()
    };
    if parts & PART_OVERVIEW != 0 {
        if let Ok(a) = store::export("dio", &[Arc::new(rec.clone())], "") { d.dio = String::from_utf8_lossy(&a.data).into_owned(); }
        d.request_headers = rec
            .req_headers
            .iter()
            .map(|(n, v)| pb::Header {
                name: n.clone(),
                value: v.clone(),
            })
            .collect();
        d.response_headers = rec
            .res_headers
            .iter()
            .map(|(n, v)| pb::Header {
                name: n.clone(),
                value: v.clone(),
            })
            .collect();
        d.timings = Some(convert::timings(&rec.timings));
        d.curl = decode::curl_command(rec);
    }
    if parts & PART_REQUEST_BODY != 0 {
        d.request_body = Some(view(
            rec.request_header("content-type"),
            rec.request_header("content-encoding"),
            &rec.req_body,
            rec.req_size,
            rec.flags & flow::flags::REQ_TRUNCATED != 0,
        ));
    }
    if parts & PART_RESPONSE_BODY != 0 {
        d.response_body = Some(view(
            rec.response_header("content-type"),
            rec.response_header("content-encoding"),
            &rec.res_body,
            rec.res_size,
            rec.flags & flow::flags::RES_TRUNCATED != 0,
        ));
    }
    d
}

async fn get_flow(ctx: Arc<ApiContext>, id: u64, r: pb::GetFlowRequest) -> pb::ServerMessage {
    let parts = if r.parts == 0 { PART_OVERVIEW } else { r.parts };
    let ctx2 = ctx.clone();
    let found = tokio::task::spawn_blocking(move || ctx2.store.get(r.id)).await;
    let rec = match found {
        Ok(Ok(Some(rec))) => rec,
        Ok(Ok(None)) => {
            // Not stored yet: report what the hub knows.
            return match ctx.hub.inflight(r.id) {
                Some(s) => pb::ServerMessage {
                    id,
                    body: Some(Res::Flow(pb::FlowDetail {
                        summary: Some(convert::summary(&s)),
                        ..Default::default()
                    })),
                },
                None => error(id, "not_found", format!("no flow {}", r.id)),
            };
        }
        Ok(Err(e)) => return error(id, "store", e.to_string()),
        Err(e) => return error(id, "internal", e.to_string()),
    };

    let store = ctx.store.clone();
    let res = tokio::task::spawn_blocking(move || {
        let mut d = build_detail(&rec, parts);
        d.sdk = crate::sdk_api::context(&rec);
        if rec.source == flow::Source::Merged && rec.merged_with != 0 {
            if let Ok(Some(wire)) = store.get(rec.merged_with) {
                d.wire = Some(Box::new(build_detail(&wire, parts)));
            }
        }
        d
    })
    .await;
    match res {
        Ok(d) => pb::ServerMessage {
            id,
            body: Some(Res::Flow(d)),
        },
        Err(e) => error(id, "internal", e.to_string()),
    }
}

async fn export(ctx: Arc<ApiContext>, id: u64, r: pb::ExportRequest) -> pb::ServerMessage {
    let q = to_query(r.filter.as_ref(), 0, 0);
    let kind = r.kind.clone();
    let ctx2 = ctx.clone();
    let res = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, u32, String), String> {
        let ids = if r.ids.is_empty() {
            ctx2.store.ids(&q, 100_000).map_err(|e| e.to_string())?
        } else {
            r.ids.clone()
        };
        if ids.len() > 10_000 { return Err("export limited to 10,000 flows".into()); }
        let mut total = 0usize;
        let mut recs = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(rec) = ctx2.store.get(id).map_err(|e| e.to_string())? {
                total += rec.req_body.len() + rec.res_body.len();
                if total > 32 * 1024 * 1024 { return Err("export source bodies exceed 32 MiB".into()); }
                recs.push(rec);
            }
        }
        let artifact = store::export(&kind, &recs, &ctx2.info.version)?;
        Ok((artifact.data, artifact.count, artifact.extension.into()))
    })
    .await;
    match res {
        Ok(Ok((data, count, extension))) => {
            let now = time::OffsetDateTime::now_utc();
            let filename = format!(
                "proxy-{:04}{:02}{:02}-{:02}{:02}{:02}.{}",
                now.year(),
                u8::from(now.month()),
                now.day(),
                now.hour(),
                now.minute(),
                now.second(),
                extension
            );
            pb::ServerMessage {
                id,
                body: Some(Res::Export(pb::ExportResponse {
                    data,
                    filename,
                    count,
                })),
            }
        }
        Ok(Err(e)) => error(id, "export", e),
        Err(e) => error(id, "internal", e.to_string()),
    }
}
