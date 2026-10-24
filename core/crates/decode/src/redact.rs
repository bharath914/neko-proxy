//! Strip secrets before data leaves the core: exports now, MCP later.

use serde_json::Value;

pub const MASK: &str = "[redacted]";

/// Header names that always carry credentials.
pub fn is_sensitive_header(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(
        n.as_str(),
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "x-api-key"
            | "api-key"
            | "x-auth-token"
            | "x-csrf-token"
            | "x-xsrf-token"
            | "x-amz-security-token"
    ) || is_sensitive_key(&n)
}

/// Field names (JSON keys, form fields, query params) that carry secrets.
pub fn is_sensitive_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase().replace(['-', '.', ' '], "_");
    [
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "authorization",
        "credential",
        "private_key",
        "client_secret",
        "session_id",
    ]
    .iter()
    .any(|s| k.contains(s))
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~' | '+' | '/')
}

fn looks_like_secret(tok: &str) -> bool {
    let t = tok.trim_matches(|c| matches!(c, '.' | '=' | '/' | '+' | '-' | '_'));
    if t.len() < 20 {
        return false;
    }
    // JWT: three base64url segments, header starting eyJ.
    if t.starts_with("eyJ") && t.split('.').count() == 3 {
        return true;
    }
    const PREFIXES: [&str; 14] = [
        "sk-",
        "sk_live_",
        "sk_test_",
        "pk_live_",
        "rk_live_",
        "ghp_",
        "gho_",
        "ghs_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
        "xoxa-",
        "glpat-",
        "ya29.",
    ];
    if PREFIXES.iter().any(|p| t.starts_with(p)) {
        return true;
    }
    if t.starts_with("AKIA")
        && t.len() == 20
        && t.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return true;
    }
    if t.starts_with("AIza") && t.len() == 39 {
        return true;
    }
    // Long opaque run with both letters and digits.
    t.len() >= 40
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '/' | '='))
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().any(|c| c.is_ascii_alphabetic())
}

/// Mask token-shaped values and anything after `Bearer` or `Basic`.
pub fn redact_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut after_scheme = false;
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String, after_scheme: &mut bool| {
        if token.is_empty() {
            return;
        }
        if *after_scheme || looks_like_secret(token) {
            out.push_str(MASK);
        } else {
            for (i, part) in token.split('/').enumerate() {
                if i > 0 {
                    out.push('/');
                }
                out.push_str(if looks_like_secret(part) { MASK } else { part });
            }
        }
        *after_scheme = token.eq_ignore_ascii_case("bearer") || token.eq_ignore_ascii_case("basic");
        token.clear();
    };
    for c in s.chars() {
        if is_token_char(c) {
            token.push(c);
        } else {
            flush(&mut token, &mut out, &mut after_scheme);
            out.push(c);
        }
    }
    flush(&mut token, &mut out, &mut after_scheme);
    out
}

pub fn redact_headers(headers: &[(String, String)]) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(k, v)| {
            if is_sensitive_header(k) {
                (k.clone(), MASK.to_string())
            } else {
                (k.clone(), redact_text(v))
            }
        })
        .collect()
}

/// Redact a query string or form body, field by field.
pub fn redact_pairs(s: &str) -> String {
    s.split('&')
        .map(|p| match p.split_once('=') {
            Some((k, v)) => {
                let name = k.replace('+', " ");
                let name = percent_encoding::percent_decode_str(&name).decode_utf8_lossy();
                let value = percent_encoding::percent_decode_str(v).decode_utf8_lossy();
                if is_sensitive_header(&name)
                    || looks_like_secret(&value)
                    || redact_text(&value) != value
                {
                    format!("{k}={MASK}")
                } else {
                    format!("{k}={v}")
                }
            }
            None => p.to_string(),
        })
        .collect::<Vec<_>>()
        .join("&")
}

