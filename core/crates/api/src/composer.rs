//! The composer's requests: saved requests, collections, environments and
//! sending. The app only edits fields; building, variables and cURL parsing
//! happen here and in the compose crate.

use crate::convert::{
    composed_from_pb, composed_to_pb, composer_to_pb, config_from_pb, environment_from_pb,
    kv_from_pb,
};
use crate::pb::{self, client_message::Body as Req, server_message::Body as Res};
use crate::server::error;
use crate::{ApiContext, ReplayCall};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn state(ctx: &ApiContext, id: u64, saved_id: String) -> pb::ServerMessage {
    let mut s = composer_to_pb(&ctx.composer.snapshot());
    s.saved_id = saved_id;
    pb::ServerMessage {
        id,
        body: Some(Res::Composer(s)),
    }
}

fn done(ctx: &ApiContext, id: u64, r: Result<(), String>) -> pb::ServerMessage {
    match r {
        Ok(()) => state(ctx, id, String::new()),
        Err(e) => error(id, "composer", e),
    }
}

pub(crate) async fn handle(ctx: Arc<ApiContext>, id: u64, body: Req) -> pb::ServerMessage {
    let c = &ctx.composer;
    match body {
        Req::ComposerGet(_) => state(&ctx, id, String::new()),
        Req::ComposerSave(r) => {
            match c.save_request(composed_from_pb(r.request.unwrap_or_default())) {
                Ok(saved) => state(&ctx, id, saved),
                Err(e) => error(id, "composer", e),
            }
        }
        Req::ComposerRemove(r) => done(&ctx, id, c.remove_request(&r.id)),
        Req::ComposerCollection(r) => {
            let res = if r.remove {
                c.remove_collection(&r.name)
            } else if !r.rename_to.is_empty() {
                c.rename_collection(&r.name, &r.rename_to)
            } else {
                c.add_collection(&r.name)
            };
            done(&ctx, id, res)
        }
        Req::ComposerEnvironments(r) => {
            let envs = r
                .environments
                .into_iter()
                .map(environment_from_pb)
                .collect();
            done(&ctx, id, c.set_environments(envs, &r.active))
        }
        Req::ComposerClearHistory(_) => done(&ctx, id, c.clear_history()),
        Req::ComposerResolve(r) => {
            let req = composed_from_pb(r.request.unwrap_or_default());
            let (built, missing, error) = build_lenient(c, &req);
            pb::ServerMessage {
                id,
                body: Some(Res::ComposerResolved(pb::ComposerResolved {
                    curl: compose::to_curl(&built),
                    method: built.method,
                    url: built.url,
                    headers: built
                        .headers
                        .into_iter()
                        .map(|(name, value)| pb::Header { name, value })
                        .collect(),
                    body: built.body,
                    missing,
                    generated: built.generated,
                    error,
                })),
            }
        }
        Req::ComposerUrl(r) => {
            let mut req = composed_from_pb(r.request.unwrap_or_default());
            if r.edit {
                compose::apply_url_text(&mut req, &r.text);
            } else {
                compose::sync_path_vars(&mut req);
            }
            let display = compose::display_url(&req);
            pb::ServerMessage {
                id,
                body: Some(Res::ComposerUrl(pb::ComposerUrl {
                    request: Some(composed_to_pb(&req)),
                    display,
                })),
            }
        }
        Req::ComposerBulk(r) => {
            let (rows, text) = match r.source {
                Some(pb::composer_bulk_request::Source::Text(t)) => (compose::bulk_parse(&t), t),
                Some(pb::composer_bulk_request::Source::Rows(l)) => {
                    let rows = kv_from_pb(l.rows);
                    let text = compose::bulk_format(&rows);
                    (rows, text)
                }
                None => (Vec::new(), String::new()),
            };
            pb::ServerMessage {
                id,
                body: Some(Res::ComposerBulk(pb::ComposerBulk {
                    rows: crate::convert::kv_to_pb_pub(&rows),
                    text,
                })),
            }
        }
        Req::ComposerCode(r) => {
            let languages = compose::code_languages()
                .iter()
                .map(|(id, label)| pb::CodeLanguage {
                    id: id.to_string(),
                    label: label.to_string(),
                })
                .collect();
            let (mut code, mut error) = (String::new(), String::new());
            if !r.lang.is_empty() {
                let req = composed_from_pb(r.request.unwrap_or_default());
                let (built, _, problem) = build_lenient(c, &req);
                if !problem.is_empty() {
                    error = problem;
                } else {
                    match compose::generate_code(&r.lang, &built) {
                        Ok(text) => code = text,
                        Err(e) => error = e,
                    }
                }
            }
            pb::ServerMessage {
                id,
                body: Some(Res::ComposerCode(pb::ComposerCode {
                    code,
                    error,
                    languages,
                })),
            }
        }
        Req::ComposerConfig(r) => done(
            &ctx,
            id,
            c.set_config(config_from_pb(r.config.unwrap_or_default())),
        ),
        Req::ComposerGlobals(r) => done(&ctx, id, c.set_globals(kv_from_pb(r.vars))),
        Req::ComposerTransfer(r) => match r.action {
            Some(pb::composer_transfer_request::Action::ImportJson(json)) => {
                let added = compose::import_postman(&json).and_then(|i| c.add_imported(i));
                match added {
                    Ok(notice) => {
                        let mut s = composer_to_pb(&c.snapshot());
                        s.notice = notice;
                        pb::ServerMessage {
                            id,
                            body: Some(Res::Composer(s)),
                        }
                    }
                    Err(e) => error(id, "composer", e),
                }
            }
            Some(pb::composer_transfer_request::Action::Export(t)) => {
                let state = c.snapshot();
                let out = if t.environment {
                    compose::export_environment(&state, &t.name)
                        .map(|j| (j, format!("{}.postman_environment.json", t.name)))
                } else {
                    compose::export_collection(&state, &t.name)
                        .map(|j| (j, format!("{}.postman_collection.json", t.name)))
                };
                match out {
                    Ok((json, filename)) => pb::ServerMessage {
                        id,
                        body: Some(Res::ComposerExport(pb::ComposerExport { json, filename })),
                    },
                    Err(e) => error(id, "composer", e),
                }
            }
            None => error(id, "composer", "nothing to transfer"),
        },
        Req::ComposerImport(r) => {
            let req = match r.source {
                Some(pb::composer_import_request::Source::Curl(text)) => compose::parse_curl(&text),
                Some(pb::composer_import_request::Source::FlowId(flow)) => {
                    from_flow(&ctx, flow).await
                }
                None => Err("nothing to import".into()),
            };
            match req {
                Ok(req) => pb::ServerMessage {
                    id,
                    body: Some(Res::Composed(composed_to_pb(&req))),
                },
                Err(e) => error(id, "composer", e),
            }
        }
        Req::ComposerSend(r) => {
            send(&ctx, id, composed_from_pb(r.request.unwrap_or_default())).await
        }
        _ => error(id, "internal", "not a composer request"),
    }
}

