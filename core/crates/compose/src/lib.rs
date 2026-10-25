//! The request composer: requests the person writes by hand and sends from the
//! app, saved in collections, with environments of `{{variables}}` and a
//! history of what was sent. Everything is kept in `composer.json` beside the
//! rules, so it is readable and survives restarts.
//!
//! Turning a composed request into the bytes to send lives in [`build`];
//! reading one back from a cURL command or a captured flow lives in [`curl`]
//! and [`from_parts`].

mod assertions;
mod build;
mod bulk;
mod codegen;
mod cookies;
mod curl;
mod postman;
mod urlsync;

pub use assertions::{run_assertions, ResponseView, TestResult};
pub use build::{build, missing_variables, Built, FormField};
pub use bulk::{bulk_format, bulk_parse};
pub use codegen::{code_languages, generate_code};
pub use cookies::{parse_set_cookies, Cookie};
pub use curl::{from_parts, parse_curl, to_curl};
pub use postman::{
    export_collection, export_environment, import_postman, Imported, PostmanKind,
};
pub use urlsync::{apply_url_text, display_url, sync_path_vars};

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// History keeps this many sends, newest first.
pub const HISTORY_LIMIT: usize = 100;

/// Where a saved request goes when no collection is named.
pub const DEFAULT_COLLECTION: &str = "My requests";

fn is_false(b: &bool) -> bool {
    !*b
}

/// One editable row: a header, query parameter, form field or variable.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KeyValue {
    pub name: String,
    #[serde(default)]
    pub value: String,
    /// Kept in the table but left out when sending.
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,
    /// A note shown beside the row.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// A variable whose value the app masks.
    #[serde(default, skip_serializing_if = "is_false")]
    pub secret: bool,
    /// A form-data row whose value is the path of a file to upload.
    #[serde(default, skip_serializing_if = "is_false")]
    pub file: bool,
}

impl KeyValue {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> KeyValue {
        KeyValue {
            name: name.into(),
            value: value.into(),
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BodyKind {
    #[default]
    None,
    Json,
    Text,
    Xml,
    /// `application/x-www-form-urlencoded` from the form rows.
    Form,
    Html,
    Javascript,
    /// `multipart/form-data` from the form rows; a row can be a file.
    FormData,
    /// A query in `body` and its variables in `graphql_variables`, sent as JSON.
    Graphql,
    /// The file at `binary_path`.
    Binary,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthKind {
    #[default]
    None,
    Bearer,
    Basic,
    /// Use the nearest collection or folder that sets one.
    Inherit,
    ApiKey,
}

/// The auth settings of a request, collection or folder.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Auth {
    pub kind: AuthKind,
    pub token: String,
    pub user: String,
    pub password: String,
    pub key: String,
    pub value: String,
    /// The API key goes in the query instead of a header.
    pub in_query: bool,
    /// Bearer prefix; empty means "Bearer".
    pub prefix: String,
}

/// What a test checks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssertKind {
    #[default]
    Status,
    /// `target` is the header name.
    Header,
    Body,
    /// `target` is a path such as `data.items[0].id`.
    Json,
    /// Response time in milliseconds.
    Time,
    /// Response size in bytes.
    Size,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssertOp {
    #[default]
    Equals,
    NotEquals,
    Contains,
    NotContains,
    Exists,
    NotExists,
    LessThan,
    GreaterThan,
    Matches,
}

/// One check run against the response after a send.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Assertion {
    pub kind: AssertKind,
    pub target: String,
    pub op: AssertOp,
    pub expected: String,
    #[serde(skip_serializing_if = "is_false")]
    pub disabled: bool,
}

/// A request as the person wrote it. Any text may hold `{{variables}}`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Request {
    /// Empty until saved.
    pub id: String,
    pub name: String,
    pub collection: String,
    pub method: String,
    pub url: String,
    /// Added to the URL's query.
    pub params: Vec<KeyValue>,
    pub headers: Vec<KeyValue>,
    pub body_kind: BodyKind,
    /// The body for JSON, text and XML.
    pub body: String,
    /// The fields for a form body.
    pub form: Vec<KeyValue>,
    pub auth: AuthKind,
    pub auth_token: String,
    pub auth_user: String,
    pub auth_password: String,
    pub auth_key: String,
    pub auth_value: String,
    pub auth_in_query: bool,
    pub auth_prefix: String,
    pub description: String,
    /// Values for `:name` segments in the URL path.
    pub path_vars: Vec<KeyValue>,
    pub graphql_variables: String,
    pub binary_path: String,
    pub tests: Vec<Assertion>,
    pub updated_ms: i64,
}

impl Request {
    pub fn auth(&self) -> Auth {
        Auth {
            kind: self.auth,
            token: self.auth_token.clone(),
            user: self.auth_user.clone(),
            password: self.auth_password.clone(),
            key: self.auth_key.clone(),
            value: self.auth_value.clone(),
            in_query: self.auth_in_query,
            prefix: self.auth_prefix.clone(),
        }
    }

