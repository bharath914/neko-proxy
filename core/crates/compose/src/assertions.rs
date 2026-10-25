//! Tests run against a response after every send.

use crate::{AssertKind, AssertOp, Assertion};
use serde_json::Value;
use std::cell::Cell;

/// The parts of a response a test can look at. The body is already decoded.
pub struct ResponseView<'a> {
    pub status: u16,
    pub headers: &'a [(String, String)],
    pub body: &'a [u8],
    pub duration_ms: f64,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TestResult {
    /// What was checked, in words: "Status equals 200".
    pub name: String,
    pub passed: bool,
    /// Why it failed (what was found); empty when it passed.
    pub message: String,
}

/// Run every enabled test, in order.
pub fn run_assertions(tests: &[Assertion], res: &ResponseView) -> Vec<TestResult> {
    tests
        .iter()
        .filter(|t| !t.disabled)
        .map(|t| {
            let name = describe(t);
            match check(t, res) {
                Ok(()) => TestResult {
                    name,
                    passed: true,
                    message: String::new(),
                },
                Err(message) => TestResult {
                    name,
                    passed: false,
                    message,
                },
            }
        })
        .collect()
}

// ---------------------------------------------------------------- naming

fn op_words(op: AssertOp) -> &'static str {
    match op {
        AssertOp::Equals => "equals",
        AssertOp::NotEquals => "does not equal",
        AssertOp::Contains => "contains",
        AssertOp::NotContains => "does not contain",
        AssertOp::Exists => "exists",
        AssertOp::NotExists => "does not exist",
        AssertOp::LessThan => "less than",
        AssertOp::GreaterThan => "greater than",
        AssertOp::Matches => "matches",
    }
}

fn takes_expected(op: AssertOp) -> bool {
    !matches!(op, AssertOp::Exists | AssertOp::NotExists)
}

fn describe(t: &Assertion) -> String {
    let op = op_words(t.op);
    let exp = t.expected.trim();
    let tail = |s: &mut String| {
        if takes_expected(t.op) {
            s.push(' ');
            s.push_str(exp);
        }
    };
    let mut s = match t.kind {
        AssertKind::Status => format!("Status {op}"),
        AssertKind::Header => format!("Header {} {op}", t.target.trim()),
        AssertKind::Body => {
            let mut s = format!("Body {op}");
            if takes_expected(t.op) {
                s.push_str(&format!(" \"{}\"", truncate(&t.expected, 60)));
            }
            return s;
        }
        AssertKind::Json => format!("JSON {} {op}", t.target.trim()),
        AssertKind::Time => format!("Response time {op}"),
        AssertKind::Size => format!("Response size {op}"),
    };
    tail(&mut s);
    match t.kind {
        AssertKind::Time if takes_expected(t.op) => s.push_str(" ms"),
        AssertKind::Size if takes_expected(t.op) => s.push_str(" bytes"),
        _ => {}
    }
    s
}

fn kind_word(k: AssertKind) -> &'static str {
    match k {
        AssertKind::Status => "status",
        AssertKind::Header => "header",
        AssertKind::Body => "body",
        AssertKind::Json => "JSON",
        AssertKind::Time => "time",
        AssertKind::Size => "size",
    }
}

