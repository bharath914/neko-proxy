//! Rules beyond Map Local: mock, map remote, rewrite, throttle and breakpoint.
//!
//! One rule is a [`Match`] (which requests) and an [`Action`] (what happens).
//! They are plain data so they can be edited by hand, kept in git, and created
//! by the desktop app or an AI client through the same API.

use crate::pattern::host_matches;
use serde::{Deserialize, Serialize};

/// Largest inline mock body.
pub const MAX_MOCK_BYTES: usize = 4 * 1024 * 1024;
/// Longest artificial delay.
pub const MAX_DELAY_MS: u32 = 60_000;
/// Breakpoints give up and let the message through after this many seconds
/// unless the rule says otherwise.
pub const DEFAULT_BREAKPOINT_TIMEOUT_S: u32 = 60;
pub const MAX_BREAKPOINT_TIMEOUT_S: u32 = 3600;

/// Which requests a rule applies to. Every field narrows; empty means any.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Match {
    /// `http`, `https` or empty for both.
    #[serde(default)]
    pub scheme: String,
    /// Host pattern; `*` matches any run of characters.
    pub host: String,
    /// 0 matches any port.
    #[serde(default)]
    pub port: u16,
    /// Empty matches any method.
    #[serde(default)]
    pub method: String,
    /// Exact path, or a prefix when it ends with `*`. Empty or `*` matches any.
    #[serde(default)]
    pub path: String,
    /// Exact query string (without `?`). Empty matches any query.
    #[serde(default)]
    pub query: String,
}

impl Match {
    pub fn matches(
        &self,
        scheme: &str,
        host: &str,
        port: u16,
        method: &str,
        path: &str,
        query: &str,
    ) -> bool {
        if !self.scheme.is_empty() && !self.scheme.eq_ignore_ascii_case(scheme) {
            return false;
        }
        if !host_matches(&self.host, host) {
            return false;
        }
        if self.port != 0 && self.port != port {
            return false;
        }
        if !self.method.is_empty() && !self.method.eq_ignore_ascii_case(method) {
            return false;
        }
        let path_ok = match self.path.as_str() {
            "" | "*" => true,
            p => match p.strip_suffix('*') {
                Some(prefix) => path.starts_with(prefix),
                None => p == path,
            },
        };
        path_ok && (self.query.is_empty() || self.query == query)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeaderOp {
    /// Replace any existing values with this one.
    Set,
    /// Add another value, keeping existing ones.
    Add,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderEdit {
    pub op: HeaderOp,
    pub name: String,
    #[serde(default)]
    pub value: String,
}

/// When a breakpoint holds a message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakpointPhase {
    Request,
    Response,
    #[default]
    Both,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Answer with a fixed response; the server is never contacted.
    Mock {
        #[serde(default = "ok_status")]
        status: u16,
        #[serde(default)]
        headers: Vec<(String, String)>,
        #[serde(default)]
        body: String,
        #[serde(default)]
        delay_ms: u32,
    },
    /// Send the request to another host, port or scheme. The flow list still
    /// shows the URL the client asked for.
    MapRemote {
        /// `http`, `https` or empty to keep.
        #[serde(default)]
        scheme: String,
        /// Empty keeps the original host.
        #[serde(default)]
        host: String,
        /// 0 keeps the original port.
        #[serde(default)]
        port: u16,
    },
    /// Edit headers on the way out, on the way back, or both.
    Rewrite {
        #[serde(default)]
        request_headers: Vec<HeaderEdit>,
        #[serde(default)]
        response_headers: Vec<HeaderEdit>,
    },
    /// Add latency before the request goes upstream.
    Throttle { delay_ms: u32 },
    /// Deterministic every-N failure plus bounded delay/jitter and flaky 502s.
    Scenario {
        #[serde(default)]
        fail_one_in: u32,
        #[serde(default)]
        delay_ms: u32,
        #[serde(default)]
        jitter_ms: u32,
        #[serde(default)]
        flaky_one_in: u32,
    },
    /// Hold the request, the response, or both until someone resumes it.
    Breakpoint {
        #[serde(default)]
        phase: BreakpointPhase,
        /// 0 uses the default.
        #[serde(default)]
        timeout_s: u32,
    },
}

fn ok_status() -> u16 {
    200
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(rename = "match")]
    pub matcher: Match,
    pub action: Action,
}

impl Rule {
    /// Short word for the kind of rule.
    pub fn kind(&self) -> &'static str {
        match self.action {
            Action::Mock { .. } => "mock",
            Action::MapRemote { .. } => "map_remote",
            Action::Rewrite { .. } => "rewrite",
            Action::Throttle { .. } => "throttle",
            Action::Scenario { .. } => "scenario",
            Action::Breakpoint { .. } => "breakpoint",
        }
    }

