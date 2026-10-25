//! Postman collection (v2.0/2.1) and environment files.
//!
//! Import reads what the composer can represent (requests, folders, bodies,
//! auth, variables, simple status tests) and ignores the rest (scripts,
//! examples, events other than a plain status check). Export writes a v2.1
//! collection or an environment that Postman opens.

use crate::{
    apply_url_text, default_name, display_url, AssertKind, AssertOp, Assertion, Auth, AuthKind,
    BodyKind, CollectionConfig, Environment, KeyValue, Request, State,
};
use serde_json::{json, Map, Value};

const NOT_POSTMAN: &str = "This file is not a Postman collection or environment.";
const SCHEMA: &str = "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostmanKind {
    Collection,
    Environment,
}

/// What a file held. Each collection is `(path, requests, folder configs)`; a
/// request's `collection` starts with the collection path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Imported {
    pub collections: Vec<(String, Vec<Request>, Vec<CollectionConfig>)>,
    pub environments: Vec<Environment>,
}

fn detect(v: &Value) -> Option<PostmanKind> {
    if v.get("info").is_some_and(Value::is_object) && v.get("item").is_some_and(Value::is_array) {
        Some(PostmanKind::Collection)
    } else if v.get("values").is_some_and(Value::is_array) {
        Some(PostmanKind::Environment)
    } else {
        None
    }
}

// ---------------------------------------------------------------- import

/// Any scalar as text; null and missing are empty.
fn text(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(other) => other.to_string(),
    }
}

/// A description is a string or `{content, type}`.
fn description(v: Option<&Value>) -> String {
    match v {
        Some(Value::Object(o)) => text(o.get("content")),
        other => text(other),
    }
}

fn flag(o: &Map<String, Value>, key: &str) -> bool {
    o.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Header, query and urlencoded rows: `{key, value, disabled, description}`.
fn row(v: &Value) -> Option<KeyValue> {
    let o = v.as_object()?;
    let name = text(o.get("key"));
    let value = text(o.get("value"));
    if name.is_empty() && value.is_empty() {
        return None;
    }
    Some(KeyValue {
        name,
        value,
        disabled: flag(o, "disabled"),
        description: description(o.get("description")),
        secret: false,
        file: false,
    })
}

fn rows(v: Option<&Value>) -> Vec<KeyValue> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(row).collect())
        .unwrap_or_default()
}

fn form_rows(v: Option<&Value>) -> Vec<KeyValue> {
    let Some(arr) = v.and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let o = item.as_object()?;
            let is_file = o.get("type").and_then(Value::as_str) == Some("file");
            if is_file {
                // `src` is a path, or a list of paths; the first one is kept.
                let src = match o.get("src") {
                    Some(Value::Array(a)) => text(a.first()),
                    other => text(other),
                };
                let name = text(o.get("key"));
                if name.is_empty() && src.is_empty() {
                    return None;
                }
                Some(KeyValue {
                    name,
                    value: src,
                    disabled: flag(o, "disabled"),
                    description: description(o.get("description")),
                    secret: false,
                    file: true,
                })
            } else {
                row(item)
            }
        })
        .collect()
}

/// Collection, folder and environment variables.
fn variables(v: Option<&Value>) -> Vec<KeyValue> {
    let Some(arr) = v.and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let o = item.as_object()?;
            let name = if o.contains_key("key") {
                text(o.get("key"))
            } else {
                text(o.get("id"))
            };
            if name.is_empty() {
                return None;
            }
            Some(KeyValue {
                name,
                value: text(o.get("value")),
                // Environments say `enabled: false`, collections `disabled: true`.
                disabled: flag(o, "disabled")
                    || o.get("enabled").and_then(Value::as_bool) == Some(false),
                description: description(o.get("description")),
                secret: o.get("type").and_then(Value::as_str) == Some("secret"),
                file: false,
            })
        })
        .collect()
}

/// One parameter of an auth block. v2.1 keeps `[{key, value}]`, v2.0 `{key: value}`.
fn auth_param(o: &Map<String, Value>, kind: &str, key: &str) -> String {
    match o.get(kind) {
        Some(Value::Array(a)) => a
            .iter()
            .find(|p| p.get("key").and_then(Value::as_str) == Some(key))
            .map(|p| text(p.get("value")))
            .unwrap_or_default(),
        Some(Value::Object(m)) => text(m.get(key)),
        _ => String::new(),
    }
}

/// Absent (or unsupported) auth inherits; `noauth` turns auth off.
fn parse_auth(v: Option<&Value>) -> Auth {
    let inherit = Auth {
        kind: AuthKind::Inherit,
        ..Default::default()
    };
    let Some(o) = v.and_then(Value::as_object) else {
        return inherit;
    };
    match o.get("type").and_then(Value::as_str).unwrap_or("") {
        "noauth" => Auth::default(),
        "bearer" => Auth {
            kind: AuthKind::Bearer,
            token: auth_param(o, "bearer", "token"),
            ..Default::default()
        },
        "basic" => Auth {
            kind: AuthKind::Basic,
            user: auth_param(o, "basic", "username"),
            password: auth_param(o, "basic", "password"),
            ..Default::default()
        },
        "apikey" => Auth {
            kind: AuthKind::ApiKey,
            key: auth_param(o, "apikey", "key"),
            value: auth_param(o, "apikey", "value"),
            in_query: auth_param(o, "apikey", "in") == "query",
            ..Default::default()
        },
        "oauth2" => {
            let token = auth_param(o, "oauth2", "accessToken");
            if token.is_empty() {
                inherit
            } else {
                Auth {
                    kind: AuthKind::Bearer,
                    token,
                    ..Default::default()
                }
            }
        }
        _ => inherit,
    }
}

