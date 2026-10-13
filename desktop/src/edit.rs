//! Form state for the breakpoint editor and the rule editor. Plain data and
//! small conversions to and from the wire types, so the screens stay thin and
//! this can be tested without a window. Validation of the rules themselves
//! happens in the core; this only turns text into fields.

use proto::pb;

// ------------------------------------------------------------ headers as text

pub fn headers_to_text(h: &[pb::Header]) -> String {
    h.iter()
        .map(|h| format!("{}: {}", h.name, h.value))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `Name: value` per line. Blank lines and lines without a colon are skipped.
pub fn text_to_headers(t: &str) -> Vec<pb::Header> {
    t.lines()
        .filter_map(|l| {
            let (n, v) = l.split_once(':')?;
            let n = n.trim();
            if n.is_empty() {
                return None;
            }
            Some(pb::Header {
                name: n.to_string(),
                value: v.trim().to_string(),
            })
        })
        .collect()
}

// ------------------------------------------------------------ breakpoint editor

/// What the person is editing while a message is held.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BpEdit {
    pub flow_id: u64,
    pub request: bool,
    pub method: String,
    pub path: String,
    pub status: String,
    pub headers: String,
    pub body: String,
    /// The body is not text; it is passed through untouched.
    pub binary: bool,
}

impl BpEdit {
    pub fn new(p: &pb::PausedFlow) -> BpEdit {
        let (body, binary) = match std::str::from_utf8(&p.body) {
            Ok(t) => (t.to_string(), false),
            Err(_) => (String::new(), true),
        };
        BpEdit {
            flow_id: p.flow_id,
            request: p.phase == pb::MessagePhase::PhaseRequest as i32,
            method: p.method.clone(),
            path: p.path.clone(),
            status: if p.status == 0 {
                String::new()
            } else {
                p.status.to_string()
            },
            headers: headers_to_text(&p.headers),
            body,
            binary,
        }
    }

    /// Only the fields that differ from what was held.
    pub fn resume(&self, orig: &pb::PausedFlow) -> pb::ResumeRequest {
        let mut r = pb::ResumeRequest {
            flow_id: self.flow_id,
            ..Default::default()
        };
        if self.request {
            if self.method.trim() != orig.method {
                r.method = Some(self.method.trim().to_string());
            }
            if self.path.trim() != orig.path {
                r.path = Some(self.path.trim().to_string());
            }
        } else if let Ok(s) = self.status.trim().parse::<u32>() {
            if s != orig.status {
                r.status = Some(s);
            }
        }
        let headers = text_to_headers(&self.headers);
        if headers != orig.headers {
            r.replace_headers = true;
            r.headers = headers;
        }
        if !self.binary && self.body.as_bytes() != &orig.body[..] {
            r.body = Some(self.body.clone().into_bytes());
        }
        r
    }

    pub fn changed(&self, orig: &pb::PausedFlow) -> bool {
        let r = self.resume(orig);
        r.method.is_some()
            || r.path.is_some()
            || r.status.is_some()
            || r.replace_headers
            || r.body.is_some()
    }
}

// ------------------------------------------------------------ rule editor

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleKind {
    Mock,
    MapRemote,
    Rewrite,
    Throttle,
    Breakpoint,
}