pub fn redact_json_value(v: &mut Value) {
    redact_value(v, 0);
}
fn redact_value(v: &mut Value, depth: usize) {
    if depth > 32 {
        *v = Value::String(MASK.into());
        return;
    }
    match v {
        Value::Object(map) => {
            let sensitive_header = map
                .get("name")
                .or_else(|| map.get("key"))
                .and_then(Value::as_str)
                .is_some_and(is_sensitive_header);
            if sensitive_header && map.contains_key("value") {
                map.insert("value".into(), Value::String(MASK.into()));
            }
            for (k, item) in map.iter_mut() {
                if k.eq_ignore_ascii_case("cookies") && item.is_array() {
                    for cookie in item.as_array_mut().unwrap() {
                        if let Some(fields) = cookie.as_object_mut() {
                            fields.insert("value".into(), Value::String(MASK.into()));
                        } else {
                            *cookie = Value::String(MASK.into());
                        }
                    }
                    continue;
                }
                if is_sensitive_header(k) && !item.is_null() {
                    *item = Value::String(MASK.into());
                } else if matches!(k.as_str(), "body" | "text" | "raw") && item.is_string() {
                    let text = item.as_str().unwrap();
                    let redacted = if let Ok(mut embedded) = serde_json::from_str::<Value>(text) {
                        redact_value(&mut embedded, depth + 1);
                        embedded.to_string()
                    } else {
                        redact_body_text(text)
                    };
                    *item = Value::String(redacted);
                } else if k == "query" && item.is_string() {
                    *item = Value::String(redact_text(&redact_pairs(item.as_str().unwrap())));
                } else {
                    redact_value(item, depth + 1);
                }
            }
        }
        Value::Array(items) => {
            if items.len() == 2 && items[0].as_str().is_some_and(is_sensitive_header) {
                items[1] = Value::String(MASK.into());
            }
            for item in items {
                redact_value(item, depth + 1);
            }
        }
        Value::String(s) => {
            let text = if (s.starts_with("http://") || s.starts_with("https://"))
                && !s.chars().any(char::is_whitespace)
            {
                s.split_once('?')
                    .map(|(path, q)| format!("{path}?{}", redact_pairs(q)))
                    .unwrap_or_else(|| s.clone())
            } else {
                s.clone()
            };
            *s = redact_text(&text);
        }
        _ => {}
    }
}

/// Redact captured text, including malformed JSON and event-stream data.
/// This is separate from metadata/code strings so generated schemas stay valid.
pub fn redact_body_text(text: &str) -> String {
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return redact_json_text(text);
    }
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let newline = if line.ends_with('\n') { "\n" } else { "" };
        if let Some((key, value)) = line.trim_end_matches(['\r', '\n']).split_once(':') {
            if is_sensitive_header(key.trim().trim_matches(['\"', '\''])) {
                out.push_str(&format!("{key}: {MASK}{newline}"));
                continue;
            }
            if key.trim() == "data" {
                let data = value.trim_start();
                if data.starts_with('{') || data.starts_with('[') {
                    out.push_str(&format!("{key}: {}{newline}", redact_json_text(data)));
                    continue;
                }
                if let Some((field, _)) = data.split_once(':').or_else(|| data.split_once('=')) {
                    if is_sensitive_header(field.trim().trim_matches(['\"', '\''])) {
                        out.push_str(&format!("{key}: {MASK}{newline}"));
                        continue;
                    }
                }
            }
        }
        out.push_str(line);
    }
    redact_text(&redact_pairs(&out))
}

