//! Rules the person sets from the app (or an AI client they approved): which
//! hosts are decrypted (SSL proxying), which URLs are answered from a local
//! file (Map Local), and mocks, remote maps, header rewrites, throttles and
//! breakpoints.
//!
//! State lives in one JSON file in the data directory, so it is easy to read,
//! diff and keep in git. The data path only ever takes read locks, and the
//! file is read at request time for mapped responses so edits show up at once.

mod pattern;
mod pause;
mod rule;

pub use pattern::{host_matches, ssl_entry_matches};
pub use pause::{PauseEvent, PausedFlow, Pauses};
pub use rule::{
    apply_header_edits, Action, BreakpointPhase, HeaderEdit, HeaderOp, Match, Rule,
    DEFAULT_BREAKPOINT_TIMEOUT_S, MAX_BREAKPOINT_TIMEOUT_S, MAX_DELAY_MS, MAX_MOCK_BYTES,
};

use bytes::Bytes;
use flow::{
    BoxFuture, ConnectPolicy, Delay, Edited, FlowSummary, Gate, Held, Intercept, Phase,
    RequestInterceptor, ResponseInterceptor, Retarget, Ticket, Verdict,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

/// Largest file a rule may serve.
pub const MAX_MAPPED_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SslMode {
    /// Decrypt every host (the default: zero configuration).
    #[default]
    All,
    /// Decrypt only the hosts in the list; tunnel the rest untouched.
    Listed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SslSettings {
    pub mode: SslMode,
    /// Used in `Listed` mode. `example.com`, `*.example.com`,
    /// `api.example.com:8443`, `*`.
    pub hosts: Vec<String>,
    /// Never decrypted, in either mode. Wins over `hosts`.
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapLocalRule {
    #[serde(default)]
    pub id: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Host pattern; `*` matches any run of characters.
    pub host: String,
    /// 0 matches any port.
    #[serde(default)]
    pub port: u16,
    /// Exact path, or a prefix when it ends with `*`.
    pub path: String,
    /// Exact query string (without `?`). Empty matches any query.
    #[serde(default)]
    pub query: String,
    pub file: PathBuf,
    #[serde(default = "ok_status")]
    pub status: u16,
    /// Empty: guessed from the file extension.
    #[serde(default)]
    pub content_type: String,
}

fn yes() -> bool {
    true
}
fn ok_status() -> u16 {
    200
}

/// What an AI client connected through MCP may change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpWrites {
    /// Every change waits for approval in the desktop app (the default).
    #[default]
    Ask,
    /// Changes apply at once. For people who trust their client.
    Auto,
    /// Read only.
    Off,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    ssl: SslSettings,
    #[serde(default)]
    map_local: Vec<MapLocalRule>,
    #[serde(default)]
    rules: Vec<Rule>,
    #[serde(default)]
    mcp_writes: McpWrites,
}

#[derive(Debug)]
pub enum RulesError {
    Invalid(String),
    Io(std::io::Error),
}

impl std::fmt::Display for RulesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RulesError::Invalid(m) => f.write_str(m),
            RulesError::Io(e) => write!(f, "could not save rules: {e}"),
        }
    }
}

impl std::error::Error for RulesError {}

pub struct RuleSet {
    state: RwLock<State>,
    path: Option<PathBuf>,
    next: AtomicU64,
    pauses: Pauses,
}

impl RuleSet {
    /// Rules that live only in memory. For tests and the demo.
    pub fn in_memory() -> RuleSet {
        RuleSet {
            state: RwLock::new(State::default()),
            path: None,
            next: AtomicU64::new(1),
            pauses: Pauses::default(),
        }
    }

    /// Load `rules.json` from `dir`; a missing or unreadable file starts empty
    /// (an unreadable one is kept aside as `rules.json.bad`, never overwritten).
    pub fn load(dir: &Path) -> RuleSet {
        let path = dir.join("rules.json");
        let mut state = State::default();
        match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<State>(&bytes) {
                Ok(s) => state = s,
                Err(e) => {
                    tracing::warn!("{} is not valid ({e}); starting with no rules", path.display());
                    let _ = std::fs::rename(&path, dir.join("rules.json.bad"));
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!("could not read {}: {e}", path.display()),
        }
        let num = |id: &str| id.strip_prefix('r').and_then(|n| n.parse::<u64>().ok());
        let mut max = state
            .map_local
            .iter()
            .filter_map(|r| num(&r.id))
            .chain(state.rules.iter().filter_map(|r| num(&r.id)))
            .max()
            .unwrap_or(0);
        for id in state
            .map_local
            .iter_mut()
            .map(|r| &mut r.id)
            .chain(state.rules.iter_mut().map(|r| &mut r.id))
        {
            if id.is_empty() {
                max += 1;
                *id = format!("r{max}");
            }
        }
        RuleSet {
            state: RwLock::new(state),
            path: Some(path),
            next: AtomicU64::new(max + 1),
            pauses: Pauses::default(),
        }
    }

    fn save(&self, state: &State) -> Result<(), RulesError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let json = serde_json::to_vec_pretty(state)
            .map_err(|e| RulesError::Invalid(e.to_string()))?;
        // Write beside the target and rename, so a crash never leaves half a file.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(RulesError::Io)?;
        std::fs::rename(&tmp, path).map_err(RulesError::Io)
    }