    pub fn set_auth(&mut self, a: Auth) {
        self.auth = a.kind;
        self.auth_token = a.token;
        self.auth_user = a.user;
        self.auth_password = a.password;
        self.auth_key = a.key;
        self.auth_value = a.value;
        self.auth_in_query = a.in_query;
        self.auth_prefix = a.prefix;
    }
}

/// Settings that every request in a collection or folder inherits. `name` is
/// the collection path, with `/` between a folder and its parents.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CollectionConfig {
    pub name: String,
    pub description: String,
    /// `Inherit` (the default) passes the parent's auth down.
    pub auth: Auth,
    pub headers: Vec<KeyValue>,
    pub vars: Vec<KeyValue>,
}

impl Default for CollectionConfig {
    fn default() -> Self {
        CollectionConfig {
            name: String::new(),
            description: String::new(),
            auth: Auth {
                kind: AuthKind::Inherit,
                ..Default::default()
            },
            headers: Vec::new(),
            vars: Vec::new(),
        }
    }
}

/// A named set of variables, such as "staging" or "production".
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Environment {
    pub name: String,
    pub vars: Vec<KeyValue>,
}

/// One send, newest first in [`State::history`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryEntry {
    pub request: Request,
    /// The recorded flow; 0 when the request never left.
    pub flow_id: u64,
    pub status: u16,
    pub sent_ms: i64,
    pub duration_ms: f64,
    /// The method and URL after variables were filled in.
    pub method: String,
    pub url: String,
    pub error: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// In display order. A collection can be empty.
    pub collections: Vec<String>,
    pub requests: Vec<Request>,
    pub environments: Vec<Environment>,
    /// The environment whose variables fill `{{name}}`; empty for none.
    pub active_environment: String,
    pub history: Vec<HistoryEntry>,
    /// Variables available everywhere, below a collection's and an environment's.
    pub globals: Vec<KeyValue>,
    pub configs: Vec<CollectionConfig>,
}

