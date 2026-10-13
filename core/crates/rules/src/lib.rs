//! Rules the person sets from the app: which hosts are decrypted (SSL
//! proxying) and which URLs are answered from a local file (Map Local).
//!
//! State lives in one JSON file in the data directory, so it is easy to read,
//! diff and keep in git. The data path only ever takes read locks, and the
//! file is read at request time for mapped responses so edits show up at once.

mod pattern;

pub use pattern::{host_matches, ssl_entry_matches};

use bytes::Bytes;
use flow::{ConnectPolicy, FlowSummary, Intercept, RequestInterceptor};
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    ssl: SslSettings,
    #[serde(default)]
    map_local: Vec<MapLocalRule>,
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
}

impl RuleSet {
    /// Rules that live only in memory. For tests and the demo.
    pub fn in_memory() -> RuleSet {
        RuleSet {
            state: RwLock::new(State::default()),
            path: None,
            next: AtomicU64::new(1),
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
        let max = state
            .map_local
            .iter()
            .filter_map(|r| r.id.strip_prefix('r').and_then(|n| n.parse::<u64>().ok()))
            .max()
            .unwrap_or(0);
        for r in &mut state.map_local {
            if r.id.is_empty() {
                r.id = format!("r{}", max + 1);
            }
        }
        RuleSet {
            state: RwLock::new(state),
            path: Some(path),
            next: AtomicU64::new(max + 1),
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

    pub fn remove(&self, id: &str) -> Result<bool, RulesError> {
        let mut st = self.state.write().unwrap();
        let before = st.map_local.len();
        st.map_local.retain(|r| r.id != id);
        let changed = st.map_local.len() != before;
        if changed {
            self.save(&st)?;
        }
        Ok(changed)
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

impl RequestInterceptor for RuleSet {
    fn on_request(&self, flow: &FlowSummary, parts: &mut http::request::Parts) -> Intercept {
        // The common case is no rules at all: leave without touching a path.
        if self.state.read().unwrap().map_local.is_empty() {
            return Intercept::Continue;
        }
        let path = parts.uri.path();
        let query = parts.uri.query().unwrap_or("");
        let host = flow.host.to_ascii_lowercase();
        let Some(rule) = self.find(&host, flow.port, path, query) else {
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
            .header("x-proxy-map-local", rule.id.as_str())
            .body(Bytes::from(body))
            .unwrap_or_else(|_| http::Response::new(Bytes::new()));
        Intercept::Respond(res)
    }
}

/// A rule whose file cannot be served says so, rather than silently falling
/// through to the real server and hiding the mistake.
fn failure(rule: &MapLocalRule, why: &str) -> Intercept {
    let msg = format!("Map Local: {} {}", rule.file.display(), why);
    let res = http::Response::builder()
        .status(502)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header("x-proxy-map-local", rule.id.as_str())
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
}