    pub fn ssl(&self) -> SslSettings {
        self.state.read().unwrap().ssl.clone()
    }

    pub fn set_ssl(&self, settings: SslSettings) -> Result<SslSettings, RulesError> {
        fn clean(list: Vec<String>) -> Result<Vec<String>, RulesError> {
            let mut out: Vec<String> = Vec::new();
            for h in list {
                let h = h.trim().to_ascii_lowercase();
                if h.is_empty() {
                    continue;
                }
                if h.len() > 253 || h.chars().any(|c| c.is_whitespace() || c == '/') {
                    return Err(RulesError::Invalid(format!(
                        "\"{h}\" is not a host pattern"
                    )));
                }
                if !out.contains(&h) {
                    out.push(h);
                }
            }
            Ok(out)
        }
        let new = SslSettings {
            mode: settings.mode,
            hosts: clean(settings.hosts)?,
            exclude: clean(settings.exclude)?,
        };
        let mut st = self.state.write().unwrap();
        st.ssl = new.clone();
        self.save(&st)?;
        Ok(new)
    }

    /// Make sure `host` is decrypted: out of the exclude list and, when only
    /// listed hosts are decrypted, into the list. Other hosts are untouched.
    pub fn enable_host(&self, host: &str) -> Result<SslSettings, RulesError> {
        let mut s = self.ssl();
        let h = host.trim().to_ascii_lowercase();
        s.exclude.retain(|e| e != &h);
        if s.mode == SslMode::Listed && !s.hosts.iter().any(|e| e == &h) {
            s.hosts.push(h);
        }
        self.set_ssl(s)
    }

    /// Make sure `host` is tunneled untouched.
    pub fn disable_host(&self, host: &str) -> Result<SslSettings, RulesError> {
        let mut s = self.ssl();
        let h = host.trim().to_ascii_lowercase();
        s.hosts.retain(|e| e != &h);
        if !s.exclude.iter().any(|e| e == &h) {
            s.exclude.push(h);
        }
        self.set_ssl(s)
    }

    pub fn map_local(&self) -> Vec<MapLocalRule> {
        self.state.read().unwrap().map_local.clone()
    }

    /// Add a rule, or replace the one with the same id.
    pub fn put_map_local(&self, mut rule: MapLocalRule) -> Result<MapLocalRule, RulesError> {
        rule.host = rule.host.trim().to_ascii_lowercase();
        if rule.host.is_empty() {
            return Err(RulesError::Invalid("a rule needs a host".into()));
        }
        if !rule.path.starts_with('/') {
            return Err(RulesError::Invalid("the path must start with /".into()));
        }
        if rule.file.as_os_str().is_empty() {
            return Err(RulesError::Invalid("choose a file to serve".into()));
        }
        if !(100..=599).contains(&rule.status) {
            rule.status = 200;
        }
        if rule.id.is_empty() {
            rule.id = format!("r{}", self.next.fetch_add(1, Ordering::Relaxed));
        }
        let mut st = self.state.write().unwrap();
        match st.map_local.iter_mut().find(|r| r.id == rule.id) {
            Some(slot) => *slot = rule.clone(),
            None => {
                // The same target replaces the older rule rather than stacking.
                st.map_local.retain(|r| {
                    !(r.host == rule.host
                        && r.port == rule.port
                        && r.path == rule.path
                        && r.query == rule.query)
                });
                st.map_local.push(rule.clone());
            }
        }
        self.save(&st)?;
        Ok(rule)
    }

    /// Remove a rule of any kind by id.
    pub fn remove(&self, id: &str) -> Result<bool, RulesError> {
        let mut st = self.state.write().unwrap();
        let before = st.map_local.len() + st.rules.len();
        st.map_local.retain(|r| r.id != id);
        st.rules.retain(|r| r.id != id);
        let changed = st.map_local.len() + st.rules.len() != before;
        if changed {
            self.save(&st)?;
        }
        Ok(changed)
    }

