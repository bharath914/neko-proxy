//! Bounded local heuristics. Findings contain no captured values.
use flow::{flags, FlowRecord};
pub const ISSUE_MASK: u32 = flags::SLOW
    | flags::DUPLICATE
    | flags::LARGE_PAYLOAD
    | flags::MISSING_CACHE
    | flags::N_PLUS_ONE
    | flags::SENSITIVE;
pub fn issue_labels(bits: u32) -> Vec<String> {
    [
        (flags::SLOW, "Slow response"),
        (flags::DUPLICATE, "Possible duplicate"),
        (flags::LARGE_PAYLOAD, "Large payload"),
        (flags::MISSING_CACHE, "Missing cache headers"),
        (flags::N_PLUS_ONE, "Possible N+1"),
        (flags::SENSITIVE, "Possible sensitive data"),
    ]
    .into_iter()
    .filter(|(bit, _)| bits & bit != 0)
    .map(|(_, label)| label.into())
    .collect()
}
fn luhn(s: &str) -> bool {
    let digits: Vec<u32> = s
        .chars()
        .filter(|c| c.is_ascii_digit())
        .map(|c| c as u32 - '0' as u32)
        .collect();
    if !(13..=19).contains(&digits.len()) || digits.iter().all(|d| *d == digits[0]) {
        return false;
    }
    digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 1 {
                let x = d * 2;
                if x > 9 {
                    x - 9
                } else {
                    x
                }
            } else {
                d
            }
        })
        .sum::<u32>()
        % 10
        == 0
}
fn text_sensitive(text: &str) -> bool {
    if crate::redact::redact_text(text) != text {
        return true;
    }
    text.split_whitespace().take(10_000).any(|word| {
        let word = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && !"@.-_".contains(c));
        let email = word.split_once('@').is_some_and(|(left, right)| {
            !left.is_empty() && right.contains('.') && !right.ends_with('.')
        });
        email || (word.chars().all(|c| c.is_ascii_digit() || c == '-') && luhn(word))
    })
}
fn json_sensitive(v: &serde_json::Value, depth: usize, budget: &mut usize) -> bool {
    if depth > 16 || *budget == 0 {
        return false;
    }
    *budget -= 1;
    match v {
        serde_json::Value::Object(m) => m.iter().any(|(k, v)| {
            (crate::redact::is_sensitive_key(k)
                && !v.is_null()
                && v.as_str() != Some(crate::redact::MASK))
                || json_sensitive(v, depth + 1, budget)
        }),
        serde_json::Value::Array(a) => a.iter().any(|v| json_sensitive(v, depth + 1, budget)),
        serde_json::Value::String(s) => text_sensitive(s),
        _ => false,
    }
}
pub fn sensitive(rec: &FlowRecord) -> bool {
    for (k, v) in rec.req_headers.iter().chain(&rec.res_headers).take(4096) {
        if !v.is_empty()
            && v != crate::redact::MASK
            && (crate::redact::is_sensitive_header(k) || text_sensitive(v))
        {
            return true;
        }
    }
    if crate::parse_form(&rec.query)
        .iter()
        .any(|(k, v)| (!v.is_empty() && crate::redact::is_sensitive_key(k)) || text_sensitive(v))
    {
        return true;
    }
    for (body, encoding) in [
        (&rec.req_body, rec.request_header("content-encoding")),
        (&rec.res_body, rec.response_header("content-encoding")),
    ] {
        let decoded = crate::decode_content(encoding, &body[..body.len().min(65536)], 65536);
        let text = String::from_utf8_lossy(&decoded.bytes);
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
            if json_sensitive(&json, 0, &mut 2000) {
                return true;
            }
        } else if text_sensitive(&text) {
            return true;
        }
    }
    false
}
pub fn static_flags(rec: &FlowRecord) -> u32 {
    let mut bits = 0;
    if rec.timings.total_us >= 1_000_000 {
        bits |= flags::SLOW;
    }
    if rec.req_size >= 1024 * 1024 || rec.res_size >= 1024 * 1024 {
        bits |= flags::LARGE_PAYLOAD;
    }
    if rec.method == "GET"
        && rec.status == 200
        && !rec.res_headers.is_empty()
        && ["cache-control", "expires", "etag", "last-modified"]
            .iter()
            .all(|h| rec.response_header(h).is_empty())
        && rec.response_header("content-type").split(';').next() != Some("text/event-stream")
    {
        bits |= flags::MISSING_CACHE;
    }
    if sensitive(rec) {
        bits |= flags::SENSITIVE;
    }
    bits
}
/// Numeric and UUID path segments are grouped only for heuristic analysis.
pub fn route_path(path: &str) -> String {
    path.split('/')
        .take(100)
        .map(|part| {
            if (!part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
                || (part.len() == 36 && part.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'))
            {
                "{id}"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scanner_and_flags() {
        let rec = FlowRecord {
            method: "GET".into(),
            status: 200,
            res_headers: vec![("content-type".into(), "application/json".into())],
            res_body: bytes::Bytes::from_static(br#"{"password":"hidden"}"#),
            res_size: 2_000_000,
            timings: flow::Timings {
                total_us: 2_000_000,
                ..Default::default()
            },
            ..Default::default()
        };
        let bits = static_flags(&rec);
        assert_ne!(bits & flags::SENSITIVE, 0);
        assert_eq!(issue_labels(bits).len(), 4);
        assert!(text_sensitive("card 4242424242424242"));
        assert!(text_sensitive("a@example.com"));
        assert!(!text_sensitive("hello world 123"));
        assert_eq!(route_path("/users/12/posts/88"), "/users/{id}/posts/{id}");
    }
}
