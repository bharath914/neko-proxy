//! Keeps the URL text and the Params table in step, like Postman's URL bar:
//! typing `?a=1` adds a row, editing a row rewrites the URL.

use crate::{KeyValue, Request};
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};

/// Enough escaping for a value to survive a round trip through the URL text,
/// while `{{variables}}` and `:names` stay readable.
const VALUE: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'&')
    .add(b'+')
    .add(b'<')
    .add(b'>');
const NAME: &AsciiSet = &VALUE.add(b'=');

fn decode(s: &str) -> String {
    percent_decode_str(&s.replace('+', " "))
        .decode_utf8_lossy()
        .into_owned()
}

/// The URL as the bar shows it: the address plus every enabled parameter.
pub fn display_url(r: &Request) -> String {
    let mut url = r.url.clone();
    let query: Vec<String> = r
        .params
        .iter()
        .filter(|p| !p.disabled && !p.name.is_empty())
        .map(|p| {
            let name = utf8_percent_encode(&p.name, NAME);
            if p.value.is_empty() {
                name.to_string()
            } else {
                format!("{name}={}", utf8_percent_encode(&p.value, VALUE))
            }
        })
        .collect();
    if !query.is_empty() {
        if !url.contains('?') {
            url.push('?');
        } else if !url.ends_with('?') && !url.ends_with('&') {
            url.push('&');
        }
        url.push_str(&query.join("&"));
    }
    url
}

/// Take the text typed in the URL bar: the part before `?` becomes the address
/// and the rest becomes the enabled parameters. Disabled rows stay where they
/// were, and an enabled row keeps its description when it is edited in place.
pub fn apply_url_text(r: &mut Request, text: &str) {
    let text = text.trim();
    let text = text.split('#').next().unwrap_or(text);
    let (base, query) = match text.split_once('?') {
        Some((b, q)) => (b, q),
        None => (text, ""),
    };
    r.url = base.to_string();
    let mut fresh: std::collections::VecDeque<(String, String)> = query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (decode(k), decode(v)),
            None => (decode(p), String::new()),
        })
        .collect();
    let mut rows: Vec<KeyValue> = Vec::new();
    for old in std::mem::take(&mut r.params) {
        if old.disabled || old.name.is_empty() {
            rows.push(old);
        } else if let Some((name, value)) = fresh.pop_front() {
            rows.push(KeyValue { name, value, ..old });
        }
    }
    rows.extend(fresh.into_iter().map(|(n, v)| KeyValue::new(n, v)));
    r.params = rows;
    sync_path_vars(r);
}

/// The `:name` segments of the URL path, in order.
fn path_names(url: &str) -> Vec<String> {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let path = match rest.find('/') {
        Some(i) => &rest[i..],
        None => return Vec::new(),
    };
    let path = path.split(['?', '#']).next().unwrap_or("");
    let mut out: Vec<String> = Vec::new();
    for seg in path.split('/') {
        if let Some(name) = seg.strip_prefix(':') {
            if !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                && !out.iter().any(|n| n == name)
            {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// Make `path_vars` match the `:name` segments in the URL, keeping the values
/// already typed for names that are still there.
pub fn sync_path_vars(r: &mut Request) {
    let names = path_names(&r.url);
    let old = std::mem::take(&mut r.path_vars);
    r.path_vars = names
        .into_iter()
        .map(|n| {
            old.iter()
                .find(|o| o.name == n)
                .cloned()
                .unwrap_or_else(|| KeyValue::new(n, ""))
        })
        .collect();
}

pub(crate) fn path_variable_names(url: &str) -> Vec<String> {
    path_names(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(url: &str) -> Request {
        Request {
            url: url.into(),
            ..Default::default()
        }
    }

    #[test]
    fn typing_a_query_makes_rows_and_back() {
        let mut r = req("");
        apply_url_text(&mut r, "https://x.dev/users/:id?page=2&q=a%20b&flag#frag");
        assert_eq!(r.url, "https://x.dev/users/:id");
        assert_eq!(
            r.params,
            vec![
                KeyValue::new("page", "2"),
                KeyValue::new("q", "a b"),
                KeyValue::new("flag", "")
            ]
        );
        assert_eq!(r.path_vars, vec![KeyValue::new("id", "")]);
        assert_eq!(display_url(&r), "https://x.dev/users/:id?page=2&q=a%20b&flag");
    }

    #[test]
    fn editing_keeps_disabled_rows_and_descriptions() {
        let mut r = req("http://x/");
        r.params = vec![
            KeyValue {
                description: "page number".into(),
                ..KeyValue::new("page", "1")
            },
            KeyValue {
                disabled: true,
                ..KeyValue::new("off", "1")
            },
            KeyValue::new("q", "x"),
        ];
        assert_eq!(display_url(&r), "http://x/?page=1&q=x");
        apply_url_text(&mut r, "http://x/?page=5");
        assert_eq!(r.params.len(), 2);
        assert_eq!(r.params[0].value, "5");
        assert_eq!(r.params[0].description, "page number");
        assert!(r.params[1].disabled);
    }

    #[test]
    fn variables_and_special_characters_round_trip() {
        let mut r = req("{{base}}/a");
        r.params = vec![KeyValue::new("k&=", "{{v}} +%#")];
        let shown = display_url(&r);
        assert!(shown.contains("{{v}}"), "{shown}");
        let mut again = req("");
        apply_url_text(&mut again, &shown);
        assert_eq!(again.params, r.params);
    }

    #[test]
    fn path_vars_keep_values_and_ignore_ports() {
        let mut r = req("http://localhost:8080/a/:id/b/:name");
        r.path_vars = vec![KeyValue::new("id", "7"), KeyValue::new("gone", "1")];
        sync_path_vars(&mut r);
        assert_eq!(
            r.path_vars,
            vec![KeyValue::new("id", "7"), KeyValue::new("name", "")]
        );
    }
}