    /// Mock, map remote, rewrite, throttle and breakpoint rules, in the order
    /// they apply.
    pub fn rules(&self) -> Vec<Rule> {
        self.state.read().unwrap().rules.clone()
    }

    /// Add a rule, or replace the one with the same id.
    pub fn put_rule(&self, mut rule: Rule) -> Result<Rule, RulesError> {
        rule.validate().map_err(RulesError::Invalid)?;
        let mut st = self.state.write().unwrap();
        if rule.id.is_empty() {
            rule.id = format!("r{}", self.next.fetch_add(1, Ordering::Relaxed));
        } else if st.map_local.iter().any(|r| r.id == rule.id) {
            return Err(RulesError::Invalid(format!(
                "{} is a Map Local rule, not a {}",
                rule.id,
                rule.kind()
            )));
        }
        match st.rules.iter_mut().find(|r| r.id == rule.id) {
            Some(slot) => *slot = rule.clone(),
            None => st.rules.push(rule.clone()),
        }
        self.save(&st)?;
        Ok(rule)
    }

    /// Turn one rule of any kind on or off. False if there is no such rule.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<bool, RulesError> {
        let mut st = self.state.write().unwrap();
        let mut found = false;
        for r in st.map_local.iter_mut().filter(|r| r.id == id) {
            r.enabled = enabled;
            found = true;
        }
        for r in st.rules.iter_mut().filter(|r| r.id == id) {
            r.enabled = enabled;
            found = true;
        }
        if found {
            self.save(&st)?;
        }
        Ok(found)
    }

    pub fn mcp_writes(&self) -> McpWrites {
        self.state.read().unwrap().mcp_writes
    }

    pub fn set_mcp_writes(&self, mode: McpWrites) -> Result<McpWrites, RulesError> {
        let mut st = self.state.write().unwrap();
        st.mcp_writes = mode;
        self.save(&st)?;
        Ok(mode)
    }

    /// Messages held at breakpoints.
    pub fn pauses(&self) -> &Pauses {
        &self.pauses
    }

    /// Would this host be decrypted under the current list?
    pub fn decrypts(&self, host: &str, port: u16) -> bool {
        let st = self.state.read().unwrap();
        if st
            .ssl
            .exclude
            .iter()
            .any(|e| ssl_entry_matches(e, host, port))
        {
            return false;
        }
        match st.ssl.mode {
            SslMode::All => true,
            SslMode::Listed => st
                .ssl
                .hosts
                .iter()
                .any(|e| ssl_entry_matches(e, host, port)),
        }
    }

    fn find(&self, host: &str, port: u16, path: &str, query: &str) -> Option<MapLocalRule> {
        let st = self.state.read().unwrap();
        // Most specific first: exact path before prefix, longest prefix first.
        st.map_local
            .iter()
            .filter(|r| r.enabled && rule_matches(r, host, port, path, query))
            .max_by_key(|r| {
                (
                    !r.path.ends_with('*'),
                    !r.query.is_empty(),
                    r.path.len(),
                )
            })
            .cloned()
    }
}

fn rule_matches(r: &MapLocalRule, host: &str, port: u16, path: &str, query: &str) -> bool {
    if !host_matches(&r.host, host) {
        return false;
    }
    if r.port != 0 && r.port != port {
        return false;
    }
    let path_ok = match r.path.strip_suffix('*') {
        Some(prefix) => path.starts_with(prefix),
        None => r.path == path,
    };
    path_ok && (r.query.is_empty() || r.query == query)
}

/// Content type from a file extension. Small on purpose.
pub fn guess_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("json") => "application/json",
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") | Some("mjs") => "application/javascript",
        Some("txt") | Some("log") => "text/plain; charset=utf-8",
        Some("xml") => "application/xml",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("pdf") => "application/pdf",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}

impl ConnectPolicy for RuleSet {
    fn should_decrypt(&self, host: &str, port: u16) -> bool {
        self.decrypts(host, port)
    }
}

/// Split `path?query` the way a flow summary carries it.
fn split_path(path_and_query: &str) -> (&str, &str) {
    path_and_query
        .split_once('?')
        .unwrap_or((path_and_query, ""))
}