    /// Normalize in place and report what is wrong, if anything.
    pub fn validate(&mut self) -> Result<(), String> {
        let m = &mut self.matcher;
        m.host = m.host.trim().to_ascii_lowercase();
        if m.host.is_empty() {
            return Err("a rule needs a host pattern (use * for any host)".into());
        }
        if m.host.len() > 253 || m.host.chars().any(|c| c.is_whitespace() || c == '/') {
            return Err(format!("\"{}\" is not a host pattern", m.host));
        }
        m.scheme = m.scheme.trim().to_ascii_lowercase();
        if !matches!(m.scheme.as_str(), "" | "http" | "https") {
            return Err("scheme must be http, https or empty".into());
        }
        m.method = m.method.trim().to_ascii_uppercase();
        if !m.method.is_empty() && http::Method::from_bytes(m.method.as_bytes()).is_err() {
            return Err(format!("\"{}\" is not an HTTP method", m.method));
        }
        m.path = m.path.trim().to_string();
        if !m.path.is_empty() && m.path != "*" && !m.path.starts_with('/') {
            return Err("the path must start with / (or be * for any path)".into());
        }
        self.name = self.name.trim().to_string();

        match &mut self.action {
            Action::Mock {
                status,
                headers,
                body,
                delay_ms,
            } => {
                if !(100..=599).contains(status) {
                    return Err("status must be between 100 and 599".into());
                }
                if body.len() > MAX_MOCK_BYTES {
                    return Err("mock body is larger than 4 MiB; use Map Local".into());
                }
                if *delay_ms > MAX_DELAY_MS {
                    return Err("delay is limited to 60 seconds".into());
                }
                check_headers(headers.iter().map(|(n, v)| (n.as_str(), v.as_str())))?;
            }
            Action::MapRemote { scheme, host, port } => {
                *scheme = scheme.trim().to_ascii_lowercase();
                if !matches!(scheme.as_str(), "" | "http" | "https") {
                    return Err("scheme must be http, https or empty".into());
                }
                *host = host.trim().to_ascii_lowercase();
                if host.len() > 253
                    || host
                        .chars()
                        .any(|c| c.is_whitespace() || c == '/' || c == '@' || c == '*')
                {
                    return Err(format!("\"{host}\" is not a host name"));
                }
                if host.is_empty() && *port == 0 && scheme.is_empty() {
                    return Err("map remote needs a host, a port or a scheme".into());
                }
            }
            Action::Rewrite {
                request_headers,
                response_headers,
            } => {
                if request_headers.is_empty() && response_headers.is_empty() {
                    return Err("a rewrite rule needs at least one header edit".into());
                }
                for e in request_headers.iter().chain(response_headers.iter()) {
                    check_headers(std::iter::once((e.name.as_str(), e.value.as_str())))?;
                }
            }
            Action::Scenario {
                fail_one_in,
                delay_ms,
                jitter_ms,
                flaky_one_in,
            } => {
                if delay_ms.saturating_add(*jitter_ms) > MAX_DELAY_MS {
                    return Err("delay plus jitter is limited to 60 seconds".into());
                }
                if *fail_one_in == 0 && *delay_ms == 0 && *jitter_ms == 0 && *flaky_one_in == 0 {
                    return Err("scenario needs a failure rate or delay".into());
                }
            }
            Action::Throttle { delay_ms } => {
                if *delay_ms == 0 || *delay_ms > MAX_DELAY_MS {
                    return Err("delay must be between 1 ms and 60 seconds".into());
                }
            }
            Action::Breakpoint { timeout_s, .. } => {
                if *timeout_s > MAX_BREAKPOINT_TIMEOUT_S {
                    return Err("timeout is limited to one hour".into());
                }
            }
        }
        Ok(())
    }
}

fn check_headers<'a>(it: impl Iterator<Item = (&'a str, &'a str)>) -> Result<(), String> {
    for (n, v) in it {
        http::HeaderName::from_bytes(n.as_bytes())
            .map_err(|_| format!("\"{n}\" is not a valid header name"))?;
        http::HeaderValue::from_str(v)
            .map_err(|_| format!("the value of header {n} has characters HTTP does not allow"))?;
    }
    Ok(())
}

