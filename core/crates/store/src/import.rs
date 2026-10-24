//! HAR input is untrusted. Bound the document, entry count and decoded bodies.
use base64::Engine;
use bytes::Bytes;
use flow::{flags, FlowRecord, FlowState, IdGen, Timings};
use serde_json::Value;
use std::sync::Arc;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub const HAR_IMPORT_LIMIT: usize = 64 * 1024 * 1024;
const BODY_CAP: usize = 8 * 1024 * 1024;

fn headers(v: &Value) -> Vec<(String, String)> {
    v.as_array()
        .into_iter()
        .flatten()
        .take(4096)
        .filter_map(|h| {
            Some((
                h["name"].as_str()?.to_owned(),
                h["value"].as_str()?.to_owned(),
            ))
        })
        .collect()
}
fn body(v: &Value) -> Result<(Bytes, bool), String> {
    let text = v["text"].as_str().unwrap_or("");
    let mut bytes = if v["encoding"] == "base64" {
        // Decode only a complete, bounded base64 prefix.
        base64::engine::general_purpose::STANDARD
            .decode(&text.as_bytes()[..text.len().min((BODY_CAP / 3 + 1) * 4)])
            .map_err(|e| format!("invalid HAR base64: {e}"))?
    } else {
        text.as_bytes()[..text.len().min(BODY_CAP)].to_vec()
    };
    let cut = bytes.len() > BODY_CAP
        || text.len()
            > if v["encoding"] == "base64" {
                (BODY_CAP / 3 + 1) * 4
            } else {
                BODY_CAP
            };
    bytes.truncate(BODY_CAP);
    Ok((Bytes::from(bytes), cut))
}
fn us(v: &Value) -> u64 {
    v.as_f64()
        .filter(|n| n.is_finite() && *n > 0.0)
        .map(|n| (n.min(86_400_000.0) * 1000.0) as u64)
        .unwrap_or(0)
}

