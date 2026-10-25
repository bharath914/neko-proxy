//! A composed request to method, URL, headers and body bytes.

use crate::urlsync::path_variable_names;
use crate::{AuthKind, BodyKind, KeyValue, Request};
use base64::Engine;
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Everything but the unreserved characters, so a value round-trips exactly.
const COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// What goes on the wire.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Built {
    pub method: String,
    /// Absolute URL with the query parameters added.
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// For a form-data body, the fields as written, so a snippet can use `-F`.
    pub form: Vec<FormField>,
    /// Names of the headers added for the person (auth, content type, defaults).
    pub generated: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormField {
    pub name: String,
    /// The text, or the file's path when `file` is set.
    pub value: String,
    pub file: bool,
}

fn enabled(rows: &[KeyValue]) -> impl Iterator<Item = &KeyValue> {
    rows.iter()
        .filter(|r| !r.disabled && !r.name.trim().is_empty())
}

fn encode(s: &str) -> String {
    utf8_percent_encode(s, COMPONENT).to_string()
}

/// Values for `{{$timestamp}}`, `{{$randomInt}}` and `{{$guid}}`, which change on
/// every send.
fn dynamic(name: &str) -> Option<String> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let random = || {
        let mut h = blake3::Hasher::new();
        h.update(&now.as_nanos().to_le_bytes());
        h.update(&COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
        h.update(&std::process::id().to_le_bytes());
        *h.finalize().as_bytes()
    };
    match name {
        "$timestamp" => Some(now.as_secs().to_string()),
        "$timestampMs" => Some(now.as_millis().to_string()),
        "$randomInt" => {
            let b = random();
            Some((u16::from_le_bytes([b[0], b[1]]) % 1000).to_string())
        }
        "$guid" | "$uuid" | "$randomUUID" => {
            let mut b = random();
            b[6] = (b[6] & 0x0f) | 0x40;
            b[8] = (b[8] & 0x3f) | 0x80;
            let h: String = b[..16].iter().map(|x| format!("{x:02x}")).collect();
            Some(format!(
                "{}-{}-{}-{}-{}",
                &h[0..8],
                &h[8..12],
                &h[12..16],
                &h[16..20],
                &h[20..32]
            ))
        }
        _ => None,
    }
}

struct Vars<'a> {
    map: HashMap<&'a str, &'a str>,
    missing: Vec<String>,
    /// A problem that stops the request from being built, such as bad JSON.
    error: Option<String>,
}