impl RequestInterceptor for RuleSet {
    fn on_request(&self, flow: &FlowSummary, parts: &mut http::request::Parts) -> Intercept {
        // The common case is no rules at all: leave without touching a path.
        {
            let st = self.state.read().unwrap();
            if st.map_local.is_empty() && st.rules.is_empty() {
                return Intercept::Continue;
            }
        }
        let path = parts.uri.path().to_string();
        let query = parts.uri.query().unwrap_or("").to_string();
        let host = flow.host.to_ascii_lowercase();
        let method = parts.method.as_str().to_string();

        // Rules in order: every matching rewrite applies, the first remote
        // map wins, delays take the longest, the first mock answers.
        let mut delay = 0u32;
        let mut retarget: Option<Retarget> = None;
        let mut mock: Option<http::Response<Bytes>> = None;
        let mut edited = false;
        {
            let st = self.state.read().unwrap();
            for r in st.rules.iter().filter(|r| r.enabled) {
                if !r
                    .matcher
                    .matches(&flow.scheme, &host, flow.port, &method, &path, &query)
                {
                    continue;
                }
                match &r.action {
                    Action::Rewrite { request_headers, .. } => {
                        edited |= apply_header_edits(&mut parts.headers, request_headers);
                    }
                    Action::MapRemote { scheme, host, port } if retarget.is_none() => {
                        retarget = Some(Retarget {
                            scheme: match scheme.as_str() {
                                "http" => Some("http"),
                                "https" => Some("https"),
                                _ => None,
                            },
                            host: (!host.is_empty()).then(|| host.clone()),
                            port: (*port != 0).then_some(*port),
                        });
                    }
                    Action::Throttle { delay_ms } => delay = delay.max(*delay_ms),
                    Action::Mock {
                        status,
                        headers,
                        body,
                        delay_ms,
                    } if mock.is_none() => {
                        delay = delay.max(*delay_ms);
                        let mut b = http::Response::builder().status(*status);
                        let mut has_type = false;
                        for (n, v) in headers {
                            has_type |= n.eq_ignore_ascii_case("content-type");
                            b = b.header(n.as_str(), v.as_str());
                        }
                        if !has_type {
                            b = b.header(http::header::CONTENT_TYPE, mock_content_type(body));
                        }
                        mock = b
                            .header("x-neko-mock", r.id.as_str())
                            .body(Bytes::from(body.clone()))
                            .ok();
                    }
                    _ => {}
                }
            }
        }
        if delay > 0 {
            parts
                .extensions
                .insert(Delay(std::time::Duration::from_millis(delay as u64)));
        }
        if let Some(r) = retarget {
            parts.extensions.insert(r);
            edited = true;
        }
        if edited {
            parts.extensions.insert(Edited);
        }
        if let Some(res) = mock {
            return Intercept::Respond(res);
        }

        let Some(rule) = self.find(&host, flow.port, &path, &query) else {
            return Intercept::Continue;
        };
        let meta = match std::fs::metadata(&rule.file) {
            Ok(m) if m.is_file() && m.len() <= MAX_MAPPED_BYTES => m,
            Ok(_) => return failure(&rule, "is not a regular file or is too large"),
            Err(e) => return failure(&rule, &e.to_string()),
        };
        let _ = meta;
        let body = match std::fs::read(&rule.file) {
            Ok(b) => b,
            Err(e) => return failure(&rule, &e.to_string()),
        };
        let ct = if rule.content_type.is_empty() {
            guess_content_type(&rule.file).to_string()
        } else {
            rule.content_type.clone()
        };
        let res = http::Response::builder()
            .status(rule.status)
            .header(http::header::CONTENT_TYPE, ct)
            .header(http::header::CONTENT_LENGTH, body.len())
            .header(http::header::CACHE_CONTROL, "no-store")
            .header("x-neko-map-local", rule.id.as_str())
            .body(Bytes::from(body))
            .unwrap_or_else(|_| http::Response::new(Bytes::new()));
        Intercept::Respond(res)
    }
}

/// JSON when the body parses as JSON, otherwise plain text.
fn mock_content_type(body: &str) -> &'static str {
    let t = body.trim_start();
    if (t.starts_with('{') || t.starts_with('[')) && serde_json::from_str::<serde_json::Value>(body).is_ok()
    {
        "application/json"
    } else {
        "text/plain; charset=utf-8"
    }
}

impl ResponseInterceptor for RuleSet {
    fn on_response(&self, flow: &FlowSummary, parts: &mut http::response::Parts) {
        let st = self.state.read().unwrap();
        if st.rules.is_empty() {
            return;
        }
        let (path, query) = split_path(&flow.path);
        let host = flow.host.to_ascii_lowercase();
        for r in st.rules.iter().filter(|r| r.enabled) {
            if let Action::Rewrite { response_headers, .. } = &r.action {
                if r.matcher
                    .matches(&flow.scheme, &host, flow.port, &flow.method, path, query)
                {
                    apply_header_edits(&mut parts.headers, response_headers);
                }
            }
        }
    }
}