fn unsupported(t: &Assertion) -> String {
    let w = op_words(t.op);
    let mut c = w.chars();
    let cap: String = c
        .next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default();
    format!("{cap} is not supported for {}", kind_word(t.kind))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

fn got(s: &str) -> String {
    format!("Got \"{}\"", truncate(s, 120))
}

fn fmt_num(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        let s = format!("{n:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

// ---------------------------------------------------------------- checks

fn check(t: &Assertion, res: &ResponseView) -> Result<(), String> {
    match t.kind {
        AssertKind::Status => check_status(t, res),
        AssertKind::Header => check_header(t, res),
        AssertKind::Body => {
            let body = String::from_utf8_lossy(res.body);
            check_text(t, Some(&body), body.is_empty(), true, "Body is empty", "Body is not empty")
        }
        AssertKind::Json => check_json(t, res),
        AssertKind::Time => check_number(t, res.duration_ms, " ms"),
        AssertKind::Size => check_number(t, res.size as f64, " bytes"),
    }
}

fn parse_num(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|n| n.is_finite())
}

/// Number comparison for status, time and size.
fn check_number(t: &Assertion, actual: f64, unit: &str) -> Result<(), String> {
    let exp = parse_num(&t.expected);
    let need = || exp.ok_or_else(|| format!("Expected value \"{}\" is not a number", t.expected.trim()));
    let ok = match t.op {
        AssertOp::Equals => (actual - need()?).abs() < 0.5,
        AssertOp::NotEquals => (actual - need()?).abs() >= 0.5,
        AssertOp::LessThan => actual < need()?,
        AssertOp::GreaterThan => actual > need()?,
        _ => return Err(unsupported(t)),
    };
    if ok {
        Ok(())
    } else {
        Err(format!("Got {}{unit}", fmt_num(actual)))
    }
}

fn check_status(t: &Assertion, res: &ResponseView) -> Result<(), String> {
    if matches!(t.op, AssertOp::Equals | AssertOp::NotEquals) {
        if let Some(class) = status_class(&t.expected) {
            let is = res.status / 100 == class;
            let want = t.op == AssertOp::Equals;
            return if is == want {
                Ok(())
            } else {
                Err(format!("Got {}", res.status))
            };
        }
    }
    check_number(t, f64::from(res.status), "")
}

/// "2xx" / "2XX" gives 2.
fn status_class(s: &str) -> Option<u16> {
    let b = s.trim().as_bytes();
    if b.len() == 3 && b[0].is_ascii_digit() && b[1..].eq_ignore_ascii_case(b"xx") {
        Some(u16::from(b[0] - b'0'))
    } else {
        None
    }
}

fn check_header(t: &Assertion, res: &ResponseView) -> Result<(), String> {
    let name = t.target.trim();
    if name.is_empty() {
        return Err("No header name set".into());
    }
    let value = res
        .headers
        .iter()
        .find(|(k, _)| k.trim().eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str());
    check_text(t, value, value.is_none(), false, "Header not present", "Header is present")
}

/// Shared text operations. `value` is None when there is nothing to compare
/// (header absent); `missing` says whether the thing counts as absent.
fn check_text(
    t: &Assertion,
    value: Option<&str>,
    missing: bool,
    trim_equals: bool,
    absent_msg: &str,
    present_msg: &str,
) -> Result<(), String> {
    match t.op {
        AssertOp::Exists => return if missing { Err(absent_msg.into()) } else { Ok(()) },
        AssertOp::NotExists => return if missing { Ok(()) } else { Err(present_msg.into()) },
        _ => {}
    }
    let Some(v) = value else {
        // Nothing there: negative checks hold, the rest cannot.
        return match t.op {
            AssertOp::NotEquals | AssertOp::NotContains => Ok(()),
            _ => Err(absent_msg.into()),
        };
    };
    let ok = match t.op {
        AssertOp::Equals => text_eq(v, &t.expected, trim_equals),
        AssertOp::NotEquals => !text_eq(v, &t.expected, trim_equals),
        AssertOp::Contains => v.contains(t.expected.as_str()),
        AssertOp::NotContains => !v.contains(t.expected.as_str()),
        AssertOp::Matches => regex_matches(&t.expected, v)?,
        AssertOp::LessThan | AssertOp::GreaterThan => {
            if t.kind == AssertKind::Body {
                return Err(unsupported(t));
            }
            return compare_text_numbers(t, v);
        }
        AssertOp::Exists | AssertOp::NotExists => unreachable!(),
    };
    if ok {
        Ok(())
    } else {
        Err(got(v))
    }
}

fn text_eq(a: &str, b: &str, trim: bool) -> bool {
    if trim {
        a.trim_end() == b.trim_end()
    } else {
        a == b
    }
}

fn compare_text_numbers(t: &Assertion, actual: &str) -> Result<(), String> {
    let a = parse_num(actual).ok_or_else(|| format!("Value is not a number: {}", got(actual)))?;
    let e = parse_num(&t.expected)
        .ok_or_else(|| format!("Expected value \"{}\" is not a number", t.expected.trim()))?;
    let ok = if t.op == AssertOp::LessThan { a < e } else { a > e };
    if ok {
        Ok(())
    } else {
        Err(got(actual))
    }
}

// ---------------------------------------------------------------- JSON

#[derive(Debug, PartialEq)]
enum Seg {
    Key(String),
    Index(usize),
}

fn parse_path(path: &str) -> Result<Vec<Seg>, String> {
    let mut s = path.trim();
    if let Some(rest) = s.strip_prefix('$') {
        s = rest;
    }
    let chars: Vec<char> = s.chars().collect();
    let mut segs = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '.' => i += 1,
            '[' => {
                let close = chars[i..]
                    .iter()
                    .position(|&c| c == ']')
                    .ok_or_else(|| format!("Bad path {path}: missing ]"))?
                    + i;
                let inner: String = chars[i + 1..close].iter().collect();
                let inner = inner.trim();
                let quoted = inner.len() >= 2
                    && ((inner.starts_with('"') && inner.ends_with('"'))
                        || (inner.starts_with('\'') && inner.ends_with('\'')));
                if quoted {
                    segs.push(Seg::Key(inner[1..inner.len() - 1].to_string()));
                } else if let Ok(n) = inner.parse::<usize>() {
                    segs.push(Seg::Index(n));
                } else {
                    return Err(format!("Bad path {path}: [{inner}] is not an index"));
                }
                i = close + 1;
            }
            _ => {
                let start = i;
                while i < chars.len() && chars[i] != '.' && chars[i] != '[' {
                    i += 1;
                }
                let key: String = chars[start..i].iter().collect();
                segs.push(Seg::Key(key));
            }
        }
    }
    Ok(segs)
}

fn resolve<'a>(root: &'a Value, segs: &[Seg]) -> Option<&'a Value> {
    let mut cur = root;
    for seg in segs {
        cur = match (seg, cur) {
            (Seg::Key(k), Value::Object(m)) => m.get(k)?,
            (Seg::Key(k), Value::Array(a)) => a.get(k.parse::<usize>().ok()?)?,
            (Seg::Index(n), Value::Array(a)) => a.get(*n)?,
            (Seg::Index(n), Value::Object(m)) => m.get(&n.to_string())?,
            _ => return None,
        };
    }
    Some(cur)
}

