//! JSON rendering of protocol messages and assembled flows, for
//! `sdk-test-core --record` and for tests that assert on what arrived.

use crate::assembler::Identity;
use crate::wire::{body_chunk::Part, envelope::Msg, Header};
use base64::Engine;
use flow::FlowRecord;
use serde_json::{json, Value};

fn headers(h: &[Header]) -> Value {
    Value::Array(h.iter().map(|x| json!([x.name, x.value])).collect())
}

fn timings(t: &Option<crate::wire::Timings>) -> Value {
    match t {
        Some(t) => {
            json!({"dns": t.dns, "connect": t.connect, "tls": t.tls, "request_sent": t.request_sent, "ttfb": t.ttfb, "download": t.download})
        }
        None => Value::Null,
    }
}

fn part(p: i32) -> &'static str {
    match Part::try_from(p) {
        Ok(Part::Response) => "response",
        _ => "request",
    }
}

fn bytes(b: &[u8]) -> Value {
    let mut v = json!({"data_b64": base64::engine::general_purpose::STANDARD.encode(b)});
    if let Ok(s) = std::str::from_utf8(b) {
        v["data_text"] = json!(s);
    }
    v
}

/// One protocol message as it arrived from an authenticated session.
pub fn message(who: &Identity, msg: &Msg) -> Value {
    let mut v = match msg {
        Msg::CallStart(m) => {
            json!({"type": "call_start", "call_id": m.call_id, "started_at_ms": m.started_at_ms,
            "method": m.method, "url": m.url, "headers": headers(&m.headers), "http_version": m.http_version,
            "client_stack": m.client_stack, "route_template": m.route_template, "name": m.name,
            "owner": m.owner, "tag": m.tag, "screen": m.screen,
            "request_body_truncated": m.request_body_truncated, "request_body_size": m.request_body_size})
        }
        Msg::BodyChunk(m) => {
            let mut v = json!({"type": "body_chunk", "call_id": m.call_id, "part": part(m.part), "seq": m.seq,
                "encoding": m.encoding, "last": m.last, "truncated": m.truncated});
            // Deflated chunks stay opaque here; the assembled flow has the body.
            if m.encoding == 0 {
                if let (Some(a), Some(b)) = (v.as_object_mut(), bytes(&m.data).as_object()) {
                    a.extend(b.clone());
                }
            } else {
                v["size"] = json!(m.data.len());
            }
            v
        }
        Msg::ResponseStart(m) => {
            json!({"type": "response_start", "call_id": m.call_id, "status": m.status,
            "headers": headers(&m.headers), "http_version": m.http_version, "response_body_size": m.response_body_size})
        }
        Msg::CallComplete(m) => {
            json!({"type": "call_complete", "call_id": m.call_id, "ended_at_ms": m.ended_at_ms,
            "timings": timings(&m.timings), "from_cache": m.from_cache,
            "response_body_available": m.response_body_available, "request_body_available": m.request_body_available,
            "request_redacted": m.request_redacted, "response_redacted": m.response_redacted,
            "response_body_skipped": m.response_body_skipped, "request_body_skipped": m.request_body_skipped,
            "response_body_size": m.response_body_size, "request_body_size": m.request_body_size})
        }
        Msg::CallFailed(m) => json!({"type": "call_failed", "call_id": m.call_id, "kind": m.kind,
            "message": m.message, "timings": timings(&m.timings)}),
        Msg::BodyFetchResult(m) => json!({"type": "body_fetch_result", "call_id": m.call_id,
            "part": part(m.part), "available": m.available}),
        Msg::Ping(_) => json!({"type": "ping"}),
        Msg::Pong(_) => json!({"type": "pong"}),
        other => json!({"type": "other", "debug": format!("{other:?}")}),
    };
    v["device"] = json!(who.device_id);
    v["app"] = json!(who.app_id);
    v
}

/// A finished flow as the core assembled it.
pub fn flow(rec: &FlowRecord) -> Value {
    let sdk = rec.sdk.as_deref();
    let mut v = json!({
        "type": "flow", "id": rec.id, "method": rec.method, "url": rec.url(), "status": rec.status,
        "protocol": rec.protocol, "state": rec.state as u8, "flags": rec.flags, "error": rec.error,
        "started_at_ms": rec.started_at_ms, "duration_us": rec.timings.total_us,
        "request_headers": rec.req_headers, "response_headers": rec.res_headers,
        "request_body": bytes(&rec.req_body), "response_body": bytes(&rec.res_body),
        "request_size": rec.req_size, "response_size": rec.res_size,
    });
    if let Some(i) = sdk {
        v["sdk"] = json!({
            "device_id": i.device_id, "app_id": i.app_id, "call_id": i.call_id, "name": i.name,
            "tag": i.tag, "owner": i.owner, "screen": i.screen, "route_template": i.route_template,
            "client_stack": i.client_stack, "app_version": i.app_version, "build": i.build,
            "request_body_state": i.req_body as u8, "response_body_state": i.res_body as u8,
            "request_redacted": i.req_redacted, "response_redacted": i.res_redacted,
            "from_cache": i.from_cache,
        });
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{BodyChunk, CallStart};

    #[test]
    fn renders_messages_with_the_session_identity() {
        let who = Identity {
            device_id: "d".into(),
            app_id: "a".into(),
            ..Default::default()
        };
        let v = message(
            &who,
            &Msg::CallStart(CallStart {
                call_id: "c".into(),
                url: "https://x/".into(),
                ..Default::default()
            }),
        );
        assert_eq!(v["type"], "call_start");
        assert_eq!(v["device"], "d");
        let v = message(
            &who,
            &Msg::BodyChunk(BodyChunk {
                call_id: "c".into(),
                data: b"hi".to_vec(),
                ..Default::default()
            }),
        );
        assert_eq!(v["data_text"], "hi");
        assert_eq!(v["data_b64"], "aGk=");
    }
}
