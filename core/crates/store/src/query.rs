use crate::Result;
use flow::{FlowState, FlowSummary, SLOW_MS};
use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    All,
    Errors,
    Slow,
    Pinned,
}

#[derive(Clone, Debug, Default)]
pub struct Query {
    pub text: String,
    pub kind: Kind,
    pub since_ms: i64,
    pub until_ms: i64,
    /// Exact host names (case-insensitive). Empty means every host.
    pub hosts: Vec<String>,
    /// Exact SDK device ids. Empty means every device (and proxy traffic).
    pub devices: Vec<String>,
    /// Exact SDK app ids. Empty means every app.
    pub apps: Vec<String>,
    /// Also return proxy records that are hidden behind a merged SDK record.
    pub include_hidden: bool,
}

pub struct Page {
    pub total: u64,
    pub items: Vec<FlowSummary>,
}

/// Split on whitespace, keeping "quoted phrases" together.
fn terms(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn like(s: &str) -> String {
    let esc = s
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{esc}%")
}

fn fts_term(s: &str) -> Option<String> {
    if !s.chars().any(|c| c.is_alphanumeric()) {
        return None;
    }
    Some(format!("\"{}\"*", s.replace('"', "\"\"")))
}

/// Build the WHERE clause and its parameters.
fn build_where(q: &Query) -> (String, Vec<Value>) {
    let mut clauses: Vec<String> = Vec::new();
    let mut args: Vec<Value> = Vec::new();

    let mut kind = q.kind;
    if !q.include_hidden {
        // The wire half of a merged pair is shown through its SDK record.
        clauses.push("NOT (source = 'proxy' AND merged_with != 0)".into());
    }
    for (col, vals) in [("device_id", &q.devices), ("app_id", &q.apps)] {
        if !vals.is_empty() {
            let marks = vec!["?"; vals.len()].join(",");
            clauses.push(format!("{col} IN ({marks})"));
            for v in vals {
                args.push(v.clone().into());
            }
        }
    }
    if !q.hosts.is_empty() {
        let marks = vec!["?"; q.hosts.len()].join(",");
        clauses.push(format!("host COLLATE NOCASE IN ({marks})"));
        for h in &q.hosts {
            args.push(h.clone().into());
        }
    }
    for term in terms(&q.text) {
        let lower = term.to_ascii_lowercase();
        if let Some((key, val)) = lower
            .split_once(':')
            .filter(|(k, v)| !k.is_empty() && !v.is_empty())
        {
            match key {
                "method" => {
                    clauses.push("method = ? COLLATE NOCASE".into());
                    args.push(val.to_string().into());
                    continue;
                }
                "status" => {
                    if let Some(prefix) = val.strip_suffix("xx").and_then(|d| d.parse::<i64>().ok())
                    {
                        clauses.push("status BETWEEN ? AND ?".into());
                        args.push((prefix * 100).into());
                        args.push((prefix * 100 + 99).into());
                        continue;
                    }
                    let (op, num) = if let Some(n) = val.strip_prefix(">=") {
                        (">=", n)
                    } else if let Some(n) = val.strip_prefix("<=") {
                        ("<=", n)
                    } else if let Some(n) = val.strip_prefix('>') {
                        (">", n)
                    } else if let Some(n) = val.strip_prefix('<') {
                        ("<", n)
                    } else {
                        ("=", val)
                    };
                    if let Ok(n) = num.parse::<i64>() {
                        clauses.push(format!("status {op} ?"));
                        args.push(n.into());
                        continue;
                    }
                }
                "host" => {
                    clauses.push("host LIKE ? ESCAPE '\\'".into());
                    args.push(like(val).into());
                    continue;
                }
                "app" => {
                    clauses.push("app LIKE ? ESCAPE '\\'".into());
                    args.push(like(val).into());
                    continue;
                }
                "type" => {
                    clauses.push("content_type = ?".into());
                    args.push(val.to_string().into());
                    continue;
                }
                "source" if matches!(val, "proxy" | "sdk" | "merged") => {
                    // A merged pair counts as both: it is an SDK record that
                    // also has a wire view.
                    if val == "sdk" {
                        clauses.push("source IN ('sdk', 'merged')".into());
                    } else {
                        clauses.push("source = ?".into());
                        args.push(val.to_string().into());
                    }
                    continue;
                }
                "name" | "tag" | "owner" | "screen" => {
                    clauses.push(format!("{key} LIKE ? ESCAPE '\\'"));
                    args.push(like(val).into());
                    continue;
                }
                "route" => {
                    clauses.push("route_template LIKE ? ESCAPE '\\'".into());
                    args.push(like(val).into());
                    continue;
                }
                "device" => {
                    clauses.push("id IN (SELECT rowid FROM flow_sdk_fts WHERE flow_sdk_fts MATCH ?)".into());
                    args.push(format!("device:{}", fts_term(val).unwrap_or_else(|| "\"\"".into())).into());
                    continue;
                }
                "is" => {
                    match val {
                        "error" | "errors" | "failed" => kind = Kind::Errors,
                        "slow" => kind = Kind::Slow,
                        "pinned" => kind = Kind::Pinned,
                        _ => {}
                    }
                    continue;
                }
                _ => {}
            }
        }
        // Plain term: full text, host or path substring, or a bare status code.
        let mut any = vec![
            "host LIKE ? ESCAPE '\\'".to_string(),
            "(path || CASE WHEN query = '' THEN '' ELSE '?' || query END) LIKE ? ESCAPE '\\'"
                .to_string(),
        ];
        args.push(like(&term).into());
        args.push(like(&term).into());
        if let Some(m) = fts_term(&term) {
            any.push("id IN (SELECT rowid FROM flow_fts WHERE flow_fts MATCH ?)".into());
            args.push(m.clone().into());
            any.push("id IN (SELECT rowid FROM flow_sdk_fts WHERE flow_sdk_fts MATCH ?)".into());
            args.push(m.into());
        }
        if term.len() == 3 && term.bytes().all(|b| b.is_ascii_digit()) {
            any.push("status = ?".into());
            args.push(term.parse::<i64>().unwrap_or(0).into());
        }
        clauses.push(format!("({})", any.join(" OR ")));
    }

    match kind {
        Kind::All => {}
        Kind::Errors => clauses.push("(status >= 400 OR (flags & 4) != 0 OR state = 3)".into()),
        Kind::Slow => {
            clauses.push("duration_ms >= ?".into());
            args.push(SLOW_MS.into());
        }
        Kind::Pinned => clauses.push("(flags & 1) != 0".into()),
    }
    if q.since_ms > 0 {
        clauses.push("started_at >= ?".into());
        args.push(q.since_ms.into());
    }
    if q.until_ms > 0 {
        clauses.push("started_at <= ?".into());
        args.push(q.until_ms.into());
    }
    let sql = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    (sql, args)
}

pub fn run(conn: &Connection, q: &Query, offset: u32, limit: u32) -> Result<Page> {
    let (where_sql, args) = build_where(q);
    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM flow{where_sql}"),
        params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    let mut all = args.clone();
    all.push((limit as i64).into());
    all.push((offset as i64).into());
    let mut stmt = conn.prepare(&format!(
        "SELECT id, started_at, duration_ms, scheme, host, port, method, path, query, status,
                protocol, app, req_size, res_size, state, flags, content_type, error,
                source, merged_with, device_id, app_id, name, tag, owner, screen,
                route_template, client_stack
         FROM flow{where_sql} ORDER BY id ASC LIMIT ? OFFSET ?"
    ))?;
    let items = stmt
        .query_map(params_from_iter(all.iter()), |r| {
            let path: String = r.get(7)?;
            let query: String = r.get(8)?;
            Ok(FlowSummary {
                id: r.get::<_, i64>(0)? as u64,
                started_at_ms: r.get(1)?,
                duration_ms: r.get(2)?,
                scheme: r.get(3)?,
                host: r.get(4)?,
                port: r.get::<_, i64>(5)? as u16,
                method: r.get(6)?,
                path: if query.is_empty() {
                    path
                } else {
                    format!("{path}?{query}")
                },
                status: r.get::<_, i64>(9)? as u16,
                protocol: r.get(10)?,
                app: r.get(11)?,
                req_size: r.get::<_, i64>(12)? as u64,
                res_size: r.get::<_, i64>(13)? as u64,
                state: FlowState::from_u8(r.get::<_, i64>(14)? as u8),
                flags: r.get::<_, i64>(15)? as u32,
                content_type: r.get(16)?,
                error: r.get(17)?,
                source: flow::Source::parse(&r.get::<_, String>(18)?),
                merged_with: r.get::<_, i64>(19)? as u64,
                device_id: r.get(20)?,
                app_id: r.get(21)?,
                name: r.get(22)?,
                tag: r.get(23)?,
                owner: r.get(24)?,
                screen: r.get(25)?,
                route_template: r.get(26)?,
                client_stack: r.get(27)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(Page {
        total: total as u64,
        items,
    })
}

/// One row of the host list: what traffic a host has produced.
#[derive(Clone, Debug, Default)]
pub struct HostRow {
    pub host: String,
    pub flows: u64,
    pub errors: u64,
    pub last_started_ms: i64,
    pub pinned: u64,
    pub tunnels: u64,
    pub mapped: u64,
    pub bypassed: u64,
}

/// Hosts with stored traffic, most recently active first.
pub fn hosts(conn: &Connection, limit: u32) -> Result<Vec<HostRow>> {
    let mut stmt = conn.prepare(
        "SELECT host, COUNT(*),
                SUM(CASE WHEN status >= 400 OR (flags & 4) != 0 OR state = 3 THEN 1 ELSE 0 END),
                MAX(started_at),
                SUM(CASE WHEN (flags & 1) != 0 THEN 1 ELSE 0 END),
                SUM(CASE WHEN (flags & 2) != 0 THEN 1 ELSE 0 END),
                SUM(CASE WHEN (flags & 64) != 0 THEN 1 ELSE 0 END),
                SUM(CASE WHEN (flags & 128) != 0 THEN 1 ELSE 0 END)
         FROM flow WHERE NOT (source = 'proxy' AND merged_with != 0)
         GROUP BY host ORDER BY MAX(id) DESC LIMIT ?",
    )?;
    let rows = stmt
        .query_map([limit as i64], |r| {
            Ok(HostRow {
                host: r.get(0)?,
                flows: r.get::<_, i64>(1)? as u64,
                errors: r.get::<_, i64>(2)? as u64,
                last_started_ms: r.get(3)?,
                pinned: r.get::<_, i64>(4)? as u64,
                tunnels: r.get::<_, i64>(5)? as u64,
                mapped: r.get::<_, i64>(6)? as u64,
                bypassed: r.get::<_, i64>(7)? as u64,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Stored SDK traffic of one device and app.
#[derive(Clone, Debug, Default)]
pub struct DeviceRow {
    pub device_id: String,
    pub app_id: String,
    pub flows: u64,
    pub last_started_ms: i64,
}

/// Flow counts per SDK device and app.
pub fn devices(conn: &Connection) -> Result<Vec<DeviceRow>> {
    let mut stmt = conn.prepare(
        "SELECT device_id, app_id, COUNT(*), MAX(started_at) FROM flow
         WHERE device_id != '' GROUP BY device_id, app_id",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(DeviceRow {
                device_id: r.get(0)?,
                app_id: r.get(1)?,
                flows: r.get::<_, i64>(2)? as u64,
                last_started_ms: r.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Cheap in-memory version of [`Query`] for live events. Only sees what a
/// summary carries (method, host, path, status, app, type), so words that
/// would match headers or bodies in a stored search do not match here.
pub fn matches_summary(q: &Query, s: &FlowSummary) -> bool {
    let mut kind = q.kind;
    let hay_path = s.path.to_ascii_lowercase();
    let host = s.host.to_ascii_lowercase();
    if !q.hosts.is_empty() && !q.hosts.iter().any(|h| h.eq_ignore_ascii_case(&s.host)) {
        return false;
    }
    if !q.include_hidden && s.source == flow::Source::Proxy && s.merged_with != 0 {
        return false;
    }
    if !q.devices.is_empty() && !q.devices.contains(&s.device_id) {
        return false;
    }
    if !q.apps.is_empty() && !q.apps.contains(&s.app_id) {
        return false;
    }
    for term in terms(&q.text) {
        let lower = term.to_ascii_lowercase();
        if let Some((key, val)) = lower
            .split_once(':')
            .filter(|(k, v)| !k.is_empty() && !v.is_empty())
        {
            let handled = match key {
                "source" if matches!(val, "proxy" | "sdk" | "merged") => {
                    let ok = match (val, s.source) {
                        ("sdk", flow::Source::Sdk | flow::Source::Merged) => true,
                        ("proxy", flow::Source::Proxy) => true,
                        ("merged", flow::Source::Merged) => true,
                        _ => false,
                    };
                    if !ok {
                        return false;
                    }
                    true
                }
                "name" | "tag" | "owner" | "screen" => {
                    let field = match key {
                        "name" => &s.name,
                        "tag" => &s.tag,
                        "owner" => &s.owner,
                        _ => &s.screen,
                    };
                    if !field.to_ascii_lowercase().contains(val) {
                        return false;
                    }
                    true
                }
                "route" => {
                    if !s.route_template.to_ascii_lowercase().contains(val) {
                        return false;
                    }
                    true
                }
                "method" => {
                    if !s.method.eq_ignore_ascii_case(val) {
                        return false;
                    }
                    true
                }
                "status" => {
                    let st = s.status as i64;
                    let ok = if let Some(p) =
                        val.strip_suffix("xx").and_then(|d| d.parse::<i64>().ok())
                    {
                        st / 100 == p
                    } else if let Some(n) =
                        val.strip_prefix(">=").and_then(|n| n.parse::<i64>().ok())
                    {
                        st >= n
                    } else if let Some(n) =
                        val.strip_prefix("<=").and_then(|n| n.parse::<i64>().ok())
                    {
                        st <= n
                    } else if let Some(n) =
                        val.strip_prefix('>').and_then(|n| n.parse::<i64>().ok())
                    {
                        st > n
                    } else if let Some(n) =
                        val.strip_prefix('<').and_then(|n| n.parse::<i64>().ok())
                    {
                        st < n
                    } else if let Ok(n) = val.parse::<i64>() {
                        st == n
                    } else {
                        return false;
                    };
                    if !ok {
                        return false;
                    }
                    true
                }
                "host" => {
                    if !host.contains(val) {
                        return false;
                    }
                    true
                }
                "app" => {
                    if !s.app.to_ascii_lowercase().contains(val) {
                        return false;
                    }
                    true
                }
                "type" => {
                    if s.content_type != val {
                        return false;
                    }
                    true
                }
                "is" => {
                    match val {
                        "error" | "errors" | "failed" => kind = Kind::Errors,
                        "slow" => kind = Kind::Slow,
                        "pinned" => kind = Kind::Pinned,
                        _ => {}
                    }
                    true
                }
                _ => false,
            };
            if handled {
                continue;
            }
        }
        let status_hit = term.len() == 3 && term.parse::<u16>().ok() == Some(s.status);
        let sdk_hit = [&s.name, &s.tag, &s.owner, &s.screen, &s.route_template]
            .iter()
            .any(|f| f.to_ascii_lowercase().contains(&lower));
        if !(host.contains(&lower) || hay_path.contains(&lower) || status_hit || sdk_hit) {
            return false;
        }
    }
    match kind {
        Kind::All => {}
        Kind::Errors => {
            if !(s.status >= 400
                || s.flags & flow::flags::ERROR != 0
                || s.state == FlowState::Failed)
            {
                return false;
            }
        }
        Kind::Slow => {
            if s.duration_ms < SLOW_MS {
                return false;
            }
        }
        Kind::Pinned => {
            if s.flags & flow::flags::PINNED == 0 {
                return false;
            }
        }
    }
    if q.since_ms > 0 && s.started_at_ms < q.since_ms {
        return false;
    }
    if q.until_ms > 0 && s.started_at_ms > q.until_ms {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_matching() {
        let s = FlowSummary {
            host: "api.example.com".into(),
            path: "/v1/me".into(),
            method: "GET".into(),
            status: 404,
            ..Default::default()
        };
        let q = |t: &str, kind| Query {
            text: t.into(),
            kind,
            ..Default::default()
        };
        assert!(matches_summary(&q("example v1", Kind::All), &s));
        assert!(matches_summary(&q("status:4xx method:get", Kind::All), &s));
        assert!(matches_summary(&q("", Kind::Errors), &s));
        assert!(!matches_summary(&q("nope", Kind::All), &s));
        assert!(!matches_summary(&q("", Kind::Slow), &s));
    }

    #[test]
    fn host_focus_is_exact() {
        let s = FlowSummary {
            host: "api.example.com".into(),
            ..Default::default()
        };
        let q = |hosts: &[&str]| Query {
            hosts: hosts.iter().map(|h| h.to_string()).collect(),
            ..Default::default()
        };
        assert!(matches_summary(&q(&["API.example.com"]), &s));
        assert!(matches_summary(&q(&["x.org", "api.example.com"]), &s));
        // Unlike `host:`, a focus never matches a longer name.
        assert!(!matches_summary(&q(&["example.com"]), &s));
        let (sql, args) = build_where(&q(&["a", "b"]));
        assert!(sql.contains("host COLLATE NOCASE IN (?,?)"));
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn term_splitting() {
        assert_eq!(terms(r#"a "b c" d:e"#), ["a", "b c", "d:e"]);
    }

    #[test]
    fn operators_become_clauses() {
        let (sql, args) = build_where(&Query {
            text: "method:post status:5xx host:example.com is:slow".into(),
            ..Default::default()
        });
        assert!(sql.contains("method = ? COLLATE NOCASE"));
        assert!(sql.contains("status BETWEEN ? AND ?"));
        assert!(sql.contains("duration_ms >= ?"));
        assert_eq!(args.len(), 5);
    }
}