fn render(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn json_equals(v: &Value, expected: &str) -> bool {
    let exp = expected.trim();
    match v {
        Value::String(s) => s == expected,
        Value::Number(n) => match (n.as_f64(), parse_num(exp)) {
            (Some(a), Some(b)) => a == b,
            _ => n.to_string() == exp,
        },
        Value::Bool(_) | Value::Null => render(v) == exp,
        _ => {
            render(v) == exp
                || serde_json::from_str::<Value>(exp)
                    .map(|e| &e == v)
                    .unwrap_or(false)
        }
    }
}

fn json_contains(v: &Value, expected: &str) -> bool {
    match v {
        Value::String(s) => s.contains(expected),
        Value::Array(a) => a.iter().any(|e| json_equals(e, expected)),
        Value::Object(m) => m.contains_key(expected.trim()),
        other => other.to_string().contains(expected.trim()),
    }
}

fn check_json(t: &Assertion, res: &ResponseView) -> Result<(), String> {
    let target = t.target.trim();
    let root: Value = serde_json::from_slice(res.body).map_err(|_| "The body is not JSON".to_string())?;
    let segs = parse_path(target)?;
    let found = resolve(&root, &segs);
    match t.op {
        AssertOp::Exists => {
            return found.map(|_| ()).ok_or_else(|| format!("No value at {target}"));
        }
        AssertOp::NotExists => {
            return match found {
                None => Ok(()),
                Some(v) => Err(got(&render(v))),
            };
        }
        _ => {}
    }
    let Some(v) = found else {
        return match t.op {
            AssertOp::NotEquals | AssertOp::NotContains => Ok(()),
            _ => Err(format!("No value at {target}")),
        };
    };
    let ok = match t.op {
        AssertOp::Equals => json_equals(v, &t.expected),
        AssertOp::NotEquals => !json_equals(v, &t.expected),
        AssertOp::Contains => json_contains(v, &t.expected),
        AssertOp::NotContains => !json_contains(v, &t.expected),
        AssertOp::Matches => regex_matches(&t.expected, &render(v))?,
        AssertOp::LessThan | AssertOp::GreaterThan => {
            let a = match v {
                Value::Number(n) => n.as_f64(),
                Value::String(s) => parse_num(s),
                _ => None,
            }
            .ok_or_else(|| format!("Value is not a number: {}", got(&render(v))))?;
            let e = parse_num(&t.expected)
                .ok_or_else(|| format!("Expected value \"{}\" is not a number", t.expected.trim()))?;
            if t.op == AssertOp::LessThan {
                a < e
            } else {
                a > e
            }
        }
        AssertOp::Exists | AssertOp::NotExists => unreachable!(),
    };
    if ok {
        Ok(())
    } else {
        Err(got(&render(v)))
    }
}

// ---------------------------------------------------------------- mini regex
//
// Supported: literals, `.`, `^`, `$`, `*` `+` `?` `{n}` `{n,}` `{n,m}`
// (greedy), `[...]` with ranges, `[^...]`, `\d \w \s \D \W \S`, escapes,
// groups `( )` and `(?: )`, and alternation `|`. Search, not full match.

#[derive(Debug, Clone)]
enum Node {
    Char(char),
    Any,
    Class(Vec<ClassItem>, bool),
    Start,
    End,
    Group(Vec<Vec<Node>>),
    Repeat(Box<Node>, usize, usize),
}

#[derive(Debug, Clone)]
enum ClassItem {
    Range(char, char),
    Digit(bool),
    Word(bool),
    Space(bool),
}

fn class_hit(items: &[ClassItem], c: char) -> bool {
    items.iter().any(|it| match *it {
        ClassItem::Range(a, b) => a <= c && c <= b,
        ClassItem::Digit(neg) => c.is_ascii_digit() != neg,
        ClassItem::Word(neg) => (c.is_alphanumeric() || c == '_') != neg,
        ClassItem::Space(neg) => c.is_whitespace() != neg,
    })
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn alternatives(&mut self, in_group: bool) -> Result<Vec<Vec<Node>>, String> {
        let mut alts = vec![Vec::new()];
        while let Some(c) = self.peek() {
            match c {
                ')' if in_group => break,
                ')' => return Err("unmatched )".into()),
                '|' => {
                    self.pos += 1;
                    alts.push(Vec::new());
                }
                _ => {
                    let atom = self.atom()?;
                    let atom = self.quantified(atom)?;
                    alts.last_mut().unwrap().push(atom);
                }
            }
        }
        Ok(alts)
    }

    fn atom(&mut self) -> Result<Node, String> {
        let c = self.peek().ok_or("unexpected end")?;
        self.pos += 1;
        Ok(match c {
            '.' => Node::Any,
            '^' => Node::Start,
            '$' => Node::End,
            '(' => {
                if self.peek() == Some('?') {
                    if self.chars.get(self.pos + 1) == Some(&':') {
                        self.pos += 2;
                    } else {
                        return Err("unsupported group syntax".into());
                    }
                }
                let alts = self.alternatives(true)?;
                if self.peek() != Some(')') {
                    return Err("missing )".into());
                }
                self.pos += 1;
                Node::Group(alts)
            }
            '[' => self.class()?,
            '\\' => {
                let e = self.peek().ok_or("trailing backslash")?;
                self.pos += 1;
                match e {
                    'd' => Node::Class(vec![ClassItem::Digit(false)], false),
                    'D' => Node::Class(vec![ClassItem::Digit(true)], false),
                    'w' => Node::Class(vec![ClassItem::Word(false)], false),
                    'W' => Node::Class(vec![ClassItem::Word(true)], false),
                    's' => Node::Class(vec![ClassItem::Space(false)], false),
                    'S' => Node::Class(vec![ClassItem::Space(true)], false),
                    'n' => Node::Char('\n'),
                    't' => Node::Char('\t'),
                    'r' => Node::Char('\r'),
                    other => Node::Char(other),
                }
            }
            '*' | '+' | '?' => return Err(format!("nothing to repeat before {c}")),
            other => Node::Char(other),
        })
    }

    fn class(&mut self) -> Result<Node, String> {
        let mut neg = false;
        if self.peek() == Some('^') {
            neg = true;
            self.pos += 1;
        }
        let mut items = Vec::new();
        let mut first = true;
        loop {
            let c = self.peek().ok_or("missing ]")?;
            self.pos += 1;
            if c == ']' && !first {
                break;
            }
            first = false;
            let lo = if c == '\\' {
                let e = self.peek().ok_or("trailing backslash")?;
                self.pos += 1;
                match e {
                    'd' => {
                        items.push(ClassItem::Digit(false));
                        continue;
                    }
                    'D' => {
                        items.push(ClassItem::Digit(true));
                        continue;
                    }
                    'w' => {
                        items.push(ClassItem::Word(false));
                        continue;
                    }
                    'W' => {
                        items.push(ClassItem::Word(true));
                        continue;
                    }
                    's' => {
                        items.push(ClassItem::Space(false));
                        continue;
                    }
                    'S' => {
                        items.push(ClassItem::Space(true));
                        continue;
                    }
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    other => other,
                }
            } else {
                c
            };
            // Range `a-z` (a trailing `-` is a literal).
            if self.peek() == Some('-') && self.chars.get(self.pos + 1).is_some_and(|&n| n != ']') {
                self.pos += 1;
                let mut hi = self.peek().ok_or("missing ]")?;
                self.pos += 1;
                if hi == '\\' {
                    hi = self.peek().ok_or("trailing backslash")?;
                    self.pos += 1;
                }
                if hi < lo {
                    return Err("reversed range in [ ]".into());
                }
                items.push(ClassItem::Range(lo, hi));
            } else {
                items.push(ClassItem::Range(lo, lo));
            }
        }
        Ok(Node::Class(items, neg))
    }

    fn quantified(&mut self, atom: Node) -> Result<Node, String> {
        let (min, max) = match self.peek() {
            Some('*') => (0, usize::MAX),
            Some('+') => (1, usize::MAX),
            Some('?') => (0, 1),
            Some('{') => match self.braces() {
                Some(r) => return Ok(Node::Repeat(Box::new(atom), r.0, r.1)),
                None => return Ok(atom),
            },
            _ => return Ok(atom),
        };
        self.pos += 1;
        if matches!(atom, Node::Start | Node::End) {
            return Err("nothing to repeat".into());
        }
        // A trailing `?` (lazy) is accepted but treated as greedy-free: keep it simple.
        if self.peek() == Some('?') {
            return Err("lazy quantifiers are not supported".into());
        }
        Ok(Node::Repeat(Box::new(atom), min, max))
    }

    /// `{n}`, `{n,}`, `{n,m}`; anything else is a literal `{`.
    fn braces(&mut self) -> Option<(usize, usize)> {
        let rest: String = self.chars[self.pos + 1..].iter().collect();
        let close = rest.find('}')?;
        let inner = &rest[..close];
        let (min, max) = match inner.split_once(',') {
            None => {
                let n = inner.parse().ok()?;
                (n, n)
            }
            Some((a, "")) => (a.parse().ok()?, usize::MAX),
            Some((a, b)) => (a.parse().ok()?, b.parse().ok()?),
        };
        if max < min || min > 1000 || (max != usize::MAX && max > 1000) {
            return None;
        }
        self.pos += 1 + inner.chars().count() + 1;
        Some((min, max))
    }
}

struct Matcher<'a> {
    text: &'a [char],
    steps: Cell<u32>,
}