impl RuleSet {
    fn breakpoint_ticket(
        &self,
        flow: &FlowSummary,
        want: Phase,
    ) -> Option<Ticket> {
        let st = self.state.read().unwrap();
        let (path, query) = split_path(&flow.path);
        let host = flow.host.to_ascii_lowercase();
        st.rules.iter().filter(|r| r.enabled).find_map(|r| {
            let Action::Breakpoint { phase, timeout_s } = &r.action else {
                return None;
            };
            let phase_ok = matches!(
                (phase, want),
                (BreakpointPhase::Both, _)
                    | (BreakpointPhase::Request, Phase::Request)
                    | (BreakpointPhase::Response, Phase::Response)
            );
            if !phase_ok
                || !r
                    .matcher
                    .matches(&flow.scheme, &host, flow.port, &flow.method, path, query)
            {
                return None;
            }
            let secs = if *timeout_s == 0 {
                DEFAULT_BREAKPOINT_TIMEOUT_S
            } else {
                *timeout_s
            };
            Some(Ticket {
                rule_id: r.id.clone(),
                timeout: std::time::Duration::from_secs(secs as u64),
            })
        })
    }
}

impl Gate for RuleSet {
    fn request_ticket(&self, flow: &FlowSummary, _parts: &http::request::Parts) -> Option<Ticket> {
        if self.state.read().unwrap().rules.is_empty() {
            return None;
        }
        self.breakpoint_ticket(flow, Phase::Request)
    }

    fn response_ticket(&self, flow: &FlowSummary, _parts: &http::response::Parts) -> Option<Ticket> {
        if self.state.read().unwrap().rules.is_empty() {
            return None;
        }
        self.breakpoint_ticket(flow, Phase::Response)
    }

    fn hold<'a>(
        &'a self,
        flow: &'a FlowSummary,
        phase: Phase,
        ticket: Ticket,
        held: Held,
    ) -> BoxFuture<'a, Verdict> {
        Box::pin(async move {
            self.pauses
                .hold(flow, ticket.rule_id, phase, ticket.timeout, held)
                .await
        })
    }
}