/// Status codes checked by `pm.response.to.have.status(N)` and
/// `pm.expect(pm.response.code).to.eql(N)` (also `equal`), in script order.
fn status_codes(script: &str) -> Vec<u16> {
    const PREFIXES: [&str; 4] = [
        "pm.response.to.have.status(",
        "pm.expect(pm.response.code).to.eql(",
        "pm.expect(pm.response.code).to.equal(",
        "pm.expect(pm.response.code).to.be.eql(",
    ];
    let compact: String = script.chars().filter(|c| !c.is_whitespace()).collect();
    let mut found: Vec<(usize, u16)> = Vec::new();
    for prefix in PREFIXES {
        let mut from = 0;
        while let Some(at) = compact[from..].find(prefix) {
            let start = from + at + prefix.len();
            let digits: String = compact[start..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if !digits.is_empty() && compact[start + digits.len()..].starts_with(')') {
                if let Ok(code) = digits.parse::<u16>() {
                    found.push((from + at, code));
                }
            }
            from = start;
        }
    }
    found.sort();
    let mut out: Vec<u16> = Vec::new();
    for (_, code) in found {
        if !out.contains(&code) {
            out.push(code);
        }
    }
    out
}

fn tests_from(events: Option<&Value>) -> Vec<Assertion> {
    let Some(arr) = events.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for event in arr {
        if event.get("listen").and_then(Value::as_str) != Some("test") {
            continue;
        }
        let script = match event.pointer("/script/exec") {
            Some(Value::Array(lines)) => lines
                .iter()
                .map(|l| text(Some(l)))
                .collect::<Vec<_>>()
                .join("\n"),
            other => text(other),
        };
        for code in status_codes(&script) {
            let a = Assertion {
                kind: AssertKind::Status,
                target: String::new(),
                op: AssertOp::Equals,
                expected: code.to_string(),
                disabled: false,
            };
            if !out.contains(&a) {
                out.push(a);
            }
        }
    }
    out
}

/// A path segment or a title: `/` would read as a folder separator.
fn clean_name(name: &str) -> String {
    name.replace('/', "-").trim().to_string()
}

/// Host or path parts: strings, or `{type, value}` objects.
fn join_parts(v: Option<&Value>, sep: &str) -> String {
    match v {
        Some(Value::Array(a)) => a
            .iter()
            .map(|p| match p {
                Value::Object(o) => text(o.get("value")),
                other => text(Some(other)),
            })
            .collect::<Vec<_>>()
            .join(sep),
        other => text(other),
    }
}

/// The URL for a `url` object that has no `raw`.
fn build_raw(o: &Map<String, Value>) -> String {
    let protocol = text(o.get("protocol"));
    let host = join_parts(o.get("host"), ".");
    let port = text(o.get("port"));
    let path = join_parts(o.get("path"), "/");
    let mut s = String::new();
    if !protocol.is_empty() {
        s.push_str(&protocol);
        s.push_str("://");
    }
    s.push_str(&host);
    if !port.is_empty() {
        s.push(':');
        s.push_str(&port);
    }
    if !path.is_empty() {
        s.push('/');
        s.push_str(&path);
    }
    s
}

fn apply_url(r: &mut Request, url: Option<&Value>) {
    match url {
        Some(Value::String(s)) => apply_url_text(r, s),
        Some(Value::Object(o)) => {
            let raw = match o.get("raw").and_then(Value::as_str) {
                Some(s) if !s.trim().is_empty() => s.to_string(),
                _ => build_raw(o),
            };
            apply_url_text(r, &raw);
            // The structured query keeps `disabled` rows, which the raw URL can't.
            if o.get("query")
                .and_then(Value::as_array)
                .is_some_and(|q| !q.is_empty())
            {
                r.params = rows(o.get("query"));
            }
            for var in rows(o.get("variable")) {
                if let Some(slot) = r.path_vars.iter_mut().find(|p| p.name == var.name) {
                    slot.value = var.value;
                    slot.description = var.description;
                }
            }
        }
        _ => {}
    }
}

fn apply_body(r: &mut Request, body: Option<&Value>) {
    let Some(bv) = body else {
        return;
    };
    let Some(b) = bv.as_object() else {
        return;
    };
    if flag(b, "disabled") {
        return;
    }
    match b.get("mode").and_then(Value::as_str).unwrap_or("") {
        "raw" => {
            r.body = text(b.get("raw"));
            let lang = bv
                .pointer("/options/raw/language")
                .and_then(Value::as_str)
                .unwrap_or("");
            r.body_kind = match lang {
                "json" => BodyKind::Json,
                "xml" => BodyKind::Xml,
                "html" => BodyKind::Html,
                "javascript" => BodyKind::Javascript,
                _ => BodyKind::Text,
            };
        }
        "urlencoded" => {
            r.form = rows(b.get("urlencoded"));
            r.body_kind = BodyKind::Form;
        }
        "formdata" => {
            r.form = form_rows(b.get("formdata"));
            r.body_kind = BodyKind::FormData;
        }
        "file" => {
            r.binary_path = match bv.pointer("/file/src") {
                Some(Value::Array(a)) => text(a.first()),
                other => text(other),
            };
            r.body_kind = BodyKind::Binary;
        }
        "graphql" => {
            r.body = text(bv.pointer("/graphql/query"));
            r.graphql_variables = match bv.pointer("/graphql/variables") {
                Some(Value::Object(_)) | Some(Value::Array(_)) => {
                    serde_json::to_string_pretty(bv.pointer("/graphql/variables").unwrap())
                        .unwrap_or_default()
                }
                other => text(other),
            };
            r.body_kind = BodyKind::Graphql;
        }
        _ => {}
    }
}

fn import_request(item: &Map<String, Value>, req: &Value, collection: &str) -> Request {
    let mut r = Request {
        collection: collection.to_string(),
        name: text(item.get("name")).trim().to_string(),
        method: "GET".into(),
        ..Default::default()
    };
    r.set_auth(parse_auth(None));
    r.description = description(item.get("description"));
    match req {
        Value::String(url) => apply_url_text(&mut r, url),
        Value::Object(o) => {
            let method = text(o.get("method")).trim().to_ascii_uppercase();
            if !method.is_empty() {
                r.method = method;
            }
            apply_url(&mut r, o.get("url"));
            r.headers = rows(o.get("header"));
            apply_body(&mut r, o.get("body"));
            if o.contains_key("auth") {
                r.set_auth(parse_auth(o.get("auth")));
            }
            let d = description(o.get("description"));
            if !d.is_empty() {
                r.description = d;
            }
        }
        _ => {}
    }
    r.tests = tests_from(item.get("event"));
    if r.name.is_empty() {
        r.name = default_name(&r);
    }
    r
}

/// The settings a collection or folder passes down; `None` when it sets none.
fn config_of(path: &str, description: String, o: &Map<String, Value>) -> Option<CollectionConfig> {
    let auth = parse_auth(o.get("auth"));
    let vars = variables(o.get("variable"));
    if description.is_empty() && auth.kind == AuthKind::Inherit && vars.is_empty() {
        return None;
    }
    Some(CollectionConfig {
        name: path.to_string(),
        description,
        auth,
        headers: Vec::new(),
        vars,
    })
}

#[derive(Default)]
struct Walk {
    requests: Vec<Request>,
    configs: Vec<CollectionConfig>,
}

fn walk(items: &[Value], path: &str, w: &mut Walk) {
    for item in items {
        let Some(o) = item.as_object() else {
            continue;
        };
        if let Some(children) = o.get("item").and_then(Value::as_array) {
            let name = clean_name(&text(o.get("name")));
            let name = if name.is_empty() { "Folder".to_string() } else { name };
            let folder = format!("{path}/{name}");
            if let Some(c) = config_of(&folder, description(o.get("description")), o) {
                w.configs.push(c);
            }
            walk(children, &folder, w);
        } else if let Some(req) = o.get("request") {
            w.requests.push(import_request(o, req, path));
        }
    }
}

fn import_collection(v: &Value) -> Imported {
    let info = v.get("info").and_then(Value::as_object);
    let title = clean_name(&text(info.and_then(|i| i.get("name"))));
    let title = if title.is_empty() {
        "Imported collection".to_string()
    } else {
        title
    };
    let mut w = Walk::default();
    if let Some(root) = v.as_object() {
        let desc = description(info.and_then(|i| i.get("description")));
        if let Some(c) = config_of(&title, desc, root) {
            w.configs.push(c);
        }
    }
    if let Some(items) = v.get("item").and_then(Value::as_array) {
        walk(items, &title, &mut w);
    }
    Imported {
        collections: vec![(title, w.requests, w.configs)],
        environments: Vec::new(),
    }
}

fn import_environment(v: &Value) -> Imported {
    let globals = v.get("_postman_variable_scope").and_then(Value::as_str) == Some("globals");
    let mut name = text(v.get("name")).trim().to_string();
    if name.is_empty() {
        name = if globals { "Globals" } else { "Postman environment" }.to_string();
    }
    Imported {
        collections: Vec::new(),
        environments: vec![Environment {
            name,
            vars: variables(v.get("values")),
        }],
    }
}

pub fn import_postman(json: &str) -> Result<Imported, String> {
    let v: Value = serde_json::from_str(json).map_err(|_| NOT_POSTMAN.to_string())?;
    match detect(&v) {
        Some(PostmanKind::Collection) => Ok(import_collection(&v)),
        Some(PostmanKind::Environment) => Ok(import_environment(&v)),
        None => Err(NOT_POSTMAN.into()),
    }
}

// ---------------------------------------------------------------- export

/// A stable UUID-shaped id from a name.
fn uuid_for(seed: &str) -> String {
    let hash = blake3::hash(seed.as_bytes());
    let mut b = [0u8; 16];
    b.copy_from_slice(&hash.as_bytes()[..16]);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// Split on `sep`, except inside `{{ }}`.
fn split_outside_braces(s: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for c in s.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth = (depth - 1).max(0),
            _ => {}
        }
        if c == sep && depth == 0 {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

fn kv_param(key: &str, value: &str) -> Value {
    json!({"key": key, "value": value, "type": "string"})
}

fn auth_json(a: &Auth) -> Option<Value> {
    match a.kind {
        AuthKind::Inherit => None,
        AuthKind::None => Some(json!({"type": "noauth"})),
        AuthKind::Bearer => Some(json!({
            "type": "bearer",
            "bearer": [kv_param("token", &a.token)],
        })),
        AuthKind::Basic => Some(json!({
            "type": "basic",
            "basic": [kv_param("username", &a.user), kv_param("password", &a.password)],
        })),
        AuthKind::ApiKey => Some(json!({
            "type": "apikey",
            "apikey": [
                kv_param("key", &a.key),
                kv_param("value", &a.value),
                kv_param("in", if a.in_query { "query" } else { "header" }),
            ],
        })),
    }
}

fn row_json(r: &KeyValue, typed: bool) -> Value {
    let mut o = Map::new();
    o.insert("key".into(), json!(r.name));
    if typed && r.file {
        o.insert("type".into(), json!("file"));
        o.insert("src".into(), json!(r.value));
    } else {
        o.insert("value".into(), json!(r.value));
        if typed {
            o.insert("type".into(), json!("text"));
        }
    }
    if r.disabled {
        o.insert("disabled".into(), json!(true));
    }
    if !r.description.is_empty() {
        o.insert("description".into(), json!(r.description));
    }
    Value::Object(o)
}

fn variables_json(vars: &[KeyValue]) -> Vec<Value> {
    vars.iter()
        .filter(|v| !v.name.is_empty())
        .map(|v| {
            let mut o = Map::new();
            o.insert("key".into(), json!(v.name));
            o.insert("value".into(), json!(v.value));
            o.insert("type".into(), json!(if v.secret { "secret" } else { "string" }));
            if v.disabled {
                o.insert("disabled".into(), json!(true));
            }
            if !v.description.is_empty() {
                o.insert("description".into(), json!(v.description));
            }
            Value::Object(o)
        })
        .collect()
}

fn url_json(r: &Request) -> Value {
    let base = r.url.trim();
    let (protocol, rest) = match base.split_once("://") {
        Some((p, rest)) => (Some(p), rest),
        None => (None, base),
    };
    let (host_port, path) = match rest.split_once('/') {
        Some((h, p)) => (h, p),
        None => (rest, ""),
    };
    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() && !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => {
            (h, Some(p))
        }
        _ => (host_port, None),
    };
    let mut o = Map::new();
    o.insert("raw".into(), json!(display_url(r)));
    if let Some(p) = protocol {
        o.insert("protocol".into(), json!(p));
    }
    o.insert("host".into(), json!(split_outside_braces(host, '.')));
    if let Some(p) = port {
        o.insert("port".into(), json!(p));
    }
    let segments: Vec<String> = split_outside_braces(path, '/')
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    if !segments.is_empty() {
        o.insert("path".into(), json!(segments));
    }
    let query: Vec<Value> = r
        .params
        .iter()
        .filter(|p| !p.name.is_empty())
        .map(|p| row_json(p, false))
        .collect();
    if !query.is_empty() {
        o.insert("query".into(), Value::Array(query));
    }
    let vars: Vec<Value> = r.path_vars.iter().map(|p| row_json(p, false)).collect();
    if !vars.is_empty() {
        o.insert("variable".into(), Value::Array(vars));
    }
    Value::Object(o)
}

fn body_json(r: &Request) -> Option<Value> {
    let raw = |language: &str| {
        json!({
            "mode": "raw",
            "raw": r.body,
            "options": {"raw": {"language": language}},
        })
    };
    match r.body_kind {
        BodyKind::None => None,
        BodyKind::Json => Some(raw("json")),
        BodyKind::Text => Some(raw("text")),
        BodyKind::Xml => Some(raw("xml")),
        BodyKind::Html => Some(raw("html")),
        BodyKind::Javascript => Some(raw("javascript")),
        BodyKind::Form => Some(json!({
            "mode": "urlencoded",
            "urlencoded": r.form.iter().map(|f| row_json(f, false)).collect::<Vec<_>>(),
        })),
        BodyKind::FormData => Some(json!({
            "mode": "formdata",
            "formdata": r.form.iter().map(|f| row_json(f, true)).collect::<Vec<_>>(),
        })),
        BodyKind::Binary => Some(json!({
            "mode": "file",
            "file": {"src": r.binary_path},
        })),
        BodyKind::Graphql => Some(json!({
            "mode": "graphql",
            "graphql": {"query": r.body, "variables": r.graphql_variables},
        })),
    }
}

/// Only plain "status equals N" checks can be written as a Postman test.
fn test_event(r: &Request) -> Option<Value> {
    let mut lines: Vec<String> = Vec::new();
    for t in &r.tests {
        if t.disabled || t.kind != AssertKind::Status || t.op != AssertOp::Equals {
            continue;
        }
        let Ok(code) = t.expected.trim().parse::<u16>() else {
            continue;
        };
        lines.push(format!("pm.test(\"Status code is {code}\", function () {{"));
        lines.push(format!("    pm.response.to.have.status({code});"));
        lines.push("});".into());
    }
    if lines.is_empty() {
        return None;
    }
    Some(json!({
        "listen": "test",
        "script": {"type": "text/javascript", "exec": lines},
    }))
}

fn request_json(r: &Request) -> Value {
    let mut req = Map::new();
    req.insert(
        "method".into(),
        json!(if r.method.trim().is_empty() { "GET" } else { r.method.trim() }),
    );
    req.insert(
        "header".into(),
        Value::Array(
            r.headers
                .iter()
                .filter(|h| !h.name.is_empty())
                .map(|h| row_json(h, false))
                .collect(),
        ),
    );
    req.insert("url".into(), url_json(r));
    if let Some(b) = body_json(r) {
        req.insert("body".into(), b);
    }
    if let Some(a) = auth_json(&r.auth()) {
        req.insert("auth".into(), a);
    }
    if !r.description.is_empty() {
        req.insert("description".into(), json!(r.description));
    }
    let mut item = Map::new();
    item.insert("name".into(), json!(r.name));
    if let Some(e) = test_event(r) {
        item.insert("event".into(), json!([e]));
    }
    item.insert("request".into(), Value::Object(req));
    item.insert("response".into(), json!([]));
    Value::Object(item)
}

/// Auth and variables of a collection or folder, written into its object.
fn put_config(o: &mut Map<String, Value>, cfg: Option<&CollectionConfig>) {
    let Some(cfg) = cfg else {
        return;
    };
    if let Some(a) = auth_json(&cfg.auth) {
        o.insert("auth".into(), a);
    }
    let vars = variables_json(&cfg.vars);
    if !vars.is_empty() {
        o.insert("variable".into(), Value::Array(vars));
    }
}

/// The folders (first) and requests directly inside `path`.
fn items_for(state: &State, path: &str) -> Vec<Value> {
    let prefix = format!("{path}/");
    let mut children: Vec<&str> = Vec::new();
    let paths = state
        .collections
        .iter()
        .map(String::as_str)
        .chain(state.requests.iter().map(|r| r.collection.as_str()))
        .chain(state.configs.iter().map(|c| c.name.as_str()));
    for p in paths {
        if let Some(rest) = p.strip_prefix(&prefix) {
            let first = rest.split('/').next().unwrap_or("");
            if !first.is_empty() && !children.contains(&first) {
                children.push(first);
            }
        }
    }
    let mut items = Vec::new();
    for child in children {
        let folder = format!("{prefix}{child}");
        let cfg = state.configs.iter().find(|c| c.name == folder);
        let mut o = Map::new();
        o.insert("name".into(), json!(child));
        if let Some(c) = cfg.filter(|c| !c.description.is_empty()) {
            o.insert("description".into(), json!(c.description));
        }
        put_config(&mut o, cfg);
        o.insert("item".into(), Value::Array(items_for(state, &folder)));
        items.push(Value::Object(o));
    }
    items.extend(
        state
            .requests
            .iter()
            .filter(|r| r.collection == path)
            .map(request_json),
    );
    items
}

pub fn export_collection(state: &State, name: &str) -> Result<String, String> {
    let name = name.trim().trim_matches('/');
    let inside = |c: &str| c == name || c.starts_with(&format!("{name}/"));
    let exists = !name.is_empty()
        && (state.collections.iter().any(|c| inside(c))
            || state.requests.iter().any(|r| inside(&r.collection))
            || state.configs.iter().any(|c| inside(&c.name)));
    if !exists {
        return Err(format!("There is no collection called \"{name}\"."));
    }
    let title = name.rsplit('/').next().unwrap_or(name);
    let root = state.configs.iter().find(|c| c.name == name);
    let mut info = Map::new();
    info.insert("_postman_id".into(), json!(uuid_for(name)));
    info.insert("name".into(), json!(title));
    if let Some(c) = root.filter(|c| !c.description.is_empty()) {
        info.insert("description".into(), json!(c.description));
    }
    info.insert("schema".into(), json!(SCHEMA));
    let mut out = Map::new();
    out.insert("info".into(), Value::Object(info));
    out.insert("item".into(), Value::Array(items_for(state, name)));
    put_config(&mut out, root);
    serde_json::to_string_pretty(&Value::Object(out)).map_err(|e| e.to_string())
}

pub fn export_environment(state: &State, name: &str) -> Result<String, String> {
    let env = state
        .environments
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| format!("There is no environment called \"{name}\"."))?;
    let values: Vec<Value> = env
        .vars
        .iter()
        .filter(|v| !v.name.is_empty())
        .map(|v| {
            json!({
                "key": v.name,
                "value": v.value,
                "type": if v.secret { "secret" } else { "default" },
                "enabled": !v.disabled,
            })
        })
        .collect();
    let out = json!({
        "id": uuid_for(&format!("environment:{name}")),
        "name": env.name,
        "values": values,
        "_postman_variable_scope": "environment",
        "_postman_exported_using": "Neko",
    });
    serde_json::to_string_pretty(&out).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Composer;

    const SAMPLE: &str = r##"{
      "info": {
        "_postman_id": "11111111-2222-4333-8444-555555555555",
        "name": "Shop API",
        "description": "Demo shop",
        "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
      },
      "auth": {"type": "bearer", "bearer": [{"key": "token", "value": "{{token}}", "type": "string"}]},
      "variable": [
        {"key": "baseUrl", "value": "https://shop.example.com"},
        {"key": "apiSecret", "value": "s3", "type": "secret"}
      ],
      "item": [
        {
          "name": "Orders",
          "description": "All order calls",
          "item": [
            {
              "name": "List orders",
              "request": {
                "method": "get",
                "header": [
                  {"key": "Accept", "value": "application/json"},
                  {"key": "X-Debug", "value": "1", "disabled": true, "description": "debug only"}
                ],
                "url": {
                  "raw": "{{baseUrl}}/orders?page=1&sort=asc",
                  "host": ["{{baseUrl}}"],
                  "path": ["orders"],
                  "query": [
                    {"key": "page", "value": "1", "description": "page number"},
                    {"key": "sort", "value": "asc", "disabled": true}
                  ]
                },
                "description": {"content": "Lists orders", "type": "text/plain"}
              }
            },
            {
              "name": "Get order",
              "request": {
                "method": "GET",
                "header": [],
                "url": {
                  "raw": "{{baseUrl}}/orders/:id",
                  "host": ["{{baseUrl}}"],
                  "path": ["orders", ":id"],
                  "variable": [{"key": "id", "value": "42", "description": "order id"}]
                }
              }
            },
            {
              "name": "Admin",
              "auth": {"type": "noauth"},
              "item": [
                {
                  "name": "Create order",
                  "event": [{
                    "listen": "test",
                    "script": {"type": "text/javascript", "exec": [
                      "pm.test(\"created\", function () {",
                      "    pm.response.to.have.status(201);",
                      "});",
                      "pm.expect(pm.response.code).to.eql(201);",
                      "pm.expect(pm.response.json().id).to.exist;"
                    ]}
                  }],
                  "request": {
                    "auth": {"type": "basic", "basic": [
                      {"key": "username", "value": "admin"}, {"key": "password", "value": "pw"}]},
                    "method": "POST",
                    "header": [{"key": "Content-Type", "value": "application/json"}],
                    "body": {"mode": "raw", "raw": "{\"sku\": \"A1\"}", "options": {"raw": {"language": "json"}}},
                    "url": "{{baseUrl}}/orders"
                  }
                }
              ]
            }
          ]
        },
        {
          "name": "Upload avatar",
          "request": {
            "method": "POST",
            "body": {"mode": "formdata", "formdata": [
              {"key": "name", "value": "me", "type": "text"},
              {"key": "avatar", "type": "file", "src": "/tmp/me.png"}
            ]},
            "url": {"raw": "{{baseUrl}}/avatar", "host": ["{{baseUrl}}"], "path": ["avatar"]},
            "auth": {"type": "apikey", "apikey": [
              {"key": "key", "value": "X-Key"}, {"key": "value", "value": "abc"}, {"key": "in", "value": "query"}]}
          }
        }
      ]
    }"##;

    fn find<'a>(reqs: &'a [Request], name: &str) -> &'a Request {
        reqs.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("no {name}"))
    }

    #[test]
    fn imports_a_realistic_v21_collection() {
        let imp = import_postman(SAMPLE).unwrap();
        assert!(imp.environments.is_empty());
        assert_eq!(imp.collections.len(), 1);
        let (path, reqs, configs) = &imp.collections[0];
        assert_eq!(path, "Shop API");
        assert_eq!(reqs.len(), 4);

        let list = find(reqs, "List orders");
        assert_eq!(list.collection, "Shop API/Orders");
        assert_eq!(list.method, "GET");
        assert_eq!(list.url, "{{baseUrl}}/orders");
        assert_eq!(list.description, "Lists orders");
        assert_eq!(list.auth, AuthKind::Inherit);
        assert!(list.id.is_empty());
        assert_eq!(list.params.len(), 2);
        assert_eq!(list.params[0].name, "page");
        assert_eq!(list.params[0].description, "page number");
        assert!(!list.params[0].disabled);
        assert!(list.params[1].disabled);
        assert_eq!(display_url(list), "{{baseUrl}}/orders?page=1");
        assert_eq!(list.headers.len(), 2);
        assert!(list.headers[1].disabled);
        assert_eq!(list.headers[1].description, "debug only");

        let get = find(reqs, "Get order");
        assert_eq!(get.url, "{{baseUrl}}/orders/:id");
        assert_eq!(get.path_vars.len(), 1);
        assert_eq!(get.path_vars[0].name, "id");
        assert_eq!(get.path_vars[0].value, "42");
        assert_eq!(get.path_vars[0].description, "order id");
        assert_eq!(get.auth, AuthKind::Inherit);

        let create = find(reqs, "Create order");
        assert_eq!(create.collection, "Shop API/Orders/Admin");
        assert_eq!(create.method, "POST");
        assert_eq!(create.url, "{{baseUrl}}/orders");
        assert_eq!(create.body_kind, BodyKind::Json);
        assert_eq!(create.body, "{\"sku\": \"A1\"}");
        assert_eq!(create.auth, AuthKind::Basic);
        assert_eq!(create.auth_user, "admin");
        assert_eq!(create.auth_password, "pw");
        assert_eq!(
            create.tests,
            vec![Assertion {
                kind: AssertKind::Status,
                target: String::new(),
                op: AssertOp::Equals,
                expected: "201".into(),
                disabled: false,
            }]
        );

        let up = find(reqs, "Upload avatar");
        assert_eq!(up.collection, "Shop API");
        assert_eq!(up.body_kind, BodyKind::FormData);
        assert_eq!(up.form.len(), 2);
        assert!(!up.form[0].file);
        assert!(up.form[1].file);
        assert_eq!(up.form[1].name, "avatar");
        assert_eq!(up.form[1].value, "/tmp/me.png");
        assert_eq!(up.auth, AuthKind::ApiKey);
        assert_eq!(up.auth_key, "X-Key");
        assert_eq!(up.auth_value, "abc");
        assert!(up.auth_in_query);

        // Root: bearer + variables + description. Orders: description only
        // (inherits auth). Admin: noauth.
        assert_eq!(configs.len(), 3);
        let root = &configs[0];
        assert_eq!(root.name, "Shop API");
        assert_eq!(root.description, "Demo shop");
        assert_eq!(root.auth.kind, AuthKind::Bearer);
        assert_eq!(root.auth.token, "{{token}}");
        assert_eq!(root.vars.len(), 2);
        assert_eq!(root.vars[0], KeyValue::new("baseUrl", "https://shop.example.com"));
        assert!(root.vars[1].secret);
        let orders = &configs[1];
        assert_eq!(orders.name, "Shop API/Orders");
        assert_eq!(orders.auth.kind, AuthKind::Inherit);
        assert_eq!(orders.description, "All order calls");
        let admin = &configs[2];
        assert_eq!(admin.name, "Shop API/Orders/Admin");
        assert_eq!(admin.auth.kind, AuthKind::None);
    }

    #[test]
    fn inherited_bearer_reaches_requests_via_state() {
        let c = Composer::in_memory();
        c.add_imported(import_postman(SAMPLE).unwrap()).unwrap();
        let s = c.snapshot();
        let get = s.requests.iter().find(|r| r.name == "Get order").unwrap();
        let (prepared, vars) = s.prepare(get);
        assert_eq!(prepared.auth, AuthKind::Bearer);
        assert_eq!(prepared.auth_token, "{{token}}");
        assert!(vars.iter().any(|v| v.name == "baseUrl"));
        let create = s.requests.iter().find(|r| r.name == "Create order").unwrap();
        assert_eq!(s.prepare(create).0.auth, AuthKind::Basic);
    }

    #[test]
    fn imports_v20_style_and_odd_shapes() {
        let json = r#"{
          "info": {"name": "A/B", "schema": "https://schema.getpostman.com/json/collection/v2.0.0/collection.json"},
          "item": [
            {"name": "Sub/folder", "item": [
              {"name": "", "request": {
                "method": "PUT",
                "url": {"protocol": "https", "host": ["api", "example", "com"], "port": "8443",
                        "path": ["v1", "things"], "query": [{"key": "a", "value": null}]},
                "auth": {"type": "bearer", "bearer": {"token": "t0k"}},
                "body": {"mode": "urlencoded", "urlencoded": [{"key": "x", "value": "1"}]}
              }},
              {"name": "bin", "request": {"method": "POST", "url": "http://h/",
                "body": {"mode": "file", "file": {"src": "/tmp/a.bin"}}}},
              {"name": "gql", "request": {"method": "POST", "url": "http://h/g",
                "body": {"mode": "graphql", "graphql": {"query": "{ me }", "variables": "{\"a\":1}"}},
                "auth": {"type": "noauth"}}},
              {"name": "xml", "request": "http://h/plain",
               "x": 1},
              {"name": "x2", "request": {"method": "POST", "url": "http://h/x",
                "body": {"mode": "raw", "raw": "<a/>", "options": {"raw": {"language": "xml"}}}}}
            ]}
          ]
        }"#;
        let imp = import_postman(json).unwrap();
        let (path, reqs, configs) = &imp.collections[0];
        assert_eq!(path, "A-B");
        assert!(configs.is_empty());
        assert_eq!(reqs[0].collection, "A-B/Sub-folder");
        assert_eq!(reqs[0].name, "PUT /v1/things");
        assert_eq!(reqs[0].url, "https://api.example.com:8443/v1/things");
        assert_eq!(reqs[0].params, vec![KeyValue::new("a", "")]);
        assert_eq!(reqs[0].auth, AuthKind::Bearer);
        assert_eq!(reqs[0].auth_token, "t0k");
        assert_eq!(reqs[0].body_kind, BodyKind::Form);
        assert_eq!(reqs[1].body_kind, BodyKind::Binary);
        assert_eq!(reqs[1].binary_path, "/tmp/a.bin");
        assert_eq!(reqs[2].body_kind, BodyKind::Graphql);
        assert_eq!(reqs[2].body, "{ me }");
        assert_eq!(reqs[2].graphql_variables, "{\"a\":1}");
        assert_eq!(reqs[2].auth, AuthKind::None);
        assert_eq!(reqs[3].method, "GET");
        assert_eq!(reqs[3].url, "http://h/plain");
        assert_eq!(reqs[4].body_kind, BodyKind::Xml);
    }

    #[test]
    fn imports_environments_and_globals() {
        let env = r#"{"id":"x","name":"Staging","values":[
            {"key":"host","value":"s.example.com","enabled":true,"type":"default"},
            {"key":"token","value":"abc","enabled":true,"type":"secret"},
            {"key":"old","value":"1","enabled":false}
          ],"_postman_variable_scope":"environment"}"#;
        let imp = import_postman(env).unwrap();
        assert!(imp.collections.is_empty());
        let e = &imp.environments[0];
        assert_eq!(e.name, "Staging");
        assert_eq!(e.vars.len(), 3);
        assert!(!e.vars[0].disabled && !e.vars[0].secret);
        assert!(e.vars[1].secret);
        assert!(e.vars[2].disabled);

        let globals = r#"{"values":[{"key":"a","value":"1","enabled":true}],
            "_postman_variable_scope":"globals"}"#;
        let imp = import_postman(globals).unwrap();
        assert_eq!(imp.environments[0].name, "Globals");
        let named = r#"{"name":"Workspace Globals","values":[],"_postman_variable_scope":"globals"}"#;
        assert_eq!(
            import_postman(named).unwrap().environments[0].name,
            "Workspace Globals"
        );
    }

    #[test]
    fn rejects_other_files() {
        for bad in ["{}", "[]", "not json", r#"{"info":{"name":"x"}}"#, r#"{"values":3}"#] {
            assert_eq!(
                import_postman(bad).unwrap_err(),
                "This file is not a Postman collection or environment."
            );
        }
    }

    #[test]
    fn status_test_patterns() {
        assert_eq!(status_codes("pm.response.to.have.status( 200 );"), vec![200]);
        assert_eq!(
            status_codes("pm.expect(pm.response.code).to.equal(404);pm.response.to.have.status(200)"),
            vec![404, 200]
        );
        assert!(status_codes("pm.response.to.have.status(\"OK\")").is_empty());
        assert!(status_codes("pm.expect(1).to.eql(2)").is_empty());
    }

    fn crafted() -> State {
        let c = Composer::in_memory();
        let mut a = Request {
            name: "Api key".into(),
            collection: "Mix".into(),
            method: "GET".into(),
            description: "with everything".into(),
            auth: AuthKind::ApiKey,
            auth_key: "k".into(),
            auth_value: "v".into(),
            auth_in_query: true,
            ..Default::default()
        };
        apply_url_text(&mut a, "http://localhost:3000/a/:id/b?x=1&y=a%20b");
        a.params.push(KeyValue {
            disabled: true,
            description: "off".into(),
            ..KeyValue::new("off", "2")
        });
        a.path_vars[0].value = "7".into();
        a.headers = vec![
            KeyValue::new("X-A", "1"),
            KeyValue {
                disabled: true,
                description: "note".into(),
                ..KeyValue::new("X-B", "2")
            },
        ];
        a.tests = vec![Assertion {
            expected: "201".into(),
            ..Default::default()
        }];
        c.save_request(a).unwrap();

        let mut b = Request {
            name: "GraphQL".into(),
            collection: "Mix/Sub".into(),
            method: "POST".into(),
            url: "{{base}}/graphql".into(),
            body_kind: BodyKind::Graphql,
            body: "{ me { id } }".into(),
            graphql_variables: "{\"a\": 1}".into(),
            auth: AuthKind::None,
            ..Default::default()
        };
        b.headers = vec![KeyValue::new("X", "y")];
        c.save_request(b).unwrap();

        c.save_request(Request {
            name: "Upload".into(),
            collection: "Mix/Sub".into(),
            method: "POST".into(),
            url: "https://h.dev/up".into(),
            body_kind: BodyKind::FormData,
            form: vec![
                KeyValue::new("a", "1"),
                KeyValue {
                    file: true,
                    ..KeyValue::new("f", "/tmp/x.png")
                },
            ],
            auth: AuthKind::Basic,
            auth_user: "u".into(),
            auth_password: "p".into(),
            ..Default::default()
        })
        .unwrap();
        c.save_request(Request {
            name: "Form".into(),
            collection: "Mix/Sub/Deep".into(),
            method: "PATCH".into(),
            url: "https://h.dev/f".into(),
            body_kind: BodyKind::Form,
            form: vec![KeyValue::new("k", "v w"), KeyValue::new("e", "")],
            auth: AuthKind::Bearer,
            auth_token: "tok".into(),
            ..Default::default()
        })
        .unwrap();
        for (name, kind, body, ext) in [
            ("Xml", BodyKind::Xml, "<a/>", ""),
            ("Html", BodyKind::Html, "<p>", ""),
            ("Js", BodyKind::Javascript, "1+1", ""),
            ("Text", BodyKind::Text, "hi", ""),
            ("Json", BodyKind::Json, "{}", ""),
        ] {
            c.save_request(Request {
                name: name.into(),
                collection: "Mix".into(),
                method: "POST".into(),
                url: format!("http://h/{name}{ext}"),
                body_kind: kind,
                body: body.into(),
                auth: AuthKind::Inherit,
                ..Default::default()
            })
            .unwrap();
        }
        c.save_request(Request {
            name: "Bin".into(),
            collection: "Mix".into(),
            method: "PUT".into(),
            url: "http://h/bin".into(),
            body_kind: BodyKind::Binary,
            binary_path: "/tmp/data.bin".into(),
            auth: AuthKind::Inherit,
            ..Default::default()
        })
        .unwrap();
        c.set_config(CollectionConfig {
            name: "Mix".into(),
            description: "Top".into(),
            auth: Auth {
                kind: AuthKind::Bearer,
                token: "{{t}}".into(),
                ..Default::default()
            },
            vars: vec![
                KeyValue::new("base", "http://x"),
                KeyValue {
                    secret: true,
                    ..KeyValue::new("pw", "s")
                },
                KeyValue {
                    disabled: true,
                    ..KeyValue::new("off", "1")
                },
            ],
            ..Default::default()
        })
        .unwrap();
        c.set_config(CollectionConfig {
            name: "Mix/Sub".into(),
            description: "Folder note".into(),
            auth: Auth {
                kind: AuthKind::Inherit,
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
        c.set_config(CollectionConfig {
            name: "Mix/Sub/Deep".into(),
            auth: Auth {
                kind: AuthKind::None,
                ..Default::default()
            },
            vars: vec![KeyValue::new("d", "1")],
            ..Default::default()
        })
        .unwrap();
        c.snapshot()
    }

    fn normalized(mut reqs: Vec<Request>) -> Vec<Request> {
        for r in reqs.iter_mut() {
            r.id.clear();
            r.updated_ms = 0;
        }
        reqs.sort_by(|a, b| (&a.collection, &a.name).cmp(&(&b.collection, &b.name)));
        reqs
    }

    fn round_trip(state: &State, name: &str) -> (Vec<Request>, Vec<CollectionConfig>) {
        let json = export_collection(state, name).unwrap();
        let imp = import_postman(&json).unwrap_or_else(|e| panic!("{e}\n{json}"));
        assert_eq!(imp.collections.len(), 1);
        let (path, reqs, mut configs) = imp.collections.into_iter().next().unwrap();
        assert_eq!(path, name);
        configs.sort_by(|a, b| a.name.cmp(&b.name));
        (normalized(reqs), configs)
    }

    #[test]
    fn export_then_import_keeps_requests_and_configs() {
        let state = crafted();
        let (reqs, configs) = round_trip(&state, "Mix");
        let want = normalized(state.requests.clone());
        assert_eq!(reqs.len(), want.len());
        for (got, want) in reqs.iter().zip(&want) {
            assert_eq!(got, want, "request {}", want.name);
        }
        let mut want_cfg = state.configs.clone();
        // A folder that only inherits and has no other settings still has a note.
        want_cfg.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(configs, want_cfg);
    }

    #[test]
    fn imported_sample_survives_a_round_trip() {
        let c = Composer::in_memory();
        c.add_imported(import_postman(SAMPLE).unwrap()).unwrap();
        let state = c.snapshot();
        let (reqs, configs) = round_trip(&state, "Shop API");
        assert_eq!(reqs, normalized(state.requests.clone()));
        let mut want = state.configs.clone();
        want.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(configs, want);
    }

    #[test]
    fn export_shape_is_postman_v21() {
        let state = crafted();
        let json = export_collection(&state, "Mix").unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["info"]["schema"], SCHEMA);
        assert_eq!(v["info"]["name"], "Mix");
        assert_eq!(v["info"]["description"], "Top");
        assert_eq!(v["info"]["_postman_id"], uuid_for("Mix"));
        assert_eq!(v["auth"]["type"], "bearer");
        assert_eq!(v["auth"]["bearer"][0]["key"], "token");
        assert_eq!(v["variable"][1]["type"], "secret");
        // Folders come first, requests after.
        assert_eq!(v["item"][0]["name"], "Sub");
        assert_eq!(v["item"][0]["item"][0]["name"], "Deep");
        assert_eq!(v["item"][0]["item"][0]["auth"]["type"], "noauth");
        let api = v["item"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["name"] == "Api key")
            .unwrap();
        let url = &api["request"]["url"];
        assert_eq!(url["raw"], "http://localhost:3000/a/:id/b?x=1&y=a%20b");
        assert_eq!(url["protocol"], "http");
        assert_eq!(url["host"], json!(["localhost"]));
        assert_eq!(url["port"], "3000");
        assert_eq!(url["path"], json!(["a", ":id", "b"]));
        assert_eq!(url["query"][2]["disabled"], true);
        assert_eq!(url["variable"][0]["value"], "7");
        assert_eq!(api["request"]["auth"]["apikey"][2]["value"], "query");
        assert_eq!(api["event"][0]["listen"], "test");
        // Same name, same id.
        assert_eq!(json, export_collection(&state, "Mix").unwrap());
    }

    #[test]
    fn exports_a_folder_as_its_own_collection() {
        let state = crafted();
        let json = export_collection(&state, "Mix/Sub").unwrap();
        let imp = import_postman(&json).unwrap();
        let (path, reqs, configs) = &imp.collections[0];
        assert_eq!(path, "Sub");
        let reqs = normalized(reqs.clone());
        let got: Vec<_> = reqs.iter().map(|r| (r.collection.as_str(), r.name.as_str())).collect();
        assert_eq!(
            got,
            vec![("Sub", "GraphQL"), ("Sub", "Upload"), ("Sub/Deep", "Form")]
        );
        assert_eq!(configs[0].name, "Sub");
        assert_eq!(configs[0].description, "Folder note");
        assert!(export_collection(&state, "Nope").is_err());
        assert!(export_collection(&state, "").is_err());
    }

    #[test]
    fn variables_in_hosts_do_not_split_on_dots() {
        let mut r = Request {
            name: "v".into(),
            method: "GET".into(),
            url: "{{env.base}}/x/{{a/b}}".into(),
            ..Default::default()
        };
        r.collection = "C".into();
        let u = url_json(&r);
        assert_eq!(u["host"], json!(["{{env.base}}"]));
        assert_eq!(u["path"], json!(["x", "{{a/b}}"]));
    }

    #[test]
    fn exports_and_imports_environments() {
        let state = State {
            environments: vec![Environment {
                name: "Staging".into(),
                vars: vec![
                    KeyValue::new("host", "s.example.com"),
                    KeyValue {
                        secret: true,
                        ..KeyValue::new("token", "abc")
                    },
                    KeyValue {
                        disabled: true,
                        ..KeyValue::new("old", "1")
                    },
                ],
            }],
            ..Default::default()
        };
        let json = export_environment(&state, "Staging").unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["_postman_variable_scope"], "environment");
        assert_eq!(v["name"], "Staging");
        assert_eq!(v["values"][1]["type"], "secret");
        assert_eq!(v["values"][2]["enabled"], false);
        assert_eq!(
            import_postman(&json).unwrap().environments,
            state.environments
        );
        assert!(export_environment(&state, "Nope").is_err());
    }
}