/// What the request would send, even with gaps: missing variables stay as
/// `{{name}}` so a preview or snippet still shows something useful. The third
/// value is why it can't be built for a reason other than a missing variable.
fn build_lenient(c: &compose::Composer, req: &compose::Request) -> (compose::Built, Vec<String>, String) {
    let (eff, vars) = c.prepare(req);
    let missing = compose::missing_variables(&eff, &vars);
    let mut error = String::new();
    let built = match compose::build(&eff, &vars) {
        Ok(b) => b,
        Err(_) => {
            let mut known = vars.clone();
            known.extend(
                missing
                    .iter()
                    .map(|m| compose::KeyValue::new(m.clone(), format!("{{{{{m}}}}}"))),
            );
            compose::build(&eff, &known).unwrap_or_else(|e| {
                error = e;
                compose::Built {
                    method: eff.method.clone(),
                    url: eff.url.clone(),
                    ..Default::default()
                }
            })
        }
    };
    (built, missing, error)
}

/// A captured flow as a request to edit.
async fn from_flow(ctx: &ApiContext, flow: u64) -> Result<compose::Request, String> {
    let store = ctx.store.clone();
    let rec = tokio::task::spawn_blocking(move || store.get(flow))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no flow {flow}"))?;
    if rec.flags & (flow::flags::TUNNEL | flow::flags::PINNED) != 0 {
        return Err("this flow is an encrypted tunnel; there is no request to open".into());
    }
    request_from_record(&rec)
}