/// Apply header edits to a header map. Invalid names or values are skipped;
/// `validate` already rejected them when the rule was saved.
pub fn apply_header_edits(headers: &mut http::HeaderMap, edits: &[HeaderEdit]) -> bool {
    let mut changed = false;
    for e in edits {
        let Ok(name) = http::HeaderName::from_bytes(e.name.as_bytes()) else {
            continue;
        };
        match e.op {
            HeaderOp::Remove => {
                changed |= headers.remove(&name).is_some();
            }
            HeaderOp::Set | HeaderOp::Add => {
                let Ok(value) = http::HeaderValue::from_str(&e.value) else {
                    continue;
                };
                if e.op == HeaderOp::Set {
                    headers.insert(name, value);
                } else {
                    headers.append(name, value);
                }
                changed = true;
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(host: &str) -> Match {
        Match {
            scheme: String::new(),
            host: host.into(),
            port: 0,
            method: String::new(),
            path: String::new(),
            query: String::new(),
        }
    }

    #[test]
    fn matching_narrows_by_every_field() {
        let mut x = m("*.example.com");
        assert!(x.matches("https", "api.example.com", 443, "GET", "/a", ""));
        assert!(!x.matches("https", "example.com", 443, "GET", "/a", ""));
        x.method = "post".into();
        x.path = "/v1/*".into();
        assert!(!x.matches("https", "api.example.com", 443, "GET", "/v1/a", ""));
        assert!(x.matches("https", "api.example.com", 443, "POST", "/v1/a", ""));
        assert!(!x.matches("https", "api.example.com", 443, "POST", "/v2/a", ""));
        x.port = 8443;
        assert!(!x.matches("https", "api.example.com", 443, "POST", "/v1/a", ""));
        x.port = 0;
        x.scheme = "http".into();
        assert!(!x.matches("https", "api.example.com", 443, "POST", "/v1/a", ""));
        x.scheme.clear();
        x.query = "a=1".into();
        assert!(!x.matches("https", "api.example.com", 443, "POST", "/v1/a", "a=2"));
        assert!(x.matches("https", "api.example.com", 443, "POST", "/v1/a", "a=1"));
    }

    #[test]
    fn validation_normalizes_and_rejects() {
        let mut r = Rule {
            id: String::new(),
            name: " login ".into(),
            enabled: true,
            matcher: Match {
                method: "post".into(),
                ..m(" API.Example.com ")
            },
            action: Action::Mock {
                status: 401,
                headers: vec![("content-type".into(), "application/json".into())],
                body: "{}".into(),
                delay_ms: 0,
            },
        };
        r.validate().unwrap();
        assert_eq!(r.matcher.host, "api.example.com");
        assert_eq!(r.matcher.method, "POST");
        assert_eq!(r.name, "login");

        r.matcher.path = "v1".into();
        assert!(r.validate().is_err());
        r.matcher.path = "/v1".into();
        r.action = Action::Mock {
            status: 99,
            headers: vec![],
            body: String::new(),
            delay_ms: 0,
        };
        assert!(r.validate().is_err());
        r.action = Action::Rewrite {
            request_headers: vec![HeaderEdit {
                op: HeaderOp::Set,
                name: "bad name".into(),
                value: "x".into(),
            }],
            response_headers: vec![],
        };
        assert!(r.validate().is_err());
        r.action = Action::MapRemote {
            scheme: String::new(),
            host: String::new(),
            port: 0,
        };
        assert!(r.validate().is_err());
    }

    #[test]
    fn header_edits() {
        let mut h = http::HeaderMap::new();
        h.insert("x-a", "1".parse().unwrap());
        h.append("x-b", "1".parse().unwrap());
        let changed = apply_header_edits(
            &mut h,
            &[
                HeaderEdit {
                    op: HeaderOp::Set,
                    name: "x-a".into(),
                    value: "2".into(),
                },
                HeaderEdit {
                    op: HeaderOp::Add,
                    name: "x-b".into(),
                    value: "2".into(),
                },
                HeaderEdit {
                    op: HeaderOp::Remove,
                    name: "x-none".into(),
                    value: String::new(),
                },
            ],
        );
        assert!(changed);
        assert_eq!(h["x-a"], "2");
        assert_eq!(h.get_all("x-b").iter().count(), 2);
    }

    #[test]
    fn json_shape_is_stable() {
        let r: Rule = serde_json::from_str(
            r#"{"name":"m","match":{"host":"a.test","path":"/x"},
                "action":{"type":"mock","status":503,"body":"down"}}"#,
        )
        .unwrap();
        assert!(r.enabled);
        assert!(matches!(r.action, Action::Mock { status: 503, .. }));
    }
}