/// A rule whose file cannot be served says so, rather than silently falling
/// through to the real server and hiding the mistake.
fn failure(rule: &MapLocalRule, why: &str) -> Intercept {
    let msg = format!("Map Local: {} {}", rule.file.display(), why);
    let res = http::Response::builder()
        .status(502)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header("x-neko-map-local", rule.id.as_str())
        .body(Bytes::from(msg))
        .unwrap_or_else(|_| http::Response::new(Bytes::new()));
    Intercept::Respond(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(uri: &str) -> http::request::Parts {
        http::Request::builder()
            .uri(uri)
            .body(())
            .unwrap()
            .into_parts()
            .0
    }

    fn summary(host: &str, port: u16) -> FlowSummary {
        FlowSummary {
            host: host.into(),
            port,
            ..Default::default()
        }
    }

    fn rule(file: &Path) -> MapLocalRule {
        MapLocalRule {
            id: String::new(),
            enabled: true,
            host: "api.example.com".into(),
            port: 0,
            path: "/v1/me".into(),
            query: String::new(),
            file: file.to_path_buf(),
            status: 200,
            content_type: String::new(),
        }
    }

    #[test]
    fn ssl_modes() {
        let r = RuleSet::in_memory();
        assert!(r.decrypts("anything.example", 443));
        r.set_ssl(SslSettings {
            mode: SslMode::Listed,
            hosts: vec!["*.example.com".into(), "Pay.test:8443".into()],
            exclude: vec!["internal.example.com".into()],
        })
        .unwrap();
        assert!(r.decrypts("api.example.com", 443));
        assert!(!r.decrypts("example.com", 443));
        assert!(r.decrypts("pay.test", 8443));
        assert!(!r.decrypts("pay.test", 443));
        assert!(!r.decrypts("other.org", 443));
        assert!(!r.decrypts("internal.example.com", 443));
    }

    #[test]
    fn enable_and_disable_one_host() {
        let r = RuleSet::in_memory();
        // In "all" mode, disabling one host leaves every other host decrypted.
        let s = r.disable_host(" API.example.com ").unwrap();
        assert_eq!(s.mode, SslMode::All);
        assert_eq!(s.exclude, ["api.example.com"]);
        assert!(!r.decrypts("api.example.com", 443));
        assert!(r.decrypts("cdn.example.com", 443));
        let s = r.enable_host("api.example.com").unwrap();
        assert!(s.exclude.is_empty());
        assert!(s.hosts.is_empty(), "all mode needs no list entry");
        assert!(r.decrypts("api.example.com", 443));

        // In list mode, enabling adds to the list once.
        r.set_ssl(SslSettings {
            mode: SslMode::Listed,
            ..Default::default()
        })
        .unwrap();
        assert!(!r.decrypts("api.example.com", 443));
        r.enable_host("api.example.com").unwrap();
        r.enable_host("api.example.com").unwrap();
        assert_eq!(r.ssl().hosts, ["api.example.com"]);
        assert!(r.decrypts("api.example.com", 443));
        r.disable_host("api.example.com").unwrap();
        assert!(r.ssl().hosts.is_empty());
        assert!(!r.decrypts("api.example.com", 443));
    }

    #[test]
    fn rejects_bad_host_patterns() {
        let r = RuleSet::in_memory();
        let e = r.set_ssl(SslSettings {
            mode: SslMode::Listed,
            hosts: vec!["https://x.com/".into()],
            exclude: vec![],
        });
        assert!(e.is_err());
    }

    #[test]
    fn map_local_serves_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("me.json");
        std::fs::write(&f, br#"{"mapped":true}"#).unwrap();
        let r = RuleSet::in_memory();
        r.put_map_local(rule(&f)).unwrap();

        let mut p = parts("/v1/me?x=1");
        let out = r.on_request(&summary("API.example.com", 443), &mut p);
        let Intercept::Respond(res) = out else {
            panic!("expected a mapped response")
        };
        assert_eq!(res.status(), 200);
        assert_eq!(res.headers()["content-type"], "application/json");
        assert_eq!(res.body().as_ref(), br#"{"mapped":true}"#);

        // Other paths, hosts and disabled rules fall through.
        let mut p = parts("/v1/other");
        assert!(matches!(
            r.on_request(&summary("api.example.com", 443), &mut p),
            Intercept::Continue
        ));
        let mut p = parts("/v1/me");
        assert!(matches!(
            r.on_request(&summary("cdn.example.com", 443), &mut p),
            Intercept::Continue
        ));
    }

    #[test]
    fn exact_beats_prefix_and_query_narrows() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        std::fs::write(&a, "prefix").unwrap();
        std::fs::write(&b, "exact").unwrap();
        let r = RuleSet::in_memory();
        let mut pre = rule(&a);
        pre.path = "/v1/*".into();
        r.put_map_local(pre).unwrap();
        r.put_map_local(rule(&b)).unwrap();

        let mut p = parts("/v1/me");
        let Intercept::Respond(res) = r.on_request(&summary("api.example.com", 443), &mut p) else {
            panic!()
        };
        assert_eq!(res.body().as_ref(), b"exact");
        let mut p = parts("/v1/zzz");
        let Intercept::Respond(res) = r.on_request(&summary("api.example.com", 443), &mut p) else {
            panic!()
        };
        assert_eq!(res.body().as_ref(), b"prefix");

        let mut q = rule(&b);
        q.path = "/search".into();
        q.query = "q=a".into();
        r.put_map_local(q).unwrap();
        let mut p = parts("/search?q=a");
        assert!(matches!(
            r.on_request(&summary("api.example.com", 443), &mut p),
            Intercept::Respond(_)
        ));
        let mut p = parts("/search?q=b");
        assert!(matches!(
            r.on_request(&summary("api.example.com", 443), &mut p),
            Intercept::Continue
        ));
    }

    #[test]
    fn missing_file_is_loud() {
        let r = RuleSet::in_memory();
        r.put_map_local(rule(Path::new("/no/such/file.json"))).unwrap();
        let mut p = parts("/v1/me");
        let Intercept::Respond(res) = r.on_request(&summary("api.example.com", 443), &mut p) else {
            panic!()
        };
        assert_eq!(res.status(), 502);
        assert!(String::from_utf8_lossy(res.body()).contains("/no/such/file.json"));
    }

    #[test]
    fn same_target_replaces_and_ids_stay_unique() {
        let r = RuleSet::in_memory();
        let a = r.put_map_local(rule(Path::new("/a.json"))).unwrap();
        let b = r.put_map_local(rule(Path::new("/b.json"))).unwrap();
        assert_ne!(a.id, b.id);
        let all = r.map_local();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].file, Path::new("/b.json"));
        assert!(r.remove(&b.id).unwrap());
        assert!(!r.remove(&b.id).unwrap());
    }

    #[test]
    fn persists_and_survives_a_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        {
            let r = RuleSet::load(dir.path());
            r.set_ssl(SslSettings {
                mode: SslMode::Listed,
                hosts: vec!["api.example.com".into()],
                exclude: vec!["skip.example.com".into()],
            })
            .unwrap();
            r.put_map_local(rule(Path::new("/x.json"))).unwrap();
        }
        let r = RuleSet::load(dir.path());
        assert_eq!(r.ssl().hosts, ["api.example.com"]);
        assert_eq!(r.ssl().exclude, ["skip.example.com"]);
        assert_eq!(r.map_local().len(), 1);
        let next = r.put_map_local(MapLocalRule {
            path: "/other".into(),
            ..rule(Path::new("/y.json"))
        });
        assert_ne!(next.unwrap().id, r.map_local()[0].id);

        std::fs::write(dir.path().join("rules.json"), b"{ not json").unwrap();
        let r = RuleSet::load(dir.path());
        assert!(r.map_local().is_empty());
        assert!(dir.path().join("rules.json.bad").exists());
    }

    fn m(host: &str, path: &str) -> Match {
        Match {
            scheme: String::new(),
            host: host.into(),
            port: 0,
            method: String::new(),
            path: path.into(),
            query: String::new(),
        }
    }

    fn put(r: &RuleSet, matcher: Match, action: Action) -> Rule {
        r.put_rule(Rule {
            id: String::new(),
            name: String::new(),
            enabled: true,
            matcher,
            action,
        })
        .unwrap()
    }

    #[test]
    fn mock_answers_and_beats_map_local() {
        let r = RuleSet::in_memory();
        put(
            &r,
            m("api.example.com", "/v1/me"),
            Action::Mock {
                status: 503,
                headers: vec![],
                body: r#"{"error":"down"}"#.into(),
                delay_ms: 250,
            },
        );
        let mut p = parts("/v1/me");
        let Intercept::Respond(res) = r.on_request(&summary("api.example.com", 443), &mut p) else {
            panic!("expected a mock")
        };
        assert_eq!(res.status(), 503);
        assert_eq!(res.headers()["content-type"], "application/json");
        assert!(res.headers().contains_key("x-neko-mock"));
        let d = p.extensions.get::<Delay>().expect("mock delay");
        assert_eq!(d.0.as_millis(), 250);

        let mut p = parts("/v1/other");
        assert!(matches!(
            r.on_request(&summary("api.example.com", 443), &mut p),
            Intercept::Continue
        ));
    }

    #[test]
    fn rewrite_remap_and_throttle_leave_marks() {
        let r = RuleSet::in_memory();
        put(
            &r,
            m("*", ""),
            Action::Rewrite {
                request_headers: vec![HeaderEdit {
                    op: HeaderOp::Set,
                    name: "x-env".into(),
                    value: "staging".into(),
                }],
                response_headers: vec![HeaderEdit {
                    op: HeaderOp::Remove,
                    name: "set-cookie".into(),
                    value: String::new(),
                }],
            },
        );
        put(
            &r,
            m("api.example.com", ""),
            Action::MapRemote {
                scheme: "http".into(),
                host: "localhost".into(),
                port: 3000,
            },
        );
        put(&r, m("api.example.com", ""), Action::Throttle { delay_ms: 800 });

        let mut p = parts("/v1/me");
        let out = r.on_request(&summary("api.example.com", 443), &mut p);
        assert!(matches!(out, Intercept::Continue));
        assert_eq!(p.headers["x-env"], "staging");
        let t = p.extensions.get::<Retarget>().unwrap();
        assert_eq!(t.host.as_deref(), Some("localhost"));
        assert_eq!(t.port, Some(3000));
        assert_eq!(t.scheme, Some("http"));
        assert_eq!(p.extensions.get::<Delay>().unwrap().0.as_millis(), 800);
        assert!(p.extensions.get::<Edited>().is_some());

        let mut res = http::Response::builder()
            .header("set-cookie", "a=b")
            .body(())
            .unwrap()
            .into_parts()
            .0;
        r.on_response(
            &FlowSummary {
                host: "api.example.com".into(),
                port: 443,
                method: "GET".into(),
                path: "/v1/me".into(),
                ..Default::default()
            },
            &mut res,
        );
        assert!(!res.headers.contains_key("set-cookie"));
    }

    #[test]
    fn disabled_rules_do_nothing_and_ids_are_shared() {
        let r = RuleSet::in_memory();
        let a = put(&r, m("a.test", ""), Action::Throttle { delay_ms: 10 });
        let b = r
            .put_map_local(rule(Path::new("/f.json")))
            .unwrap();
        assert_ne!(a.id, b.id);
        assert!(r.set_enabled(&a.id, false).unwrap());
        assert!(!r.set_enabled("nope", false).unwrap());
        let mut p = parts("/");
        r.on_request(&summary("a.test", 80), &mut p);
        assert!(p.extensions.get::<Delay>().is_none());
        assert!(r.remove(&a.id).unwrap());
        assert!(r.rules().is_empty());
        assert_eq!(r.map_local().len(), 1);
    }

    #[test]
    fn rules_persist() {
        let dir = tempfile::tempdir().unwrap();
        {
            let r = RuleSet::load(dir.path());
            put(&r, m("a.test", "/x"), Action::Throttle { delay_ms: 10 });
            r.set_mcp_writes(McpWrites::Auto).unwrap();
        }
        let r = RuleSet::load(dir.path());
        assert_eq!(r.rules().len(), 1);
        assert_eq!(r.mcp_writes(), McpWrites::Auto);
        let again = put(&r, m("b.test", ""), Action::Throttle { delay_ms: 5 });
        assert_ne!(again.id, r.rules()[0].id);
    }

    fn held_request() -> Held {
        Held {
            method: "GET".into(),
            path_and_query: "/v1/me".into(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn breakpoint_holds_until_resumed_with_edits() {
        let r = std::sync::Arc::new(RuleSet::in_memory());
        put(
            &r,
            m("api.example.com", "/v1/*"),
            Action::Breakpoint {
                phase: BreakpointPhase::Request,
                timeout_s: 5,
            },
        );
        let flow = FlowSummary {
            id: 7,
            scheme: "https".into(),
            host: "api.example.com".into(),
            port: 443,
            method: "GET".into(),
            path: "/v1/me".into(),
            ..Default::default()
        };
        let p = parts("/v1/me");
        let ticket = r.request_ticket(&flow, &p).expect("breakpoint matches");
        // Not the response phase.
        let rp = http::Response::new(()).into_parts().0;
        assert!(r.response_ticket(&flow, &rp).is_none());

        let mut events = r.pauses().subscribe();
        let r2 = r.clone();
        let f2 = flow.clone();
        let task = tokio::spawn(async move {
            r2.hold(&f2, Phase::Request, ticket, held_request()).await
        });
        let PauseEvent::Paused(pf) = events.recv().await.unwrap() else {
            panic!("expected a pause")
        };
        assert_eq!(pf.flow_id, 7);
        assert_eq!(r.pauses().list().len(), 1);

        let edits = flow::Edits {
            path_and_query: Some("/v1/other".into()),
            ..Default::default()
        };
        assert!(r.pauses().resume(7, Verdict::Continue(edits)));
        match task.await.unwrap() {
            Verdict::Continue(e) => assert_eq!(e.path_and_query.as_deref(), Some("/v1/other")),
            Verdict::Abort(_) => panic!("not aborted"),
        }
        assert!(r.pauses().list().is_empty());
        assert!(!r.pauses().resume(7, Verdict::Abort("late".into())));
    }

    #[tokio::test]
    async fn breakpoint_times_out_and_lets_the_message_through() {
        let pauses = Pauses::default();
        let flow = FlowSummary {
            id: 9,
            ..Default::default()
        };
        let v = pauses
            .hold(
                &flow,
                "r1".into(),
                Phase::Request,
                std::time::Duration::from_millis(30),
                held_request(),
            )
            .await;
        assert!(matches!(v, Verdict::Continue(e) if e.is_empty()));
        assert!(pauses.list().is_empty());
    }

    #[tokio::test]
    async fn dropping_the_waiter_clears_the_slot() {
        let pauses = std::sync::Arc::new(Pauses::default());
        let p2 = pauses.clone();
        let task = tokio::spawn(async move {
            let flow = FlowSummary {
                id: 3,
                ..Default::default()
            };
            p2.hold(
                &flow,
                "r1".into(),
                Phase::Response,
                std::time::Duration::from_secs(30),
                Held::default(),
            )
            .await
        });
        while pauses.list().is_empty() {
            tokio::task::yield_now().await;
        }
        task.abort();
        let _ = task.await;
        assert!(pauses.list().is_empty());
    }
}