/// Redact a JSON document, keeping its shape. Non-JSON text falls back to
/// conservative body redaction.
pub fn redact_json_text(text: &str) -> String {
    match serde_json::from_str::<Value>(text) {
        Ok(mut v) => {
            redact_json_value(&mut v);
            serde_json::to_string(&v).unwrap_or_else(|_| redact_text(text))
        }
        Err(_) if text.trim_start().starts_with(['{', '[']) => MASK.into(),
        Err(_) => redact_body_text(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers() {
        let h = vec![
            ("Authorization".to_string(), "Bearer abc".to_string()),
            ("Cookie".to_string(), "sid=1".to_string()),
            ("Accept".to_string(), "application/json".to_string()),
            ("X-Custom-Token".to_string(), "zzz".to_string()),
        ];
        let r = redact_headers(&h);
        assert_eq!(r[0].1, MASK);
        assert_eq!(r[1].1, MASK);
        assert_eq!(r[2].1, "application/json");
        assert_eq!(r[3].1, MASK);
    }

    #[test]
    fn text_scrubbing() {
        assert_eq!(redact_text("Bearer abc123 ok"), format!("Bearer {MASK} ok"));
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        assert_eq!(
            redact_text(&format!("t={jwt}&x=1")),
            format!("t={MASK}&x=1")
        );
        assert_eq!(
            redact_text("key sk-abcdefghijklmnopqrstuvwxyz"),
            format!("key {MASK}")
        );
        assert_eq!(redact_text("plain words stay"), "plain words stay");
        assert_eq!(
            redact_text("id 550e8400-e29b-41d4-a716-446655440000"),
            "id 550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn json_keys() {
        let out = redact_json_text(
            r#"{"user":"a","password":"hunter2","nested":{"api_key":"k","n":1},"list":[{"token":"t"}]}"#,
        );
        assert!(out.contains(r#""password":"[redacted]""#));
        assert!(out.contains(r#""api_key":"[redacted]""#));
        assert!(out.contains(r#""token":"[redacted]""#));
        assert!(out.contains(r#""user":"a""#));
        assert!(out.contains(r#""n":1"#));
    }

    #[test]
    fn query_pairs() {
        assert_eq!(
            redact_pairs("q=charger&access_token=abc&page=2"),
            format!("q=charger&access_token={MASK}&page=2")
        );
    }
}

#[cfg(test)]
mod structured_tests {
    use super::*;
    #[test]
    fn encoded_keys_headers_and_embedded_json() {
        assert_eq!(
            redact_pairs("%61pi_key=short&ok=1"),
            format!("%61pi_key={MASK}&ok=1")
        );
        let mut v = serde_json::json!({"headers":[["Cookie","short-cookie"]],"header":{"key":"X-Api-Key","value":"short-key"},"body":"{\"password\":{\"nested\":\"short-password\"}}","url":"https://api.test/sk-abcdefghijklmnopqrstuvwxyz?token=short-query"});
        redact_json_value(&mut v);
        let text = v.to_string();
        for secret in [
            "short-cookie",
            "short-key",
            "short-password",
            "short-query",
            "sk-abcdefghijklmnopqrstuvwxyz",
        ] {
            assert!(!text.contains(secret), "{text}");
        }
    }
}

#[cfg(test)]
mod partial_body_tests {
    use super::*;
    #[test]
    fn malformed_json_and_sse_do_not_expose_short_secrets() {
        for body in [
            "{\"password\":\"tiny-secret",
            "api_key=tiny-secret",
            "Cookie: a=tiny-secret; b=also-secret",
            "data: {\"api_key\":\"tiny-secret\"}\n\n",
        ] {
            let result = redact_body_text(body);
            assert!(!result.contains("tiny-secret"), "{result}");
            assert!(!result.contains("also-secret"), "{result}");
        }
    }
}

#[cfg(test)]
mod cookie_body_tests {
    use super::*;
    #[test]
    fn cookie_representations_keep_structure_without_values() {
        assert!(!redact_pairs("Cookie=short-cookie").contains("short-cookie"));
        assert!(!redact_body_text("data: Cookie=short-cookie\n\n").contains("short-cookie"));
        let mut value = serde_json::json!({"cookies":[{"name":"sid","value":"short-cookie"}]});
        redact_json_value(&mut value);
        assert!(value["cookies"].is_array());
        assert_eq!(value["cookies"][0]["value"], MASK);
    }
}
