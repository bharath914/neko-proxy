//! HAR 1.2 export.

use base64::Engine;
use decode::{decode_content, redact, Kind};
use flow::{flags, FlowRecord};
use serde_json::{json, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

const CONTENT_LIMIT: usize = 16 * 1024 * 1024;

fn iso(ms: i64) -> String {
    OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .ok()
        .and_then(|t| t.format(&Rfc3339).ok())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".into())
}

fn name_values(headers: &[(String, String)]) -> Vec<Value> {
    headers
        .iter()
        .filter(|(k, _)| !k.starts_with(':'))
        .map(|(k, v)| json!({"name": k, "value": v}))
        .collect()
}

fn cookies(headers: &[(String, String)], request: bool, redact_on: bool) -> Vec<Value> {
    let mask = |v: &str| -> String {
        if redact_on {
            redact::MASK.to_string()
        } else {
            v.to_string()
        }
    };
    let want = if request { "cookie" } else { "set-cookie" };
    let mut out = Vec::new();
    for (k, v) in headers {
        if !k.eq_ignore_ascii_case(want) {
            continue;
        }
        if request {
            for pair in v.split(';') {
                if let Some((n, val)) = pair.trim().split_once('=') {
                    out.push(json!({"name": n, "value": mask(val)}));
                }
            }
        } else if let Some((n, val)) = v.split(';').next().and_then(|p| p.split_once('=')) {
            out.push(json!({"name": n.trim(), "value": mask(val)}));
        }
    }
    out
}

fn query_string(query: &str, redact_on: bool) -> Vec<Value> {
    let q = if redact_on {
        redact::redact_pairs(query)
    } else {
        query.to_string()
    };
    decode::parse_form(&q)
        .into_iter()
        .map(|(n, v)| json!({"name": n, "value": v}))
        .collect()
}

fn ms(us: u64) -> f64 {
    (us as f64) / 1000.0
}

fn body_text(kind: Kind, content_type: &str, decoded: &[u8], redact_on: bool) -> (String, bool) {
    // Returns (text, is_base64).
    match kind {
        Kind::Json | Kind::Text | Kind::Html | Kind::Form => {
            let text = String::from_utf8_lossy(decoded).into_owned();
            let text = if !redact_on {
                text
            } else if kind == Kind::Json {
                redact::redact_json_text(&text)
            } else if kind == Kind::Form {
                redact::redact_pairs(&text)
            } else {
                redact::redact_text(&text)
            };
            let _ = content_type;
            (text, false)
        }
        _ => (
            base64::engine::general_purpose::STANDARD.encode(decoded),
            true,
        ),
    }
}

fn entry(rec: &FlowRecord, redact_on: bool) -> Value {
    let req_headers = if redact_on {
        redact::redact_headers(&rec.req_headers)
    } else {
        rec.req_headers.clone()
    };
    let res_headers = if redact_on {
        redact::redact_headers(&rec.res_headers)
    } else {
        rec.res_headers.clone()
    };
    let mut url = rec.url();
    if redact_on && !rec.query.is_empty() {
        url = url.replace(&rec.query, &redact::redact_pairs(&rec.query));
    }

    let req_ct = rec.request_header("content-type");
    let req_bytes = decode_content(
        rec.request_header("content-encoding"),
        &rec.req_body,
        CONTENT_LIMIT,
    )
    .bytes;
    let mut request = json!({
        "method": rec.method,
        "url": url,
        "httpVersion": rec.protocol,
        "cookies": cookies(&rec.req_headers, true, redact_on),
        "headers": name_values(&req_headers),
        "queryString": query_string(&rec.query, redact_on),
        "headersSize": -1,
        "bodySize": rec.req_size as i64,
    });
    if !req_bytes.is_empty() {
        let kind = decode::classify(req_ct, &req_bytes);
        let (text, b64) = body_text(kind, req_ct, &req_bytes, redact_on);
        let mut post = json!({"mimeType": req_ct, "text": text});
        if b64 {
            post["encoding"] = json!("base64");
        }
        request["postData"] = post;
    }

    let res_ct = rec.response_header("content-type");
    let dec = decode_content(
        rec.response_header("content-encoding"),
        &rec.res_body,
        CONTENT_LIMIT,
    );
    let mut content = json!({
        "size": dec.bytes.len(),
        "mimeType": res_ct,
    });
    if rec.res_size > dec.bytes.len() as u64 && dec.bytes.len() as u64 != 0 {
        content["compression"] = json!(0);
    }
    if !dec.bytes.is_empty() {
        let kind = decode::classify(res_ct, &dec.bytes);
        let (text, b64) = body_text(kind, res_ct, &dec.bytes, redact_on);
        content["text"] = json!(text);
        if b64 {
            content["encoding"] = json!("base64");
        }
    }
    if rec.flags & flags::RES_TRUNCATED != 0 {
        content["comment"] = json!("Body was larger than the capture limit and is incomplete.");
    }
    let redirect = rec.response_header("location");
    let mut response = json!({
        "status": rec.status,
        "statusText": status_text(rec.status),
        "httpVersion": rec.protocol,
        "cookies": cookies(&rec.res_headers, false, redact_on),
        "headers": name_values(&res_headers),
        "content": content,
        "redirectURL": redirect,
        "headersSize": -1,
        "bodySize": rec.res_size as i64,
    });
    if rec.status == 0 || !rec.error.is_empty() {
        response["_error"] = json!(rec.error);
    }

    let t = &rec.timings;
    let ssl = if t.tls_us > 0 { ms(t.tls_us) } else { -1.0 };
    let connect = if t.connect_us > 0 || t.tls_us > 0 {
        ms(t.connect_us + t.tls_us)
    } else {
        -1.0
    };
    let dns = if t.dns_us > 0 { ms(t.dns_us) } else { -1.0 };
    let timings = json!({
        "blocked": -1,
        "dns": dns,
        "connect": connect,
        "ssl": ssl,
        "send": ms(t.send_us),
        "wait": ms(t.wait_us),
        "receive": ms(t.receive_us),
    });
    let sum: f64 = [dns, connect, ms(t.send_us), ms(t.wait_us), ms(t.receive_us)]
        .iter()
        .filter(|v| **v >= 0.0)
        .sum();

    let mut e = json!({
        "startedDateTime": iso(rec.started_at_ms),
        "time": sum,
        "request": request,
        "response": response,
        "cache": {},
        "timings": timings,
    });
    if !rec.app.is_empty() {
        e["_app"] = json!(rec.app);
    }
    e
}

fn status_text(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
}

/// Build a HAR document. CONNECT tunnels are not HTTP exchanges and are left out.
pub fn to_har(records: &[std::sync::Arc<FlowRecord>], redact_on: bool, version: &str) -> Value {
    let entries: Vec<Value> = records
        .iter()
        .filter(|r| r.flags & flags::TUNNEL == 0)
        .map(|r| entry(r, redact_on))
        .collect();
    json!({
        "log": {
            "version": "1.2",
            "creator": {"name": "Proxy", "version": version},
            "entries": entries,
        }
    })
}