pub fn from_har(input: &[u8], ids: &IdGen) -> Result<Vec<Arc<FlowRecord>>, String> {
    if input.len() > HAR_IMPORT_LIMIT {
        return Err("HAR exceeds 64 MiB".into());
    }
    let doc: Value = serde_json::from_slice(input).map_err(|e| e.to_string())?;
    let entries = doc["log"]["entries"]
        .as_array()
        .ok_or("HAR log.entries must be an array")?;
    if entries.len() > 10_000 {
        return Err("HAR exceeds 10,000 entries".into());
    }
    let mut out = Vec::with_capacity(entries.len());
    let mut total_body = 0usize;
    for (i, e) in entries.iter().enumerate() {
        let req = &e["request"];
        let res = &e["response"];
        let uri: http::Uri = req["url"]
            .as_str()
            .ok_or("missing request URL")?
            .parse()
            .map_err(|_| format!("entry {i}: invalid URL"))?;
        let scheme = uri
            .scheme_str()
            .filter(|s| *s == "http" || *s == "https")
            .ok_or("HAR URL must use HTTP(S)")?;
        let host = uri.host().ok_or("missing HAR host")?;
        let (req_body, req_cut) = body(&req["postData"])?;
        let (res_body, res_cut) = body(&res["content"])?;
        total_body += req_body.len() + res_body.len();
        if total_body > HAR_IMPORT_LIMIT {
            return Err("decoded HAR bodies exceed 64 MiB".into());
        }
        let mut req_headers = headers(&req["headers"]);
        let mut res_headers = headers(&res["headers"]);
        // Exported HAR bodies are decoded; do not decode them a second time.
        for h in [&mut req_headers, &mut res_headers] {
            h.retain(|(k, _)| {
                !k.eq_ignore_ascii_case("content-encoding")
                    && !k.eq_ignore_ascii_case("content-length")
            });
        }
        for (h, mime) in [
            (&mut req_headers, &req["postData"]["mimeType"]),
            (&mut res_headers, &res["content"]["mimeType"]),
        ] {
            if !h
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            {
                if let Some(mime) = mime.as_str() {
                    h.push(("content-type".into(), mime.into()));
                }
            }
        }
        let t = &e["timings"];
        let tls_us = us(&t["ssl"]);
        let status = res["status"]
            .as_u64()
            .filter(|s| *s <= 599)
            .ok_or("invalid HAR status")? as u16;
        out.push(Arc::new(FlowRecord {
            id: ids.next(),
            started_at_ms: e["startedDateTime"]
                .as_str()
                .and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())
                .map(|t| (t.unix_timestamp_nanos() / 1_000_000) as i64)
                .unwrap_or(0),
            scheme: scheme.into(),
            host: host.trim_matches(['[', ']']).into(),
            port: uri
                .port_u16()
                .unwrap_or(if scheme == "https" { 443 } else { 80 }),
            method: req["method"].as_str().unwrap_or("GET").into(),
            path: uri.path().into(),
            query: uri.query().unwrap_or("").into(),
            status,
            protocol: res["httpVersion"].as_str().unwrap_or("HTTP/1.1").into(),
            state: if status == 0 {
                FlowState::Failed
            } else {
                FlowState::Complete
            },
            req_size: req["bodySize"].as_u64().unwrap_or(req_body.len() as u64),
            res_size: res["bodySize"].as_u64().unwrap_or(res_body.len() as u64),
            req_headers,
            res_headers,
            req_body,
            res_body,
            app: e["_app"].as_str().unwrap_or("HAR import").into(),
            flags: (if req_cut { flags::REQ_TRUNCATED } else { 0 })
                | (if res_cut { flags::RES_TRUNCATED } else { 0 }),
            timings: Timings {
                dns_us: us(&t["dns"]),
                connect_us: us(&t["connect"]).saturating_sub(tls_us),
                tls_us,
                send_us: us(&t["send"]),
                wait_us: us(&t["wait"]),
                receive_us: us(&t["receive"]),
                total_us: us(&e["time"]),
                ..Default::default()
            },
            ..Default::default()
        }));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_import_roundtrip_binary_query_and_compressed_headers() {
        let record = Arc::new(FlowRecord {
            id: 1,
            started_at_ms: 1_700_000_000_000,
            scheme: "https".into(),
            host: "example.com".into(),
            port: 8443,
            method: "POST".into(),
            path: "/v1/items".into(),
            query: "page=2".into(),
            status: 201,
            protocol: "h2".into(),
            state: FlowState::Complete,
            req_headers: vec![("content-type".into(), "application/json".into())],
            req_body: Bytes::from_static(br#"{"ok":true}"#),
            req_size: 11,
            res_headers: vec![("content-type".into(), "application/octet-stream".into())],
            res_body: Bytes::from_static(&[0, 255, 1]),
            res_size: 3,
            ..Default::default()
        });
        let har = crate::to_har(&[record.clone()], false, "test");
        let input = serde_json::to_vec(&har).unwrap();
        let ids = IdGen::starting_after(100);
        let records = from_har(&input, &ids).unwrap();
        let r = &records[0];
        assert_eq!(r.id, 101);
        assert_eq!(r.url(), record.url());
        assert_eq!(r.req_body, record.req_body);
        assert_eq!(r.res_body, record.res_body);
        assert_eq!(r.started_at_ms, record.started_at_ms);
        let dir = tempfile::tempdir().unwrap();
        let store = crate::Store::open(dir.path()).unwrap();
        store.enqueue(r.clone());
        store.flush();
        assert_eq!(store.get(101).unwrap().unwrap().res_body, record.res_body);
    }
    #[test]
    fn malformed_har_is_rejected() {
        let ids = IdGen::starting_after(0);
        assert!(from_har(b"{}", &ids).is_err());
        assert!(from_har(
            br#"{"log":{"entries":[{"request":{"url":"file:///etc/passwd"}}]}}"#,
            &ids
        )
        .is_err());
    }
}