impl RuleKind {
    pub const ALL: [RuleKind; 5] = [
        RuleKind::Mock,
        RuleKind::MapRemote,
        RuleKind::Rewrite,
        RuleKind::Throttle,
        RuleKind::Breakpoint,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RuleKind::Mock => "Mock",
            RuleKind::MapRemote => "Map remote",
            RuleKind::Rewrite => "Rewrite",
            RuleKind::Throttle => "Throttle",
            RuleKind::Breakpoint => "Breakpoint",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            RuleKind::Mock => "Answer with a fixed response. The server is never contacted.",
            RuleKind::MapRemote => "Send the request to another host, port or scheme.",
            RuleKind::Rewrite => "Add, set or remove headers on the way out or back.",
            RuleKind::Throttle => "Delay the request before it goes upstream.",
            RuleKind::Breakpoint => "Hold the request or response until you resume it.",
        }
    }

    pub fn of(r: &pb::Rule) -> Option<RuleKind> {
        use pb::rule::Action as A;
        Some(match r.action.as_ref()? {
            A::Mock(_) => RuleKind::Mock,
            A::MapRemote(_) => RuleKind::MapRemote,
            A::Rewrite(_) => RuleKind::Rewrite,
            A::Throttle(_) => RuleKind::Throttle,
            A::Breakpoint(_) => RuleKind::Breakpoint,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleForm {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub kind: RuleKind,
    // match
    pub scheme: String,
    pub host: String,
    pub port: String,
    pub method: String,
    pub path: String,
    pub query: String,
    // mock
    pub status: String,
    pub headers: String,
    pub body: String,
    pub delay_ms: String,
    // map remote
    pub to_scheme: String,
    pub to_host: String,
    pub to_port: String,
    // rewrite: one edit per line, `set Name: value`, `add Name: value`, `remove Name`
    pub request_edits: String,
    pub response_edits: String,
    // breakpoint: 0 both, 1 request, 2 response
    pub phase: usize,
    pub timeout_s: String,
}

impl RuleForm {
    pub fn new(kind: RuleKind) -> RuleForm {
        RuleForm {
            id: String::new(),
            name: String::new(),
            enabled: true,
            kind,
            scheme: String::new(),
            host: String::new(),
            port: String::new(),
            method: String::new(),
            path: String::new(),
            query: String::new(),
            status: "200".into(),
            headers: "content-type: application/json".into(),
            body: String::new(),
            delay_ms: String::new(),
            to_scheme: String::new(),
            to_host: String::new(),
            to_port: String::new(),
            request_edits: String::new(),
            response_edits: String::new(),
            phase: 0,
            timeout_s: String::new(),
        }
    }

    /// A form pre-filled to match a flow's request.
    pub fn for_flow(kind: RuleKind, f: &pb::FlowSummary) -> RuleForm {
        let mut r = RuleForm::new(kind);
        r.scheme = f.scheme.clone();
        r.host = f.host.clone();
        r.method = f.method.clone();
        r.path = f.path.split('?').next().unwrap_or("").to_string();
        let default = if f.scheme == "https" { 443 } else { 80 };
        if f.port != 0 && f.port != default {
            r.port = f.port.to_string();
        }
        r
    }

    pub fn from_rule(r: &pb::Rule) -> RuleForm {
        use pb::rule::Action as A;
        let kind = RuleKind::of(r).unwrap_or(RuleKind::Mock);
        let mut f = RuleForm::new(kind);
        f.id = r.id.clone();
        f.name = r.name.clone();
        f.enabled = r.enabled;
        if let Some(m) = &r.matcher {
            f.scheme = m.scheme.clone();
            f.host = m.host.clone();
            f.port = if m.port == 0 { String::new() } else { m.port.to_string() };
            f.method = m.method.clone();
            f.path = m.path.clone();
            f.query = m.query.clone();
        }
        match r.action.as_ref() {
            Some(A::Mock(a)) => {
                f.status = a.status.to_string();
                f.headers = headers_to_text(&a.headers);
                f.body = a.body.clone();
                f.delay_ms = if a.delay_ms == 0 { String::new() } else { a.delay_ms.to_string() };
            }
            Some(A::MapRemote(a)) => {
                f.to_scheme = a.scheme.clone();
                f.to_host = a.host.clone();
                f.to_port = if a.port == 0 { String::new() } else { a.port.to_string() };
            }
            Some(A::Rewrite(a)) => {
                f.request_edits = edits_to_text(&a.request_headers);
                f.response_edits = edits_to_text(&a.response_headers);
            }
            Some(A::Throttle(a)) => f.delay_ms = a.delay_ms.to_string(),
            Some(A::Breakpoint(a)) => {
                f.phase = a.phase.clamp(0, 2) as usize;
                f.timeout_s = if a.timeout_s == 0 { String::new() } else { a.timeout_s.to_string() };
            }
            None => {}
        }
        f
    }

    pub fn to_rule(&self) -> Result<pb::Rule, String> {
        use pb::rule::Action as A;
        let num = |s: &str, what: &str| -> Result<u32, String> {
            let s = s.trim();
            if s.is_empty() {
                return Ok(0);
            }
            s.parse::<u32>().map_err(|_| format!("{what} must be a number"))
        };
        if self.host.trim().is_empty() {
            return Err("Enter a host (use * for any host)".into());
        }
        let action = match self.kind {
            RuleKind::Mock => A::Mock(pb::MockAction {
                status: num(&self.status, "Status")?,
                headers: text_to_headers(&self.headers),
                body: self.body.clone(),
                delay_ms: num(&self.delay_ms, "Delay")?,
            }),
            RuleKind::MapRemote => A::MapRemote(pb::MapRemoteAction {
                scheme: self.to_scheme.trim().to_string(),
                host: self.to_host.trim().to_string(),
                port: num(&self.to_port, "Port")?,
            }),
            RuleKind::Rewrite => A::Rewrite(pb::RewriteAction {
                request_headers: text_to_edits(&self.request_edits)?,
                response_headers: text_to_edits(&self.response_edits)?,
            }),
            RuleKind::Throttle => A::Throttle(pb::ThrottleAction {
                delay_ms: num(&self.delay_ms, "Delay")?,
            }),
            RuleKind::Breakpoint => A::Breakpoint(pb::BreakpointAction {
                phase: self.phase as i32,
                timeout_s: num(&self.timeout_s, "Timeout")?,
            }),
        };
        Ok(pb::Rule {
            id: self.id.clone(),
            name: self.name.trim().to_string(),
            enabled: self.enabled,
            matcher: Some(pb::RuleMatch {
                scheme: self.scheme.trim().to_string(),
                host: self.host.trim().to_string(),
                port: num(&self.port, "Port")?,
                method: self.method.trim().to_string(),
                path: self.path.trim().to_string(),
                query: self.query.trim().to_string(),
            }),
            action: Some(action),
        })
    }
}

fn edits_to_text(e: &[pb::HeaderEdit]) -> String {
    e.iter()
        .map(|e| match pb::header_edit::Op::try_from(e.op).unwrap_or(pb::header_edit::Op::Set) {
            pb::header_edit::Op::Set => format!("set {}: {}", e.name, e.value),
            pb::header_edit::Op::Add => format!("add {}: {}", e.name, e.value),
            pb::header_edit::Op::Remove => format!("remove {}", e.name),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn text_to_edits(t: &str) -> Result<Vec<pb::HeaderEdit>, String> {
    let mut out = Vec::new();
    for (i, line) in t.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let bad = || format!("Line {}: use `set Name: value`, `add Name: value` or `remove Name`", i + 1);
        let (verb, rest) = line.split_once(char::is_whitespace).ok_or_else(bad)?;
        let rest = rest.trim();
        let edit = match verb.to_ascii_lowercase().as_str() {
            "remove" | "delete" => pb::HeaderEdit {
                op: pb::header_edit::Op::Remove as i32,
                name: rest.to_string(),
                value: String::new(),
            },
            v @ ("set" | "add") => {
                let (n, val) = rest.split_once(':').ok_or_else(bad)?;
                pb::HeaderEdit {
                    op: if v == "set" {
                        pb::header_edit::Op::Set
                    } else {
                        pb::header_edit::Op::Add
                    } as i32,
                    name: n.trim().to_string(),
                    value: val.trim().to_string(),
                }
            }
            _ => return Err(bad()),
        };
        if edit.name.is_empty() {
            return Err(bad());
        }
        out.push(edit);
    }
    Ok(out)
}

/// One-line description of a rule's target, for lists and prompts.
pub fn describe_match(m: &pb::RuleMatch) -> String {
    let mut s = String::new();
    if !m.method.is_empty() {
        s.push_str(&m.method);
        s.push(' ');
    }
    s.push_str(&m.host);
    if m.port != 0 {
        s.push_str(&format!(":{}", m.port));
    }
    s.push_str(if m.path.is_empty() { "/*" } else { &m.path });
    if !m.query.is_empty() {
        s.push('?');
        s.push_str(&m.query);
    }
    s
}

/// One-line description of what a rule does.
pub fn describe_action(r: &pb::Rule) -> String {
    use pb::rule::Action as A;
    match r.action.as_ref() {
        Some(A::Mock(a)) => {
            let delay = if a.delay_ms > 0 {
                format!(", after {} ms", a.delay_ms)
            } else {
                String::new()
            };
            format!("Answer {}{delay}", a.status)
        }
        Some(A::MapRemote(a)) => {
            let mut to = String::new();
            if !a.scheme.is_empty() {
                to.push_str(&a.scheme);
                to.push_str("://");
            }
            to.push_str(if a.host.is_empty() { "(same host)" } else { &a.host });
            if a.port != 0 {
                to.push_str(&format!(":{}", a.port));
            }
            format!("Send to {to}")
        }
        Some(A::Rewrite(a)) => {
            let n = a.request_headers.len() + a.response_headers.len();
            format!("Edit {n} header{}", if n == 1 { "" } else { "s" })
        }
        Some(A::Throttle(a)) => format!("Delay {} ms", a.delay_ms),
        Some(A::Breakpoint(a)) => match pb::breakpoint_action::Phase::try_from(a.phase)
            .unwrap_or(pb::breakpoint_action::Phase::Both)
        {
            pb::breakpoint_action::Phase::Both => "Hold request and response".into(),
            pb::breakpoint_action::Phase::Request => "Hold request".into(),
            pb::breakpoint_action::Phase::Response => "Hold response".into(),
        },
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paused(request: bool) -> pb::PausedFlow {
        pb::PausedFlow {
            flow_id: 7,
            phase: if request {
                pb::MessagePhase::PhaseRequest
            } else {
                pb::MessagePhase::PhaseResponse
            } as i32,
            method: "POST".into(),
            path: "/v1/login".into(),
            status: if request { 0 } else { 200 },
            headers: vec![pb::Header {
                name: "content-type".into(),
                value: "application/json".into(),
            }],
            body: br#"{"a":1}"#.to_vec(),
            ..Default::default()
        }
    }

    #[test]
    fn untouched_edit_changes_nothing() {
        let p = paused(true);
        let e = BpEdit::new(&p);
        assert!(!e.changed(&p));
        let r = e.resume(&p);
        assert!(r.method.is_none() && r.path.is_none() && r.body.is_none() && !r.replace_headers);
    }

    #[test]
    fn only_changed_fields_are_sent() {
        let p = paused(true);
        let mut e = BpEdit::new(&p);
        e.path = "/v1/other?x=1".into();
        e.body = r#"{"a":2}"#.into();
        e.headers.push_str("\nx-extra: 1");
        let r = e.resume(&p);
        assert_eq!(r.path.as_deref(), Some("/v1/other?x=1"));
        assert!(r.method.is_none());
        assert_eq!(r.body.as_deref(), Some(&br#"{"a":2}"#[..]));
        assert!(r.replace_headers);
        assert_eq!(r.headers.len(), 2);
        assert!(e.changed(&p));
    }

    #[test]
    fn response_status_and_binary_bodies() {
        let mut p = paused(false);
        p.body = vec![0xff, 0xfe, 0x00];
        let mut e = BpEdit::new(&p);
        assert!(e.binary);
        e.status = "503".into();
        let r = e.resume(&p);
        assert_eq!(r.status, Some(503));
        assert!(r.body.is_none(), "binary bodies pass through");
    }

    #[test]
    fn header_text_round_trips() {
        let h = text_to_headers("Accept: */*\n\nbad line\nX-Url: http://a:80/b\n: nope");
        assert_eq!(h.len(), 2);
        assert_eq!(h[1].value, "http://a:80/b");
        assert_eq!(headers_to_text(&h), "Accept: */*\nX-Url: http://a:80/b");
    }

    #[test]
    fn rule_form_builds_and_reads_back() {
        let mut f = RuleForm::new(RuleKind::Mock);
        assert!(f.to_rule().is_err(), "host is required");
        f.host = "api.example.com".into();
        f.path = "/v1/*".into();
        f.status = "503".into();
        f.delay_ms = "250".into();
        f.body = "down".into();
        let r = f.to_rule().unwrap();
        assert_eq!(describe_action(&r), "Answer 503, after 250 ms");
        assert_eq!(describe_match(r.matcher.as_ref().unwrap()), "api.example.com/v1/*");
        let back = RuleForm::from_rule(&r);
        assert_eq!(back.kind, RuleKind::Mock);
        assert_eq!(back.status, "503");
        assert_eq!(back.body, "down");

        f.status = "abc".into();
        assert!(f.to_rule().unwrap_err().contains("Status"));
    }

    #[test]
    fn rewrite_edits_parse() {
        let mut f = RuleForm::new(RuleKind::Rewrite);
        f.host = "*".into();
        f.request_edits = "set X-Env: staging\nremove Cookie\nadd Accept: text/html".into();
        let r = f.to_rule().unwrap();
        let Some(pb::rule::Action::Rewrite(a)) = &r.action else {
            panic!()
        };
        assert_eq!(a.request_headers.len(), 3);
        assert_eq!(edits_to_text(&a.request_headers), f.request_edits);
        f.request_edits = "frobnicate X".into();
        assert!(f.to_rule().is_err());
    }

    #[test]
    fn form_for_a_flow_is_prefilled() {
        let f = RuleForm::for_flow(
            RuleKind::Breakpoint,
            &pb::FlowSummary {
                scheme: "https".into(),
                host: "api.example.com".into(),
                port: 8443,
                method: "GET".into(),
                path: "/v1/me?x=1".into(),
                ..Default::default()
            },
        );
        assert_eq!(f.host, "api.example.com");
        assert_eq!(f.path, "/v1/me");
        assert_eq!(f.port, "8443");
        assert_eq!(f.method, "GET");
    }
}