const STEP_LIMIT: u32 = 2_000_000;

type Cont<'k> = &'k mut dyn FnMut(usize) -> bool;

impl Matcher<'_> {
    fn seq(&self, nodes: &[Node], pos: usize, k: Cont) -> bool {
        match nodes.split_first() {
            None => k(pos),
            Some((first, rest)) => self.node(first, pos, &mut |p| self.seq(rest, p, k)),
        }
    }

    fn node(&self, n: &Node, pos: usize, k: Cont) -> bool {
        let s = self.steps.get() + 1;
        self.steps.set(s);
        if s > STEP_LIMIT {
            return false;
        }
        let at = self.text.get(pos).copied();
        match n {
            Node::Char(c) => at == Some(*c) && k(pos + 1),
            Node::Any => at.is_some_and(|c| c != '\n') && k(pos + 1),
            Node::Class(items, neg) => at.is_some_and(|c| class_hit(items, c) != *neg) && k(pos + 1),
            Node::Start => pos == 0 && k(pos),
            Node::End => pos == self.text.len() && k(pos),
            Node::Group(alts) => {
                for alt in alts {
                    if self.seq(alt, pos, k) {
                        return true;
                    }
                }
                false
            }
            Node::Repeat(inner, min, max) => self.repeat(inner, *min, *max, 0, pos, k),
        }
    }

    fn repeat(&self, inner: &Node, min: usize, max: usize, count: usize, pos: usize, k: Cont) -> bool {
        if count < max {
            let hit = self.node(inner, pos, &mut |p| {
                // An empty iteration past the minimum would loop forever.
                if p == pos && count >= min {
                    return false;
                }
                self.repeat(inner, min, max, count + 1, p, k)
            });
            if hit {
                return true;
            }
        }
        count >= min && k(pos)
    }
}

