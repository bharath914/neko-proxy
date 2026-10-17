use crate::codec::decode_content;
use flow::FlowRecord;

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Stands in for a header value the SDK hid on the device.
pub const REDACTED_PLACEHOLDER: &str = "<REDACTED: put the real value here>";

/// A `curl` command that repeats the request. Decodes the request body when it
/// was content-encoded; notes when the captured body is incomplete.
pub fn curl_command(rec: &FlowRecord) -> String {
    let mut parts: Vec<String> = vec!["curl".into()];
    let method = rec.method.to_ascii_uppercase();
    let body = if rec.req_body.is_empty() {
        Vec::new()
    } else {
        decode_content(
            rec.request_header("content-encoding"),
            &rec.req_body,
            8 << 20,
        )
        .bytes
    };
    let has_body = !body.is_empty();
    match method.as_str() {
        "GET" => {}
        "HEAD" => parts.push("-I".into()),
        "POST" if has_body => {}
        m => {
            parts.push(format!("-X {m}"));
        }
    }
    parts.push(quote(&rec.url()));

    let mut compressed = false;
    let mut redacted_any = false;
    for (k, v) in &rec.req_headers {
        let lk = k.to_ascii_lowercase();
        match lk.as_str() {
            "host" | "content-length" | "connection" | "te" | "upgrade" | "keep-alive" => continue,
            "proxy-connection" | "proxy-authorization" => continue,
            "accept-encoding" => {
                compressed = true;
                continue;
            }
            _ if lk.starts_with(':') => continue,
            _ => {}
        }
        // The SDK hides secrets on the device. Say so instead of pasting the
        // marker as if it were a real value.
        let v = if v == "[redacted]" { REDACTED_PLACEHOLDER } else { v.as_str() };
        parts.push(format!("-H {}", quote(&format!("{k}: {v}"))));
        if v == REDACTED_PLACEHOLDER {
            redacted_any = true;
        }
    }
    if compressed {
        parts.push("--compressed".into());
    }
    let mut out = String::new();
    if redacted_any {
        out.push_str("# Values hidden on the device are marked <REDACTED: put the real value here>\n");
    }
    if has_body {
        match std::str::from_utf8(&body) {
            Ok(s) => {
                parts.push(format!("--data-raw {}", quote(s)));
            }
            Err(_) => {
                out.push_str(
                    "# Request body is binary; save it to a file and use --data-binary @file\n",
                );
            }
        }
    }
    out.push_str(&parts.join(" \\\n  "));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn redacted_headers_get_a_visible_placeholder() {
        let rec = FlowRecord {
            method: "GET".into(),
            scheme: "https".into(),
            host: "a.test".into(),
            port: 443,
            path: "/x".into(),
            req_headers: vec![("Authorization".into(), "[redacted]".into())],
            ..Default::default()
        };
        let c = curl_command(&rec);
        assert!(c.contains("-H 'Authorization: <REDACTED: put the real value here>'"), "{c}");
        assert!(c.starts_with("# Values hidden on the device"));
        assert!(!c.contains("[redacted]"));
    }

    #[test]
    fn post_with_json() {
        let rec = FlowRecord {
            scheme: "https".into(),
            host: "api.example.com".into(),
            port: 443,
            method: "POST".into(),
            path: "/v1/charge".into(),
            query: "dry=1".into(),
            req_headers: vec![
                ("Host".into(), "api.example.com".into()),
                ("Content-Type".into(), "application/json".into()),
                ("Content-Length".into(), "15".into()),
                ("Accept-Encoding".into(), "gzip".into()),
            ],
            req_body: Bytes::from_static(br#"{"it's":"ok"}"#),
            ..Default::default()
        };
        let c = curl_command(&rec);
        assert!(c.starts_with("curl \\\n  'https://api.example.com/v1/charge?dry=1'"));
        assert!(c.contains("-H 'Content-Type: application/json'"));
        assert!(c.contains("--compressed"));
        assert!(c.contains(r#"--data-raw '{"it'\''s":"ok"}'"#));
        assert!(!c.contains("-X POST"));
        assert!(!c.contains("Host:"));
    }

    #[test]
    fn put_and_delete_use_x() {
        let rec = FlowRecord {
            scheme: "http".into(),
            host: "h".into(),
            port: 80,
            method: "DELETE".into(),
            path: "/a".into(),
            ..Default::default()
        };
        assert!(curl_command(&rec).contains("-X DELETE"));
    }
}