impl<'a> Vars<'a> {
    fn new(vars: &'a [KeyValue]) -> Vars<'a> {
        Vars {
            map: enabled(vars)
                .map(|v| (v.name.trim(), v.value.as_str()))
                .collect(),
            missing: Vec::new(),
            error: None,
        }
    }

    /// Fill every `{{name}}`. Unknown names stay as written and are noted.
    /// Braces that cannot hold a name (`{{}}`, a `{` or a line break inside)
    /// are left as text, so `{{{id}}}` keeps its outer braces.
    fn fill(&mut self, s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(start) = rest.find("{{") {
            let Some(len) = rest[start + 2..].find("}}") else {
                break;
            };
            let name = rest[start + 2..start + 2 + len].trim();
            if name.is_empty() || name.contains(['{', '\n', '\r']) {
                out.push_str(&rest[..start + 1]);
                rest = &rest[start + 1..];
                continue;
            }
            out.push_str(&rest[..start]);
            match self
                .map
                .get(name)
                .map(|v| v.to_string())
                .or_else(|| dynamic(name))
            {
                Some(v) => out.push_str(&v),
                None => {
                    if !self.missing.iter().any(|m| m == name) {
                        self.missing.push(name.to_string());
                    }
                    out.push_str(&rest[start..start + 4 + len]);
                }
            }
            rest = &rest[start + 4 + len..];
        }
        out.push_str(rest);
        out
    }
}

/// Every `{{name}}` the request uses that the variables do not define.
pub fn missing_variables(r: &Request, vars: &[KeyValue]) -> Vec<String> {
    let mut v = Vars::new(vars);
    expand(r, &mut v);
    v.missing
}

/// Fill in variables, add the query, auth and body, and default the headers a
/// client would send.
pub fn build(r: &Request, vars: &[KeyValue]) -> Result<Built, String> {
    let mut v = Vars::new(vars);
    let built = expand(r, &mut v);
    if let Some(e) = v.error.take() {
        return Err(e);
    }
    if !v.missing.is_empty() {
        let names: Vec<String> = v.missing.iter().map(|m| format!("{{{{{m}}}}}")).collect();
        return Err(format!(
            "No value for {}. Add it to the active environment, or pick an environment that has it.",
            names.join(", ")
        ));
    }
    if !built
        .method
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
    {
        return Err("the method must be an HTTP token (letters, digits or !#$%&'*+-.^_`|~)".into());
    }
    let host = built.url.split_once("://").map_or("", |(_, rest)| rest);
    if host.is_empty() || host.starts_with(['/', '?']) {
        return Err("Enter a URL to send the request to.".into());
    }
    Ok(built)
}

fn append_query(mut url: String, query: &[String]) -> String {
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

/// Replace `:name` path segments with their values. A name with no value stays
/// as written.
fn fill_path_vars(url: &str, vars: &[KeyValue], v: &mut Vars) -> String {
    let names = path_variable_names(url);
    if names.is_empty() {
        return url.to_string();
    }
    let (head, rest) = match url.split_once("://") {
        Some((scheme, rest)) => (format!("{scheme}://"), rest),
        None => (String::new(), url),
    };
    let Some(slash) = rest.find('/') else {
        return url.to_string();
    };
    let (host, tail) = rest.split_at(slash);
    let end = tail.find(['?', '#']).unwrap_or(tail.len());
    let (path, after) = tail.split_at(end);
    let segments: Vec<String> = path
        .split('/')
        .map(|seg| {
            let Some(name) = seg.strip_prefix(':') else {
                return seg.to_string();
            };
            match vars.iter().find(|p| !p.disabled && p.name == name) {
                Some(p) if !p.value.is_empty() => encode(&v.fill(&p.value)),
                _ => seg.to_string(),
            }
        })
        .collect();
    format!("{head}{host}{}{after}", segments.join("/"))
}

fn quote_param(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace(['\r', '\n'], " ")
}

fn guess_type(file_name: &str) -> &'static str {
    match file_name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "json" => "application/json",
        "txt" | "log" | "md" => "text/plain",
        "html" | "htm" => "text/html",
        "xml" => "application/xml",
        "csv" => "text/csv",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

fn expand(r: &Request, v: &mut Vars) -> Built {
    let mut method = v.fill(r.method.trim()).to_ascii_uppercase();
    if method.is_empty() {
        method = "GET".into();
    }

    let mut url = v.fill(r.url.trim());
    // The fragment never goes on the wire.
    if let Some(i) = url.find('#') {
        url.truncate(i);
    }
    if !url.is_empty() && !url.contains("://") {
        url = format!("http://{url}");
    }
    url = fill_path_vars(&url, &r.path_vars, v);
    let query: Vec<String> = enabled(&r.params)
        .map(|p| {
            let name = encode(&v.fill(p.name.trim()));
            let value = v.fill(&p.value);
            if value.is_empty() {
                name
            } else {
                format!("{name}={}", encode(&value))
            }
        })
        .collect();
    url = append_query(url, &query);

    let mut headers: Vec<(String, String)> = enabled(&r.headers)
        .map(|h| (v.fill(h.name.trim()), v.fill(&h.value)))
        .collect();
    let has = |headers: &[(String, String)], n: &str| {
        headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(n))
    };

    let mut generated: Vec<String> = Vec::new();
    match r.auth {
        AuthKind::None | AuthKind::Inherit => {}
        AuthKind::ApiKey => {
            let name = v.fill(r.auth_key.trim());
            let value = v.fill(&r.auth_value);
            if !name.is_empty() {
                if r.auth_in_query {
                    let pair = if value.is_empty() {
                        encode(&name)
                    } else {
                        format!("{}={}", encode(&name), encode(&value))
                    };
                    url = append_query(url, &[pair]);
                } else if !has(&headers, &name) {
                    generated.push(name.clone());
                    headers.push((name, value));
                }
            }
        }
        _ if has(&headers, "authorization") => {}
        AuthKind::Bearer => {
            let token = v.fill(r.auth_token.trim());
            let prefix = v.fill(r.auth_prefix.trim());
            let prefix = if prefix.is_empty() { "Bearer".to_string() } else { prefix };
            generated.push("Authorization".into());
            headers.push(("Authorization".into(), format!("{prefix} {token}")));
        }
        AuthKind::Basic => {
            let pair = format!("{}:{}", v.fill(&r.auth_user), v.fill(&r.auth_password));
            let b64 = base64::engine::general_purpose::STANDARD.encode(pair);
            generated.push("Authorization".into());
            headers.push(("Authorization".into(), format!("Basic {b64}")));
        }
    }
    let mut form_fields: Vec<FormField> = Vec::new();
    let mut force_content_type = false;
    let (body, content_type): (Vec<u8>, Option<String>) = match r.body_kind {
        BodyKind::None => (Vec::new(), None),
        BodyKind::Json => (v.fill(&r.body).into_bytes(), Some("application/json".into())),
        BodyKind::Text => (
            v.fill(&r.body).into_bytes(),
            Some("text/plain; charset=utf-8".into()),
        ),
        BodyKind::Xml => (v.fill(&r.body).into_bytes(), Some("application/xml".into())),
        BodyKind::Html => (
            v.fill(&r.body).into_bytes(),
            Some("text/html; charset=utf-8".into()),
        ),
        BodyKind::Javascript => (
            v.fill(&r.body).into_bytes(),
            Some("application/javascript".into()),
        ),
        BodyKind::Form => {
            let pairs: Vec<String> = enabled(&r.form)
                .map(|f| {
                    format!(
                        "{}={}",
                        encode(&v.fill(f.name.trim())),
                        encode(&v.fill(&f.value))
                    )
                })
                .collect();
            (
                pairs.join("&").into_bytes(),
                Some("application/x-www-form-urlencoded".into()),
            )
        }
        BodyKind::Graphql => {
            let vars_text = v.fill(&r.graphql_variables);
            let variables = if vars_text.trim().is_empty() {
                serde_json::json!({})
            } else {
                match serde_json::from_str::<serde_json::Value>(&vars_text) {
                    Ok(x) => x,
                    Err(e) => {
                        v.error.get_or_insert(format!("The GraphQL variables are not valid JSON: {e}"));
                        serde_json::json!({})
                    }
                }
            };
            let payload = serde_json::json!({ "query": v.fill(&r.body), "variables": variables });
            (
                payload.to_string().into_bytes(),
                Some("application/json".into()),
            )
        }
        BodyKind::Binary => {
            let path = v.fill(r.binary_path.trim());
            let bytes = if path.is_empty() {
                Vec::new()
            } else {
                match std::fs::read(&path) {
                    Ok(b) => b,
                    Err(e) => {
                        v.error.get_or_insert(format!("Could not read {path}: {e}"));
                        Vec::new()
                    }
                }
            };
            (bytes, Some("application/octet-stream".into()))
        }
        BodyKind::FormData => {
            force_content_type = true;
            let boundary = format!("----NekoBoundary{}", dynamic("$guid").unwrap_or_default().replace('-', ""));
            let mut body: Vec<u8> = Vec::new();
            for f in enabled(&r.form) {
                let name = v.fill(f.name.trim());
                let value = v.fill(&f.value);
                body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
                if f.file {
                    let file_name = std::path::Path::new(&value)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    match std::fs::read(&value) {
                        Ok(bytes) => {
                            body.extend_from_slice(
                                format!(
                                    "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
                                    quote_param(&name),
                                    quote_param(&file_name),
                                    guess_type(&file_name)
                                )
                                .as_bytes(),
                            );
                            body.extend_from_slice(&bytes);
                        }
                        Err(e) => {
                            v.error.get_or_insert(format!("Could not read {value}: {e}"));
                        }
                    }
                } else {
                    body.extend_from_slice(
                        format!(
                            "Content-Disposition: form-data; name=\"{}\"\r\n\r\n",
                            quote_param(&name)
                        )
                        .as_bytes(),
                    );
                    body.extend_from_slice(value.as_bytes());
                }
                body.extend_from_slice(b"\r\n");
                form_fields.push(FormField {
                    name,
                    value,
                    file: f.file,
                });
            }
            body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
            (body, Some(format!("multipart/form-data; boundary={boundary}")))
        }
    };
    if let Some(ct) = content_type {
        if force_content_type {
            headers.retain(|(k, _)| !k.eq_ignore_ascii_case("content-type"));
        }
        if !has(&headers, "content-type") {
            generated.push("Content-Type".into());
            headers.push(("Content-Type".into(), ct));
        }
    }
    if !has(&headers, "user-agent") {
        generated.push("User-Agent".into());
        headers.push((
            "User-Agent".into(),
            format!("Neko/{}", env!("CARGO_PKG_VERSION")),
        ));
    }
    if !has(&headers, "accept") {
        generated.push("Accept".into());
        headers.push(("Accept".into(), "*/*".into()));
    }
    Built {
        method,
        url,
        headers,
        body,
        form: form_fields,
        generated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(n: &str, v: &str) -> KeyValue {
        KeyValue::new(n, v)
    }

    #[test]
    fn methods_must_be_http_tokens() {
        for method in ["BAD METHOD", "GET;evil", "GET\r\nInjected", "é", "GET/POST"] {
            let r = Request {
                method: method.into(),
                url: "https://x.dev/".into(),
                ..Default::default()
            };
            assert!(build(&r, &[])
                .unwrap_err()
                .contains("method must be an HTTP token"));
        }
        let r = Request {
            method: "{{method}}".into(),
            url: "https://x.dev/".into(),
            ..Default::default()
        };
        assert!(build(&r, &[KeyValue::new("method", "BAD METHOD")]).is_err());
        assert!(build(&r, &[KeyValue::new("method", "AZ09!#$%&'*+-.^_`|~")]).is_ok());
    }

    #[test]
    fn fills_variables_params_auth_and_json() {
        let r = Request {
            method: "post".into(),
            url: "{{base}}/users#top".into(),
            params: vec![
                kv("q", "a b&c"),
                kv("flag", ""),
                KeyValue {
                    disabled: true,
                    ..kv("off", "1")
                },
            ],
            headers: vec![kv("X-Env", "{{ env }}")],
            body_kind: BodyKind::Json,
            body: r#"{"name": "{{name}}"}"#.into(),
            auth: AuthKind::Bearer,
            auth_token: "{{token}}".into(),
            ..Default::default()
        };
        let vars = vec![
            kv("base", "https://api.example.com"),
            kv("env", "staging"),
            kv("name", "Ada"),
            kv("token", "t0k"),
        ];
        let b = build(&r, &vars).unwrap();
        assert_eq!(b.method, "POST");
        assert_eq!(b.url, "https://api.example.com/users?q=a%20b%26c&flag");
        assert!(b.headers.contains(&("X-Env".into(), "staging".into())));
        assert!(b
            .headers
            .contains(&("Authorization".into(), "Bearer t0k".into())));
        assert!(b
            .headers
            .contains(&("Content-Type".into(), "application/json".into())));
        assert_eq!(b.body, br#"{"name": "Ada"}"#);
    }

    #[test]
    fn reports_missing_variables() {
        let r = Request {
            url: "{{base}}/x?{{q}}".into(),
            ..Default::default()
        };
        assert_eq!(missing_variables(&r, &[]), vec!["base", "q"]);
        let e = build(&r, &[]).unwrap_err();
        assert!(e.contains("{{base}}, {{q}}"), "{e}");
    }

    #[test]
    fn form_basic_auth_and_explicit_headers_win() {
        let r = Request {
            method: "PUT".into(),
            url: "example.com/login?a=1".into(),
            params: vec![kv("b", "2")],
            headers: vec![
                kv("content-type", "application/x-custom"),
                kv("user-agent", "me"),
            ],
            body_kind: BodyKind::Form,
            form: vec![kv("user", "a@b.c"), kv("pass", "p w")],
            auth: AuthKind::Basic,
            auth_user: "u".into(),
            auth_password: "p".into(),
            ..Default::default()
        };
        let b = build(&r, &[]).unwrap();
        assert_eq!(b.url, "http://example.com/login?a=1&b=2");
        assert_eq!(b.body, b"user=a%40b.c&pass=p%20w");
        assert!(b
            .headers
            .contains(&("Authorization".into(), "Basic dTpw".into())));
        assert_eq!(
            b.headers
                .iter()
                .filter(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                .count(),
            1
        );
        assert!(b.headers.contains(&("user-agent".into(), "me".into())));
    }

    #[test]
    fn dynamic_variables_change() {
        let r = Request {
            url: "http://x/{{$guid}}/{{$timestamp}}/{{$randomInt}}".into(),
            ..Default::default()
        };
        let a = build(&r, &[]).unwrap().url;
        let b = build(&r, &[]).unwrap().url;
        assert_ne!(a, b);
        let guid = a.split('/').nth(3).unwrap();
        assert_eq!(guid.len(), 36);
        assert_eq!(&guid[14..15], "4");
    }

    #[test]
    fn empty_url_is_an_error() {
        assert!(build(&Request::default(), &[]).is_err());
        for url in ["https://", "https:///path", "{{base}}"] {
            let r = Request {
                url: url.into(),
                ..Default::default()
            };
            assert!(build(&r, &[kv("base", "")]).is_err(), "{url}");
        }
    }

    #[test]
    fn braces_that_hold_no_name_stay_text() {
        let r = Request {
            url: "http://x/{{{id}}}".into(),
            body_kind: BodyKind::Text,
            body: "{{}} {{ {{id}} {{a\nb}} {{".into(),
            ..Default::default()
        };
        let vars = [kv("id", "7")];
        assert!(missing_variables(&r, &vars).is_empty());
        let b = build(&r, &vars).unwrap();
        assert_eq!(b.url, "http://x/{7}");
        assert_eq!(b.body, b"{{}} {{ 7 {{a\nb}} {{");
    }

    #[test]
    fn path_variables_fill_only_the_path() {
        let r = Request {
            url: "http://localhost:8080/users/:id/files/:name?x=:keep".into(),
            path_vars: vec![kv("id", "a b"), kv("name", "")],
            ..Default::default()
        };
        let b = build(&r, &[]).unwrap();
        assert_eq!(b.url, "http://localhost:8080/users/a%20b/files/:name?x=:keep");
    }

    #[test]
    fn api_key_goes_in_a_header_or_the_query() {
        let mut r = Request {
            url: "http://x/a".into(),
            params: vec![kv("p", "1")],
            auth: AuthKind::ApiKey,
            auth_key: "X-Api-Key".into(),
            auth_value: "{{k}}".into(),
            ..Default::default()
        };
        let vars = [kv("k", "s e")];
        let b = build(&r, &vars).unwrap();
        assert!(b.headers.contains(&("X-Api-Key".into(), "s e".into())));
        assert!(b.generated.contains(&"X-Api-Key".to_string()));
        r.auth_in_query = true;
        let b = build(&r, &vars).unwrap();
        assert_eq!(b.url, "http://x/a?p=1&X%2DApi%2DKey=s%20e".replace("%2D", "-"));
        assert!(!b.headers.iter().any(|(k, _)| k == "X-Api-Key"));
    }

    #[test]
    fn bearer_prefix_and_inherit_without_a_parent() {
        let r = Request {
            url: "http://x/".into(),
            auth: AuthKind::Bearer,
            auth_token: "t".into(),
            auth_prefix: "Token".into(),
            ..Default::default()
        };
        let b = build(&r, &[]).unwrap();
        assert!(b.headers.contains(&("Authorization".into(), "Token t".into())));
        let inherit = Request {
            auth: AuthKind::Inherit,
            ..r
        };
        let b = build(&inherit, &[]).unwrap();
        assert!(!b.headers.iter().any(|(k, _)| k == "Authorization"));
    }

    #[test]
    fn graphql_is_sent_as_json_and_validates_variables() {
        let mut r = Request {
            method: "POST".into(),
            url: "http://x/graphql".into(),
            body_kind: BodyKind::Graphql,
            body: "query($id: ID!) { user(id: $id) { name } }".into(),
            graphql_variables: r#"{"id": "{{id}}"}"#.into(),
            ..Default::default()
        };
        let b = build(&r, &[kv("id", "7")]).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&b.body).unwrap();
        assert_eq!(v["variables"]["id"], "7");
        assert!(v["query"].as_str().unwrap().contains("user(id: $id)"));
        assert!(b
            .headers
            .contains(&("Content-Type".into(), "application/json".into())));
        r.graphql_variables = "{nope".into();
        assert!(build(&r, &[]).unwrap_err().contains("not valid JSON"));
    }

    #[test]
    fn form_data_builds_a_multipart_body_with_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "hello").unwrap();
        let r = Request {
            method: "POST".into(),
            url: "http://x/up".into(),
            headers: vec![kv("content-type", "text/plain")],
            body_kind: BodyKind::FormData,
            form: vec![
                kv("title", "Hi \"there\""),
                KeyValue {
                    file: true,
                    ..kv("doc", file.to_str().unwrap())
                },
            ],
            ..Default::default()
        };
        let b = build(&r, &[]).unwrap();
        let ct = &b
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .unwrap()
            .1;
        let boundary = ct.strip_prefix("multipart/form-data; boundary=").unwrap();
        let body = String::from_utf8(b.body.clone()).unwrap();
        assert!(body.starts_with(&format!("--{boundary}\r\n")));
        assert!(body.contains("name=\"title\"\r\n\r\nHi \"there\"\r\n"));
        assert!(body.contains("name=\"doc\"; filename=\"a.txt\"\r\nContent-Type: text/plain\r\n\r\nhello\r\n"));
        assert!(body.ends_with(&format!("--{boundary}--\r\n")));
        assert_eq!(b.form.len(), 2);
        assert_eq!(b.headers.iter().filter(|(k, _)| k.eq_ignore_ascii_case("content-type")).count(), 1);

        let missing = Request {
            form: vec![KeyValue {
                file: true,
                ..kv("doc", "/nope/missing.bin")
            }],
            ..r
        };
        assert!(build(&missing, &[]).unwrap_err().contains("Could not read"));
    }

    #[test]
    fn binary_body_reads_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("b.bin");
        std::fs::write(&file, [0u8, 1, 2]).unwrap();
        let r = Request {
            url: "http://x/".into(),
            body_kind: BodyKind::Binary,
            binary_path: file.to_str().unwrap().into(),
            ..Default::default()
        };
        assert_eq!(build(&r, &[]).unwrap().body, vec![0, 1, 2]);
    }
}