fn request_from_record(rec: &flow::FlowRecord) -> Result<compose::Request, String> {
    let incomplete = |reason: &str| {
        format!(
            "the captured request body is incomplete ({reason}); it can't be opened for editing"
        )
    };
    if rec.flags & flow::flags::REQ_TRUNCATED != 0 {
        return Err(incomplete("the capture was truncated"));
    }
    let decoded = decode::decode_content(
        rec.request_header("content-encoding"),
        &rec.req_body,
        8 << 20,
    );
    if let Some(problem) = decoded.problem {
        return Err(incomplete(&problem));
    }
    let body = decoded.bytes;
    let mut req = compose::from_parts(&rec.method, &rec.url(), &rec.req_headers, &body);
    req.name = compose::default_name(&req);
    Ok(req)
}

async fn send(ctx: &ApiContext, id: u64, req: compose::Request) -> pb::ServerMessage {
    let started = Instant::now();
    let sent_ms = compose::now_ms();
    let (effective, vars) = ctx.composer.prepare(&req);
    let built = match compose::build(&effective, &vars) {
        Ok(b) => b,
        Err(e) => return error(id, "composer", e),
    };
    let (method, url) = (built.method.clone(), built.url.clone());
    let out = ctx
        .control
        .replay(ReplayCall {
            method: built.method,
            url: built.url,
            headers: built.headers,
            body: built.body.into(),
        })
        .await;
    let duration_ms = started.elapsed().as_secs_f64() * 1000.0;
    let (flow_id, status, err) = match out {
        Ok((f, s)) => (f, s, String::new()),
        Err(e) => (0, 0, e),
    };
    let mut size = 0;
    let mut tests = Vec::new();
    let mut cookies = Vec::new();
    if flow_id != 0 {
        // The flow finishes as its body drains; wait briefly so the app can open it.
        let store = ctx.store.clone();
        let record = tokio::task::spawn_blocking(move || {
            for _ in 0..40 {
                if let Ok(Some(rec)) = store.get(flow_id) {
                    return Some(rec);
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            None
        })
        .await
        .ok()
        .flatten();
        if let Some(rec) = record {
            size = rec.res_size;
            cookies = compose::parse_set_cookies(&rec.res_headers)
                .iter()
                .map(crate::convert::cookie_to_pb)
                .collect();
            if !effective.tests.is_empty() {
                let decoded =
                    decode::decode_content(rec.response_header("content-encoding"), &rec.res_body, 8 << 20);
                let total_ms = rec.timings.total_us as f64 / 1000.0;
                let view = compose::ResponseView {
                    status: rec.status,
                    headers: &rec.res_headers,
                    body: &decoded.bytes,
                    duration_ms: if total_ms > 0.0 { total_ms } else { duration_ms },
                    size: rec.res_size,
                };
                tests = compose::run_assertions(&effective.tests, &view)
                    .into_iter()
                    .map(|t| pb::TestResult {
                        name: t.name,
                        passed: t.passed,
                        message: t.message,
                    })
                    .collect();
            }
        }
    }
    ctx.composer.record(compose::HistoryEntry {
        request: req,
        flow_id,
        status,
        sent_ms,
        duration_ms,
        method: method.clone(),
        url: url.clone(),
        error: err.clone(),
    });
    pb::ServerMessage {
        id,
        body: Some(Res::ComposerSent(pb::ComposerSent {
            flow_id,
            status: status as u32,
            method,
            url,
            error: err,
            duration_ms,
            size,
            tests,
            cookies,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_truncated_request_bodies() {
        let rec = flow::FlowRecord {
            flags: flow::flags::REQ_TRUNCATED,
            ..Default::default()
        };
        assert_eq!(request_from_record(&rec).unwrap_err(), "the captured request body is incomplete (the capture was truncated); it can't be opened for editing");
    }

    #[test]
    fn refuses_bodies_with_decoding_problems() {
        for encoding in ["gzip", "unknown"] {
            let rec = flow::FlowRecord {
                req_headers: vec![("Content-Encoding".into(), encoding.into())],
                req_body: b"broken".as_slice().into(),
                ..Default::default()
            };
            let error = request_from_record(&rec).unwrap_err();
            assert!(error.starts_with("the captured request body is incomplete ("));
            assert!(error.ends_with("); it can't be opened for editing"));
        }
    }

    #[test]
    fn decoded_flow_drops_content_encoding() {
        let rec = flow::FlowRecord {
            req_headers: vec![("Content-Encoding".into(), "identity".into())],
            req_body: b"hello".as_slice().into(),
            ..Default::default()
        };
        let req = request_from_record(&rec).unwrap();
        assert_eq!(req.body, "hello");
        assert!(req.headers.is_empty());
    }
}