/// Does `pattern` match anywhere in `text`? Err when the pattern is invalid.
fn regex_matches(pattern: &str, text: &str) -> Result<bool, String> {
    let mut p = Parser {
        chars: pattern.chars().collect(),
        pos: 0,
    };
    let alts = p.alternatives(false).map_err(|e| format!("Invalid pattern: {e}"))?;
    let root = [Node::Group(alts)];
    let chars: Vec<char> = text.chars().collect();
    let m = Matcher {
        text: &chars,
        steps: Cell::new(0),
    };
    for start in 0..=chars.len() {
        if m.seq(&root, start, &mut |_| true) {
            return Ok(true);
        }
        if m.steps.get() > STEP_LIMIT {
            return Err("Pattern is too complex".into());
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hdrs(p: &[(&str, &str)]) -> Vec<(String, String)> {
        p.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    fn a(kind: AssertKind, target: &str, op: AssertOp, expected: &str) -> Assertion {
        Assertion {
            kind,
            target: target.into(),
            op,
            expected: expected.into(),
            disabled: false,
        }
    }

    fn run1(t: Assertion, status: u16, headers: &[(&str, &str)], body: &str, ms: f64, size: u64) -> TestResult {
        let h = hdrs(headers);
        let view = ResponseView {
            status,
            headers: &h,
            body: body.as_bytes(),
            duration_ms: ms,
            size,
        };
        let mut r = run_assertions(&[t], &view);
        assert_eq!(r.len(), 1);
        r.remove(0)
    }

    fn json(target: &str, op: AssertOp, expected: &str, body: &str) -> TestResult {
        run1(a(AssertKind::Json, target, op, expected), 200, &[], body, 1.0, 1)
    }

    fn pass(r: &TestResult) {
        assert!(r.passed, "{} failed: {}", r.name, r.message);
        assert!(r.message.is_empty());
    }

    fn fail(r: &TestResult, msg: &str) {
        assert!(!r.passed, "{} should fail", r.name);
        assert_eq!(r.message, msg, "{}", r.name);
    }

    #[test]
    fn disabled_skipped_and_order_kept() {
        let mut off = a(AssertKind::Status, "", AssertOp::Equals, "500");
        off.disabled = true;
        let ts = [
            a(AssertKind::Status, "", AssertOp::Equals, "200"),
            off,
            a(AssertKind::Status, "", AssertOp::NotEquals, "200"),
        ];
        let view = ResponseView { status: 200, headers: &[], body: b"", duration_ms: 1.0, size: 0 };
        let r = run_assertions(&ts, &view);
        assert_eq!(r.len(), 2);
        assert!(r[0].passed && !r[1].passed);
        assert!(run_assertions(&[], &view).is_empty());
    }

    #[test]
    fn status_cases() {
        let r = run1(a(AssertKind::Status, "", AssertOp::Equals, "200"), 200, &[], "", 1.0, 0);
        pass(&r);
        assert_eq!(r.name, "Status equals 200");
        let r = run1(a(AssertKind::Status, "", AssertOp::Equals, "200"), 404, &[], "", 1.0, 0);
        fail(&r, "Got 404");
        pass(&run1(a(AssertKind::Status, "", AssertOp::Equals, "2xx"), 204, &[], "", 1.0, 0));
        pass(&run1(a(AssertKind::Status, "", AssertOp::Equals, "2XX"), 299, &[], "", 1.0, 0));
        fail(&run1(a(AssertKind::Status, "", AssertOp::Equals, "2xx"), 301, &[], "", 1.0, 0), "Got 301");
        pass(&run1(a(AssertKind::Status, "", AssertOp::NotEquals, "5xx"), 200, &[], "", 1.0, 0));
        fail(&run1(a(AssertKind::Status, "", AssertOp::NotEquals, "5xx"), 503, &[], "", 1.0, 0), "Got 503");
        pass(&run1(a(AssertKind::Status, "", AssertOp::NotEquals, "404"), 200, &[], "", 1.0, 0));
        pass(&run1(a(AssertKind::Status, "", AssertOp::LessThan, "400"), 200, &[], "", 1.0, 0));
        fail(&run1(a(AssertKind::Status, "", AssertOp::LessThan, "400"), 500, &[], "", 1.0, 0), "Got 500");
        pass(&run1(a(AssertKind::Status, "", AssertOp::GreaterThan, " 199 "), 200, &[], "", 1.0, 0));
        fail(
            &run1(a(AssertKind::Status, "", AssertOp::Equals, "abc"), 200, &[], "", 1.0, 0),
            "Expected value \"abc\" is not a number",
        );
        fail(
            &run1(a(AssertKind::Status, "", AssertOp::Contains, "2"), 200, &[], "", 1.0, 0),
            "Contains is not supported for status",
        );
    }

    #[test]
    fn header_cases() {
        let h = [("Content-Type", "application/json; charset=utf-8"), ("X-N", "42")];
        let t = |op, e: &str| run1(a(AssertKind::Header, "content-type", op, e), 200, &h, "", 1.0, 0);
        let r = t(AssertOp::Contains, "json");
        pass(&r);
        assert_eq!(r.name, "Header content-type contains json");
        pass(&t(AssertOp::Equals, "application/json; charset=utf-8"));
        fail(&t(AssertOp::Equals, "application/json"), "Got \"application/json; charset=utf-8\"");
        pass(&t(AssertOp::NotEquals, "text/html"));
        pass(&t(AssertOp::NotContains, "xml"));
        fail(&t(AssertOp::NotContains, "json"), "Got \"application/json; charset=utf-8\"");
        // value comparison is case-sensitive
        fail(&t(AssertOp::Contains, "JSON"), "Got \"application/json; charset=utf-8\"");
        pass(&t(AssertOp::Matches, "^application/\\w+;"));
        fail(&t(AssertOp::Matches, "^text"), "Got \"application/json; charset=utf-8\"");
        pass(&t(AssertOp::Exists, ""));
        fail(&t(AssertOp::NotExists, ""), "Header is present");
        // names are case-insensitive
        let up = run1(a(AssertKind::Header, "CONTENT-TYPE", AssertOp::Exists, ""), 200, &h, "", 1.0, 0);
        pass(&up);
        assert_eq!(up.name, "Header CONTENT-TYPE exists");
        pass(&run1(a(AssertKind::Header, "x-n", AssertOp::GreaterThan, "10"), 200, &h, "", 1.0, 0));
        fail(
            &run1(a(AssertKind::Header, "content-type", AssertOp::GreaterThan, "10"), 200, &h, "", 1.0, 0),
            "Value is not a number: Got \"application/json; charset=utf-8\"",
        );
    }

    #[test]
    fn header_absent() {
        let h = [("A", "1")];
        let t = |op| run1(a(AssertKind::Header, "etag", op, "x"), 200, &h, "", 1.0, 0);
        fail(&t(AssertOp::Exists), "Header not present");
        pass(&t(AssertOp::NotExists));
        fail(&t(AssertOp::Equals), "Header not present");
        fail(&t(AssertOp::Contains), "Header not present");
        pass(&t(AssertOp::NotEquals));
        pass(&t(AssertOp::NotContains));
        let r = run1(a(AssertKind::Header, "", AssertOp::Exists, ""), 200, &h, "", 1.0, 0);
        fail(&r, "No header name set");
    }

    #[test]
    fn header_first_match_wins() {
        let h = [("Set-Cookie", "a=1"), ("set-cookie", "b=2")];
        pass(&run1(a(AssertKind::Header, "Set-Cookie", AssertOp::Equals, "a=1"), 200, &h, "", 1.0, 0));
    }

    #[test]
    fn body_cases() {
        let t = |op, e: &str, body: &str| run1(a(AssertKind::Body, "", op, e), 200, &[], body, 1.0, 0);
        let r = t(AssertOp::NotContains, "error", "all good");
        pass(&r);
        assert_eq!(r.name, "Body does not contain \"error\"");
        fail(&t(AssertOp::NotContains, "error", "an error here"), "Got \"an error here\"");
        pass(&t(AssertOp::Contains, "ok", "status ok"));
        fail(&t(AssertOp::Contains, "ok", "nope"), "Got \"nope\"");
        pass(&t(AssertOp::Equals, "hello", "hello\n"));
        pass(&t(AssertOp::Equals, "hello\n", "hello"));
        fail(&t(AssertOp::Equals, "hello", "Hello"), "Got \"Hello\"");
        pass(&t(AssertOp::NotEquals, "a", "b"));
        pass(&t(AssertOp::Matches, "^\\d+$", "12345"));
        fail(&t(AssertOp::Matches, "^\\d+$", "12a45"), "Got \"12a45\"");
        pass(&t(AssertOp::Exists, "", "x"));
        fail(&t(AssertOp::Exists, "", ""), "Body is empty");
        pass(&t(AssertOp::NotExists, "", ""));
        fail(&t(AssertOp::NotExists, "", "x"), "Body is not empty");
        fail(&t(AssertOp::LessThan, "5", "3"), "Less than is not supported for body");
        let r = t(AssertOp::Matches, "(", "x");
        assert!(!r.passed && r.message.starts_with("Invalid pattern"), "{}", r.message);
    }

    #[test]
    fn body_lossy_and_truncation() {
        let h: Vec<(String, String)> = vec![];
        let view = ResponseView { status: 200, headers: &h, body: &[b'a', 0xff, b'b'], duration_ms: 1.0, size: 3 };
        let r = run_assertions(&[a(AssertKind::Body, "", AssertOp::Contains, "a\u{fffd}b")], &view);
        assert!(r[0].passed);
        let long = "x".repeat(500);
        let r = run1(a(AssertKind::Body, "", AssertOp::Equals, "y"), 200, &[], &long, 1.0, 0);
        assert!(!r.passed);
        assert_eq!(r.message.chars().count(), "Got \"".len() + 120 + 1 + 1);
        assert!(r.message.ends_with("…\""));
    }

    const DOC: &str = r#"{"data":{"id":7,"name":"Neko","ok":true,"none":null,"price":1.0,
        "tags":["a","b",3],"items":[{"id":10,"t":"x"},{"id":11}],"s":"42"},"a.b":1,"list":[[1,2],[3]]}"#;

    #[test]
    fn json_paths() {
        let r = json("data.id", AssertOp::Equals, "7", DOC);
        pass(&r);
        assert_eq!(r.name, "JSON data.id equals 7");
        pass(&json("$.data.id", AssertOp::Equals, "7", DOC));
        pass(&json("$data.id", AssertOp::Equals, "7", DOC));
        pass(&json("data.items[0].id", AssertOp::Equals, "10", DOC));
        pass(&json("$.data.items[1].id", AssertOp::Equals, "11", DOC));
        pass(&json("data.items.1.id", AssertOp::Equals, "11", DOC));
        pass(&json("data.items.0.t", AssertOp::Equals, "x", DOC));
        pass(&json("list[0][1]", AssertOp::Equals, "2", DOC));
        pass(&json("['a.b']", AssertOp::Equals, "1", DOC));
        pass(&json("data[\"name\"]", AssertOp::Equals, "Neko", DOC));
        pass(&json("$", AssertOp::Exists, "", DOC));
        pass(&json("", AssertOp::Exists, "", DOC));
        pass(&json("[0]", AssertOp::Equals, "1", "[1,2]"));
        pass(&json("0", AssertOp::Equals, "1", "[1,2]"));
    }

    #[test]
    fn json_exists() {
        pass(&json("data.id", AssertOp::Exists, "", DOC));
        pass(&json("data.none", AssertOp::Exists, "", DOC)); // null is present
        fail(&json("data.nope", AssertOp::Exists, "", DOC), "No value at data.nope");
        fail(&json("data.items[5]", AssertOp::Exists, "", DOC), "No value at data.items[5]");
        fail(&json("data.id.deeper", AssertOp::Exists, "", DOC), "No value at data.id.deeper");
        pass(&json("data.nope", AssertOp::NotExists, "", DOC));
        fail(&json("data.id", AssertOp::NotExists, "", DOC), "Got \"7\"");
    }

    #[test]
    fn json_equals_semantics() {
        pass(&json("data.price", AssertOp::Equals, "1", DOC));
        pass(&json("data.id", AssertOp::Equals, "7.0", DOC));
        pass(&json("data.ok", AssertOp::Equals, "true", DOC));
        fail(&json("data.ok", AssertOp::Equals, "false", DOC), "Got \"true\"");
        pass(&json("data.none", AssertOp::Equals, "null", DOC));
        pass(&json("data.s", AssertOp::Equals, "42", DOC));
        fail(&json("data.s", AssertOp::Equals, "42.0", DOC), "Got \"42\"");
        fail(&json("data.id", AssertOp::Equals, "8", DOC), "Got \"7\"");
        pass(&json("data.id", AssertOp::NotEquals, "8", DOC));
        fail(&json("data.id", AssertOp::NotEquals, "7", DOC), "Got \"7\"");
        pass(&json("data.tags", AssertOp::Equals, "[\"a\", \"b\", 3]", DOC));
        pass(&json("data.items[1]", AssertOp::Equals, "{\"id\":11}", DOC));
        fail(&json("data.nope", AssertOp::Equals, "1", DOC), "No value at data.nope");
        pass(&json("data.nope", AssertOp::NotEquals, "1", DOC));
        pass(&json("data.id", AssertOp::Equals, "abc", "{\"data\":{\"id\":\"abc\"}}"));
    }

    #[test]
    fn json_contains_semantics() {
        pass(&json("data.name", AssertOp::Contains, "ek", DOC));
        fail(&json("data.name", AssertOp::Contains, "zzz", DOC), "Got \"Neko\"");
        pass(&json("data.name", AssertOp::NotContains, "zzz", DOC));
        pass(&json("data.tags", AssertOp::Contains, "b", DOC));
        pass(&json("data.tags", AssertOp::Contains, "3", DOC));
        fail(&json("data.tags", AssertOp::Contains, "c", DOC), "Got \"[\"a\",\"b\",3]\"");
        pass(&json("data.tags", AssertOp::NotContains, "c", DOC));
        pass(&json("data", AssertOp::Contains, "id", DOC));
        pass(&json("data.nope", AssertOp::NotContains, "x", DOC));
        fail(&json("data.nope", AssertOp::Contains, "x", DOC), "No value at data.nope");
    }

    #[test]
    fn json_numeric_and_matches() {
        pass(&json("data.id", AssertOp::LessThan, "8", DOC));
        fail(&json("data.id", AssertOp::LessThan, "7", DOC), "Got \"7\"");
        pass(&json("data.id", AssertOp::GreaterThan, "6.5", DOC));
        pass(&json("data.s", AssertOp::GreaterThan, "41", DOC));
        fail(
            &json("data.name", AssertOp::LessThan, "8", DOC),
            "Value is not a number: Got \"Neko\"",
        );
        fail(
            &json("data.id", AssertOp::LessThan, "big", DOC),
            "Expected value \"big\" is not a number",
        );
        pass(&json("data.name", AssertOp::Matches, "^N.k[a-z]$", DOC));
        pass(&json("data.id", AssertOp::Matches, "^\\d$", DOC));
        fail(&json("data.name", AssertOp::Matches, "^\\d+$", DOC), "Got \"Neko\"");
    }

    #[test]
    fn json_invalid_body_and_path() {
        fail(&json("a", AssertOp::Exists, "", "<html>"), "The body is not JSON");
        fail(&json("a", AssertOp::Equals, "1", ""), "The body is not JSON");
        let r = json("a[x]", AssertOp::Exists, "", "{}");
        assert!(!r.passed && r.message.starts_with("Bad path"), "{}", r.message);
        let r = json("a[1", AssertOp::Exists, "", "{}");
        assert!(!r.passed && r.message.contains("missing ]"));
    }

    #[test]
    fn time_and_size() {
        let t = |op, e: &str, ms: f64| run1(a(AssertKind::Time, "", op, e), 200, &[], "", ms, 0);
        let r = t(AssertOp::LessThan, "500", 120.4);
        pass(&r);
        assert_eq!(r.name, "Response time less than 500 ms");
        fail(&t(AssertOp::LessThan, "500", 812.0), "Got 812 ms");
        fail(&t(AssertOp::LessThan, "500", 812.37), "Got 812.37 ms");
        pass(&t(AssertOp::GreaterThan, "100", 120.0));
        pass(&t(AssertOp::Equals, "120", 120.2));
        pass(&t(AssertOp::NotEquals, "5", 120.2));
        fail(&t(AssertOp::Contains, "1", 120.0), "Contains is not supported for time");
        fail(&t(AssertOp::Exists, "", 120.0), "Exists is not supported for time");
        fail(&t(AssertOp::LessThan, "fast", 1.0), "Expected value \"fast\" is not a number");
        let s = |op, e: &str, size: u64| run1(a(AssertKind::Size, "", op, e), 200, &[], "", 1.0, size);
        let r = s(AssertOp::LessThan, "1000", 10);
        pass(&r);
        assert_eq!(r.name, "Response size less than 1000 bytes");
        fail(&s(AssertOp::GreaterThan, "1000", 10), "Got 10 bytes");
        pass(&s(AssertOp::Equals, "10", 10));
        fail(&s(AssertOp::Matches, "1", 10), "Matches is not supported for size");
    }

    #[test]
    fn names() {
        let n = |t: Assertion| run1(t, 200, &[], "", 1.0, 0).name;
        assert_eq!(n(a(AssertKind::Status, "", AssertOp::Equals, "2xx")), "Status equals 2xx");
        assert_eq!(n(a(AssertKind::Json, "x.y", AssertOp::NotExists, "")), "JSON x.y does not exist");
        assert_eq!(n(a(AssertKind::Body, "", AssertOp::Exists, "")), "Body exists");
        assert_eq!(n(a(AssertKind::Time, "", AssertOp::GreaterThan, "5")), "Response time greater than 5 ms");
    }

    #[test]
    fn never_panics_on_odd_input() {
        for op in [
            AssertOp::Equals, AssertOp::NotEquals, AssertOp::Contains, AssertOp::NotContains,
            AssertOp::Exists, AssertOp::NotExists, AssertOp::LessThan, AssertOp::GreaterThan, AssertOp::Matches,
        ] {
            for kind in [
                AssertKind::Status, AssertKind::Header, AssertKind::Body,
                AssertKind::Json, AssertKind::Time, AssertKind::Size,
            ] {
                for target in ["", "[", "]]", "a..b", "é[0]", "$.", "[999999999999999999999]"] {
                    let _ = run1(a(kind, target, op, "\u{1F600}("), 0, &[("", "")], "{\"é\":[1]}", f64::NAN, 0);
                }
            }
        }
    }

    // ---- mini regex

    fn rx(p: &str, t: &str) -> bool {
        regex_matches(p, t).unwrap()
    }

    #[test]
    fn regex_basics() {
        assert!(rx("abc", "xxabcxx"));
        assert!(!rx("abc", "xxabxcxx"));
        assert!(rx("^abc", "abcd"));
        assert!(!rx("^abc", "xabc"));
        assert!(rx("abc$", "xabc"));
        assert!(!rx("abc$", "abcx"));
        assert!(rx("^$", ""));
        assert!(rx("a.c", "abc"));
        assert!(!rx("a.c", "a\nc"));
        assert!(rx("", "anything"));
    }

    #[test]
    fn regex_quantifiers() {
        assert!(rx("^ab*c$", "ac"));
        assert!(rx("^ab*c$", "abbbc"));
        assert!(!rx("^ab+c$", "ac"));
        assert!(rx("^ab+c$", "abc"));
        assert!(rx("^colou?r$", "color"));
        assert!(rx("^colou?r$", "colour"));
        assert!(!rx("^colou?r$", "colouur"));
        assert!(rx("^a{3}$", "aaa"));
        assert!(!rx("^a{3}$", "aa"));
        assert!(rx("^a{2,}$", "aaaaa"));
        assert!(rx("^a{1,2}$", "aa"));
        assert!(!rx("^a{1,2}$", "aaa"));
        assert!(rx("a{x", "a{x")); // not a quantifier: literal
        assert!(rx("^.*$", "whatever"));
        assert!(rx("^(ab)+$", "ababab"));
        assert!(rx("^(a*)*$", "aaaa")); // empty iteration does not loop
        assert!(rx("^(a|b)*c$", "abbac"));
    }

    #[test]
    fn regex_classes() {
        assert!(rx("^[abc]+$", "cab"));
        assert!(!rx("^[abc]+$", "cad"));
        assert!(rx("^[a-z0-9_-]+$", "foo-bar_9"));
        assert!(rx("^[^0-9]+$", "abc"));
        assert!(!rx("^[^0-9]+$", "ab1"));
        assert!(rx("^\\d{3}-\\d{4}$", "555-1234"));
        assert!(rx("^\\w+@\\w+\\.com$", "me@site.com"));
        assert!(!rx("^\\w+@\\w+\\.com$", "me@site-com"));
        assert!(rx("\\s", "a b"));
        assert!(!rx("\\S", "  "));
        assert!(rx("^[\\d.]+$", "1.25"));
        assert!(rx("^[]a]+$", "]a")); // leading ] is literal
        assert!(rx("\\.", "a.b"));
        assert!(!rx("^\\.$", "a"));
        assert!(rx("^\\D+$", "abc"));
        assert!(rx("^\\W$", "!"));
    }

    #[test]
    fn regex_groups_and_alternation() {
        assert!(rx("^(cat|dog)$", "dog"));
        assert!(!rx("^(cat|dog)$", "cow"));
        assert!(rx("^cat|dog$", "cat food"));
        assert!(rx("^(?:a|b)c$", "bc"));
        assert!(rx("^(a|ab)(c|bcd)$", "abcd")); // needs backtracking
        assert!(rx("^(\\d+)\\.(\\d+)$", "10.25"));
        assert!(rx("application/(json|xml)", "application/json; charset=utf-8"));
    }

    #[test]
    fn regex_unicode() {
        assert!(rx("^é+$", "éé"));
        assert!(rx("^.$", "日"));
        assert!(rx("^\\w+$", "naïve"));
    }

    #[test]
    fn regex_errors() {
        for p in ["(", ")", "[abc", "*a", "a**", "\\", "(?=a)", "[z-a]", "a+?", "^*"] {
            assert!(regex_matches(p, "a").is_err(), "{p} should be invalid");
        }
    }

    #[test]
    fn regex_catastrophic_is_bounded() {
        let text = "a".repeat(40) + "b";
        // Would take forever without the step limit; must return (any result).
        let r = regex_matches("^(a+)+$", &text);
        assert!(r.is_ok() || r.is_err());
    }
}