/// A collection path without empty parts: `/Shop//Orders/` becomes `Shop/Orders`.
pub fn normalize_path(path: &str) -> String {
    path.split('/')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// `Shop/Orders` and its parents: `["Shop", "Shop/Orders"]`.
pub fn collection_chain(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for part in path.split('/').filter(|p| !p.is_empty()) {
        if !cur.is_empty() {
            cur.push('/');
        }
        cur.push_str(part);
        out.push(cur.clone());
    }
    out
}

impl State {
    /// The enabled variables of the active environment.
    pub fn variables(&self) -> Vec<KeyValue> {
        self.environments
            .iter()
            .find(|e| e.name == self.active_environment)
            .map(|e| e.vars.iter().filter(|v| !v.disabled).cloned().collect())
            .unwrap_or_default()
    }

    fn config(&self, name: &str) -> Option<&CollectionConfig> {
        self.configs.iter().find(|c| c.name == name)
    }

    /// Every variable that applies to a request in `collection`. Later entries
    /// win: globals, then the collection and its folders, then the environment.
    pub fn variables_for(&self, collection: &str) -> Vec<KeyValue> {
        let mut out: Vec<KeyValue> = self.globals.iter().filter(|v| !v.disabled).cloned().collect();
        for name in collection_chain(collection) {
            if let Some(c) = self.config(&name) {
                out.extend(c.vars.iter().filter(|v| !v.disabled).cloned());
            }
        }
        out.extend(self.variables());
        out
    }

    /// The request with inherited auth and headers filled in, and every
    /// variable that applies to it.
    pub fn prepare(&self, r: &Request) -> (Request, Vec<KeyValue>) {
        let mut req = r.clone();
        let chain = collection_chain(&r.collection);
        if req.auth == AuthKind::Inherit {
            let found = chain
                .iter()
                .rev()
                .filter_map(|n| self.config(n))
                .map(|c| &c.auth)
                .find(|a| a.kind != AuthKind::Inherit);
            req.set_auth(found.cloned().unwrap_or_default());
        }
        let mut headers: Vec<KeyValue> = Vec::new();
        for name in &chain {
            if let Some(c) = self.config(name) {
                for h in c.headers.iter().filter(|h| !h.disabled) {
                    headers.retain(|x| !x.name.eq_ignore_ascii_case(&h.name));
                    headers.push(h.clone());
                }
            }
        }
        headers.retain(|h| {
            !r.headers
                .iter()
                .any(|x| !x.disabled && x.name.eq_ignore_ascii_case(&h.name))
        });
        headers.extend(req.headers.drain(..));
        req.headers = headers;
        (req, self.variables_for(&r.collection))
    }
}

/// The composer's saved state, guarded and written through to disk.
pub struct Composer {
    state: Mutex<State>,
    path: Option<PathBuf>,
    write_error: Option<String>,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl Composer {
    /// Nothing is written to disk.
    pub fn in_memory() -> Composer {
        Composer {
            state: Mutex::new(State::default()),
            path: None,
            write_error: None,
        }
    }

    /// Load `composer.json` from `dir`; a missing file starts empty and an
    /// unreadable one is kept aside as `composer.json.bad`, never overwritten.
    pub fn load(dir: &Path) -> Composer {
        let path = dir.join("composer.json");
        let mut state = State::default();
        let mut write_error = None;
        let problem = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<State>(&bytes) {
                Ok(s) => {
                    state = s;
                    None
                }
                Err(e) => Some(e.to_string()),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => Some(e.to_string()),
        };
        if let Some(problem) = problem {
            tracing::warn!(
                "could not load {}: {problem}; starting empty",
                path.display()
            );
            let mut bad = dir.join("composer.json.bad");
            let mut suffix = 0;
            while bad.symlink_metadata().is_ok() {
                suffix += 1;
                bad = dir.join(format!("composer.json.bad.{suffix}"));
            }
            if let Err(e) = std::fs::rename(&path, &bad) {
                write_error = Some(format!(
                    "the composer file could not be preserved: {e}; editing is disabled"
                ));
            }
        }
        Composer {
            state: Mutex::new(state),
            path: Some(path),
            write_error,
        }
    }

    pub fn snapshot(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    fn write(&self, state: &State) -> Result<(), String> {
        if let Some(e) = &self.write_error {
            return Err(e.clone());
        }
        let Some(path) = &self.path else {
            return Ok(());
        };
        let json = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
        // Write beside the target and rename, so a crash never leaves half a file.
        let tmp = path.with_extension("json.tmp");
        let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        file.write_all(&json).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    /// Change the state and write it; on a failed write nothing changes.
    fn edit<T>(&self, f: impl FnOnce(&mut State) -> Result<T, String>) -> Result<T, String> {
        let mut guard = self.state.lock().unwrap();
        let mut next = guard.clone();
        let out = f(&mut next)?;
        self.write(&next)?;
        *guard = next;
        Ok(out)
    }

    /// Add or replace a saved request; one without an id gets one. Returns the id.
    pub fn save_request(&self, mut r: Request) -> Result<String, String> {
        self.edit(|s| {
            r.collection = r.collection.trim().to_string();
            if r.collection.is_empty() {
                r.collection = DEFAULT_COLLECTION.to_string();
            }
            r.name = r.name.trim().to_string();
            if r.name.is_empty() {
                r.name = default_name(&r);
            }
            r.updated_ms = now_ms();
            ensure_collection(s, &r.collection);
            if let Some(slot) = s
                .requests
                .iter_mut()
                .find(|x| !r.id.is_empty() && x.id == r.id)
            {
                *slot = r.clone();
            } else {
                r.id = next_id(s);
                s.requests.push(r.clone());
            }
            Ok(r.id)
        })
    }

    pub fn remove_request(&self, id: &str) -> Result<(), String> {
        self.edit(|s| {
            let before = s.requests.len();
            s.requests.retain(|r| r.id != id);
            if s.requests.len() == before {
                return Err(format!("no saved request {id}"));
            }
            Ok(())
        })
    }

    pub fn add_collection(&self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("give the collection a name".into());
        }
        let name = normalize_path(name);
        if name.is_empty() {
            return Err("give the collection a name".into());
        }
        self.edit(|s| {
            if s.collections.iter().any(|c| *c == name) {
                return Err(format!("there is already a collection called \"{name}\""));
            }
            ensure_collection(s, &name);
            Ok(())
        })
    }

    /// Rename a collection or folder; what is inside it moves along.
    pub fn rename_collection(&self, from: &str, to: &str) -> Result<(), String> {
        let to = normalize_path(to);
        if to.is_empty() {
            return Err("give the collection a name".into());
        }
        self.edit(|s| {
            if from != to && s.collections.iter().any(|c| *c == to) {
                return Err(format!("there is already a collection called \"{to}\""));
            }
            if to.starts_with(&format!("{from}/")) {
                return Err("a folder can't move inside itself".into());
            }
            if !s.collections.iter().any(|c| c == from) {
                return Err(format!("no collection \"{from}\""));
            }
            let moved = |c: &str| -> Option<String> {
                if c == from {
                    Some(to.clone())
                } else {
                    c.strip_prefix(&format!("{from}/")).map(|rest| format!("{to}/{rest}"))
                }
            };
            for c in s.collections.iter_mut() {
                if let Some(n) = moved(c) {
                    *c = n;
                }
            }
            for r in s.requests.iter_mut() {
                if let Some(n) = moved(&r.collection) {
                    r.collection = n;
                }
            }
            for c in s.configs.iter_mut() {
                if let Some(n) = moved(&c.name) {
                    c.name = n;
                }
            }
            ensure_collection(s, &to);
            Ok(())
        })
    }

    /// Removes the collection, its folders and every request saved in them.
    pub fn remove_collection(&self, name: &str) -> Result<(), String> {
        self.edit(|s| {
            let inside = |c: &str| c == name || c.starts_with(&format!("{name}/"));
            let before = s.collections.len();
            s.collections.retain(|c| !inside(c));
            if s.collections.len() == before {
                return Err(format!("no collection \"{name}\""));
            }
            s.requests.retain(|r| !inside(&r.collection));
            s.configs.retain(|c| !inside(&c.name));
            Ok(())
        })
    }

    /// Set what a collection or folder passes down to its requests.
    pub fn set_config(&self, mut config: CollectionConfig) -> Result<(), String> {
        self.edit(|s| {
            if !s.collections.iter().any(|c| *c == config.name) {
                return Err(format!("no collection \"{}\"", config.name));
            }
            config.headers.retain(|h| !h.name.trim().is_empty());
            config.vars.retain(|v| !v.name.trim().is_empty());
            let empty = config.description.is_empty()
                && config.auth == CollectionConfig::default().auth
                && config.headers.is_empty()
                && config.vars.is_empty();
            s.configs.retain(|c| c.name != config.name);
            if !empty {
                s.configs.push(config);
            }
            Ok(())
        })
    }

    pub fn set_globals(&self, mut vars: Vec<KeyValue>) -> Result<(), String> {
        vars.retain(|v| !v.name.trim().is_empty());
        self.edit(|s| {
            s.globals = vars;
            Ok(())
        })
    }

    /// Add what a Postman import found; collections and environments that share
    /// a name with an existing one are renamed ("Shop (2)") rather than merged.
    pub fn add_imported(&self, imported: Imported) -> Result<String, String> {
        self.edit(|s| {
            let mut summary = Vec::new();
            for (collection, mut requests, configs) in imported.collections {
                let mut name = collection.clone();
                let mut n = 1;
                while s.collections.iter().any(|c| *c == name) {
                    n += 1;
                    name = format!("{collection} ({n})");
                }
                let rebase = |path: &str| -> String {
                    match path.strip_prefix(&collection) {
                        Some(rest) => format!("{name}{rest}"),
                        None => path.to_string(),
                    }
                };
                ensure_collection(s, &name);
                let count = requests.len();
                for mut r in requests.drain(..) {
                    r.collection = rebase(&r.collection);
                    ensure_collection(s, &r.collection);
                    r.id = next_id(s);
                    r.updated_ms = now_ms();
                    s.requests.push(r);
                }
                for mut c in configs {
                    c.name = rebase(&c.name);
                    ensure_collection(s, &c.name);
                    s.configs.push(c);
                }
                summary.push(format!("{name} ({count} requests)"));
            }
            for mut env in imported.environments {
                let base = env.name.clone();
                let mut n = 1;
                while s.environments.iter().any(|e| e.name == env.name) {
                    n += 1;
                    env.name = format!("{base} ({n})");
                }
                summary.push(format!("environment {}", env.name));
                s.environments.push(env);
            }
            if summary.is_empty() {
                return Err("nothing to import".into());
            }
            Ok(summary.join(", "))
        })
    }

    /// Replace every environment and pick the active one ("" for none).
    pub fn set_environments(&self, envs: Vec<Environment>, active: &str) -> Result<(), String> {
        let mut seen = std::collections::HashSet::new();
        for e in &envs {
            let n = e.name.trim();
            if n.is_empty() {
                return Err("every environment needs a name".into());
            }
            if !seen.insert(n.to_string()) {
                return Err(format!("two environments are called \"{n}\""));
            }
        }
        self.edit(|s| {
            s.environments = envs
                .into_iter()
                .map(|mut e| {
                    e.name = e.name.trim().to_string();
                    e.vars.retain(|v| !v.name.trim().is_empty());
                    e
                })
                .collect();
            s.active_environment = if s.environments.iter().any(|e| e.name == active) {
                active.to_string()
            } else {
                String::new()
            };
            Ok(())
        })
    }

    /// The request with inherited settings applied, and its variables.
    pub fn prepare(&self, r: &Request) -> (Request, Vec<KeyValue>) {
        self.state.lock().unwrap().prepare(r)
    }

    /// Note a send at the top of the history.
    pub fn record(&self, entry: HistoryEntry) {
        let res = self.edit(|s| {
            s.history.insert(0, entry);
            s.history.truncate(HISTORY_LIMIT);
            Ok(())
        });
        if let Err(e) = res {
            tracing::warn!("could not save composer history: {e}");
        }
    }

    pub fn clear_history(&self) -> Result<(), String> {
        self.edit(|s| {
            s.history.clear();
            Ok(())
        })
    }
}

fn next_id(s: &State) -> String {
    let next = s
        .requests
        .iter()
        .filter_map(|x| x.id.strip_prefix('q').and_then(|n| n.parse::<u64>().ok()))
        .max()
        .unwrap_or(0)
        + 1;
    format!("q{next}")
}

/// Add a collection path and every parent folder that is not there yet.
fn ensure_collection(s: &mut State, path: &str) {
    for name in collection_chain(path) {
        if !s.collections.contains(&name) {
            s.collections.push(name);
        }
    }
}

/// "GET /users" from the URL, for requests saved without a name.
pub fn default_name(r: &Request) -> String {
    let method = if r.method.trim().is_empty() {
        "GET".to_string()
    } else {
        r.method.trim().to_ascii_uppercase()
    };
    let url = r.url.trim();
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let path = rest.find('/').map(|i| &rest[i..]).unwrap_or("/");
    let path = path.split(['?', '#']).next().unwrap_or("/");
    format!("{method} {path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(url: &str) -> Request {
        Request {
            method: "GET".into(),
            url: url.into(),
            ..Default::default()
        }
    }

    #[test]
    fn preserves_previous_bad_files() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["composer.json.bad", "composer.json.bad.1"] {
            std::fs::write(dir.path().join(name), name).unwrap();
        }
        std::fs::write(dir.path().join("composer.json"), b"broken").unwrap();
        let c = Composer::load(dir.path());
        assert_eq!(
            std::fs::read(dir.path().join("composer.json.bad.2")).unwrap(),
            b"broken"
        );
        for name in ["composer.json.bad", "composer.json.bad.1"] {
            assert_eq!(
                std::fs::read_to_string(dir.path().join(name)).unwrap(),
                name
            );
        }
        c.save_request(req("https://x.dev/")).unwrap();
    }

    #[test]
    fn failed_preservation_disables_writes() {
        let dir = tempfile::tempdir().unwrap();
        // A path through a file cannot be read or renamed.
        std::fs::write(dir.path().join("blocked"), b"keep").unwrap();
        let c = Composer::load(&dir.path().join("blocked"));
        assert!(c
            .save_request(req("https://x.dev/"))
            .unwrap_err()
            .contains("could not be preserved"));
        assert!(c.snapshot().requests.is_empty());
        assert_eq!(std::fs::read(dir.path().join("blocked")).unwrap(), b"keep");
    }

    #[test]
    fn writes_replace_the_file_and_leave_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let c = Composer::load(dir.path());
        c.add_collection("first").unwrap();
        c.add_collection("second").unwrap();
        assert_eq!(
            Composer::load(dir.path()).snapshot().collections,
            vec!["first", "second"]
        );
        assert!(!dir.path().join("composer.json.tmp").exists());
    }

    #[test]
    fn saves_names_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let c = Composer::load(dir.path());
        let id = c
            .save_request(req("https://api.example.com/v1/users?x=1"))
            .unwrap();
        assert_eq!(id, "q1");
        let s = c.snapshot();
        assert_eq!(s.collections, vec![DEFAULT_COLLECTION.to_string()]);
        assert_eq!(s.requests[0].name, "GET /v1/users");

        // Saving with the same id replaces.
        let mut r = s.requests[0].clone();
        r.name = "List users".into();
        assert_eq!(c.save_request(r).unwrap(), "q1");
        assert_eq!(c.snapshot().requests.len(), 1);

        let again = Composer::load(dir.path());
        assert_eq!(again.snapshot().requests[0].name, "List users");
    }

    #[test]
    fn collections_rename_and_remove_with_their_requests() {
        let c = Composer::in_memory();
        c.add_collection("Shop").unwrap();
        assert!(c.add_collection("Shop").is_err());
        let mut r = req("http://x/");
        r.collection = "Shop".into();
        c.save_request(r).unwrap();
        c.rename_collection("Shop", "Store").unwrap();
        let s = c.snapshot();
        assert_eq!(s.collections, vec!["Store".to_string()]);
        assert_eq!(s.requests[0].collection, "Store");
        c.remove_collection("Store").unwrap();
        assert!(c.snapshot().requests.is_empty());
    }

    #[test]
    fn environments_pick_active_variables() {
        let c = Composer::in_memory();
        let mut off = KeyValue::new("off", "1");
        off.disabled = true;
        c.set_environments(
            vec![Environment {
                name: "staging".into(),
                vars: vec![
                    KeyValue::new("host", "staging.example.com"),
                    off,
                    KeyValue::new(" ", ""),
                ],
            }],
            "staging",
        )
        .unwrap();
        assert_eq!(
            c.prepare(&Request::default()).1,
            vec![KeyValue::new("host", "staging.example.com")]
        );
        c.set_environments(c.snapshot().environments, "gone")
            .unwrap();
        assert!(c.prepare(&Request::default()).1.is_empty());
        assert!(c
            .set_environments(vec![Environment::default()], "")
            .is_err());
    }

    #[test]
    fn history_is_capped_newest_first() {
        let c = Composer::in_memory();
        for i in 0..(HISTORY_LIMIT as u64 + 5) {
            c.record(HistoryEntry {
                flow_id: i,
                ..Default::default()
            });
        }
        let h = c.snapshot().history;
        assert_eq!(h.len(), HISTORY_LIMIT);
        assert_eq!(h[0].flow_id, HISTORY_LIMIT as u64 + 4);
    }

    #[test]
    fn a_corrupt_file_is_kept_aside() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("composer.json"), b"{nope").unwrap();
        let c = Composer::load(dir.path());
        assert!(c.snapshot().requests.is_empty());
        assert!(dir.path().join("composer.json.bad").exists());
    }

    #[test]
    fn folders_inherit_auth_headers_and_variables() {
        let c = Composer::in_memory();
        c.add_collection("Shop/Orders").unwrap();
        c.set_config(CollectionConfig {
            name: "Shop".into(),
            auth: Auth {
                kind: AuthKind::Bearer,
                token: "{{t}}".into(),
                ..Default::default()
            },
            headers: vec![KeyValue::new("X-App", "neko"), KeyValue::new("X-Over", "shop")],
            vars: vec![KeyValue::new("t", "from-shop"), KeyValue::new("only", "shop")],
            ..Default::default()
        })
        .unwrap();
        c.set_config(CollectionConfig {
            name: "Shop/Orders".into(),
            vars: vec![KeyValue::new("t", "from-orders")],
            ..Default::default()
        })
        .unwrap();
        c.set_globals(vec![KeyValue::new("t", "global"), KeyValue::new("g", "1")])
            .unwrap();
        let r = Request {
            collection: "Shop/Orders".into(),
            auth: AuthKind::Inherit,
            headers: vec![KeyValue::new("x-over", "mine")],
            ..Default::default()
        };
        let (eff, vars) = c.prepare(&r);
        assert_eq!(eff.auth, AuthKind::Bearer);
        assert_eq!(eff.headers.len(), 2, "{:?}", eff.headers);
        assert_eq!(eff.headers[0], KeyValue::new("X-App", "neko"));
        assert_eq!(eff.headers[1].value, "mine");
        // The deepest folder wins over its parent and the globals.
        let value = |n: &str| vars.iter().rev().find(|v| v.name == n).map(|v| v.value.clone());
        assert_eq!(value("t").as_deref(), Some("from-orders"));
        assert_eq!(value("only").as_deref(), Some("shop"));
        assert_eq!(value("g").as_deref(), Some("1"));

        // A request outside any collection gets nothing.
        let (eff, _) = c.prepare(&Request {
            auth: AuthKind::Inherit,
            ..Default::default()
        });
        assert_eq!(eff.auth, AuthKind::None);
    }

    #[test]
    fn nested_collections_rename_and_remove_together() {
        let c = Composer::in_memory();
        let mut r = req("http://x/");
        r.collection = "A/B/C".into();
        c.save_request(r).unwrap();
        assert_eq!(c.snapshot().collections, vec!["A", "A/B", "A/B/C"]);
        c.rename_collection("A", "Z").unwrap();
        let s = c.snapshot();
        assert_eq!(s.collections, vec!["Z", "Z/B", "Z/B/C"]);
        assert_eq!(s.requests[0].collection, "Z/B/C");
        assert!(c.rename_collection("Z", "Z/B/Q").is_err());
        c.remove_collection("Z/B").unwrap();
        let s = c.snapshot();
        assert_eq!(s.collections, vec!["Z"]);
        assert!(s.requests.is_empty());
    }
}
