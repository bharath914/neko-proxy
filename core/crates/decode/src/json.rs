use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonTokenKind {
    Punct,
    Key,
    String,
    Number,
    Literal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonToken {
    pub kind: JsonTokenKind,
    pub text: String,
}

/// One printed line. `fold_end` is the index of the matching closing line
/// for a container opener, so the UI can collapse without parsing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonLine {
    pub depth: u32,
    pub tokens: Vec<JsonToken>,
    pub fold_end: i32,
}

const MAX_STRING: usize = 4000;

/// Pretty-print JSON as token lines. Returns `None` if `text` is not JSON.
/// The bool is true when output was cut at `max_lines`.
pub fn json_outline(text: &str, max_lines: usize) -> Option<(Vec<JsonLine>, bool)> {
    let value: Value = serde_json::from_str(text).ok()?;
    let mut w = Writer {
        lines: Vec::new(),
        max: max_lines,
        truncated: false,
    };
    w.value(&value, 0, None, false);
    Some((w.lines, w.truncated))
}

struct Writer {
    lines: Vec<JsonLine>,
    max: usize,
    truncated: bool,
}

fn tok(kind: JsonTokenKind, text: impl Into<String>) -> JsonToken {
    JsonToken {
        kind,
        text: text.into(),
    }
}

fn quoted(s: &str) -> String {
    let cut = if s.chars().count() > MAX_STRING {
        let mut t: String = s.chars().take(MAX_STRING).collect();
        t.push('…');
        t
    } else {
        s.to_string()
    };
    serde_json::to_string(&cut).unwrap_or_else(|_| "\"\"".into())
}

impl Writer {
    fn full(&self) -> bool {
        self.lines.len() >= self.max
    }

    fn push(&mut self, depth: u32, tokens: Vec<JsonToken>) -> usize {
        if self.full() {
            self.truncated = true;
            return self.lines.len();
        }
        self.lines.push(JsonLine {
            depth,
            tokens,
            fold_end: -1,
        });
        self.lines.len() - 1
    }

    fn lead(key: Option<&str>) -> Vec<JsonToken> {
        match key {
            Some(k) => vec![
                tok(JsonTokenKind::Key, quoted(k)),
                tok(JsonTokenKind::Punct, ": "),
            ],
            None => Vec::new(),
        }
    }

    fn value(&mut self, v: &Value, depth: u32, key: Option<&str>, comma: bool) {
        if self.full() {
            self.truncated = true;
            return;
        }
        let mut t = Self::lead(key);
        let tail = |t: &mut Vec<JsonToken>| {
            if comma {
                t.push(tok(JsonTokenKind::Punct, ","));
            }
        };
        match v {
            Value::Null => {
                t.push(tok(JsonTokenKind::Literal, "null"));
                tail(&mut t);
                self.push(depth, t);
            }
            Value::Bool(b) => {
                t.push(tok(JsonTokenKind::Literal, b.to_string()));
                tail(&mut t);
                self.push(depth, t);
            }
            Value::Number(n) => {
                t.push(tok(JsonTokenKind::Number, n.to_string()));
                tail(&mut t);
                self.push(depth, t);
            }
            Value::String(s) => {
                t.push(tok(JsonTokenKind::String, quoted(s)));
                tail(&mut t);
                self.push(depth, t);
            }
            Value::Array(items) => {
                if items.is_empty() {
                    t.push(tok(JsonTokenKind::Punct, "[]"));
                    tail(&mut t);
                    self.push(depth, t);
                    return;
                }
                t.push(tok(JsonTokenKind::Punct, "["));
                let open = self.push(depth, t);
                let last = items.len() - 1;
                for (i, item) in items.iter().enumerate() {
                    self.value(item, depth + 1, None, i != last);
                }
                self.close(open, depth, "]", comma);
            }
            Value::Object(map) => {
                if map.is_empty() {
                    t.push(tok(JsonTokenKind::Punct, "{}"));
                    tail(&mut t);
                    self.push(depth, t);
                    return;
                }
                t.push(tok(JsonTokenKind::Punct, "{"));
                let open = self.push(depth, t);
                let last = map.len() - 1;
                for (i, (k, item)) in map.iter().enumerate() {
                    self.value(item, depth + 1, Some(k), i != last);
                }
                self.close(open, depth, "}", comma);
            }
        }
    }

    fn close(&mut self, open: usize, depth: u32, bracket: &str, comma: bool) {
        let mut t = vec![tok(JsonTokenKind::Punct, bracket)];
        if comma {
            t.push(tok(JsonTokenKind::Punct, ","));
        }
        if self.full() {
            self.truncated = true;
            return;
        }
        let end = self.push(depth, t);
        if let Some(l) = self.lines.get_mut(open) {
            l.fold_end = end as i32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(lines: &[JsonLine]) -> Vec<String> {
        lines
            .iter()
            .map(|l| {
                format!(
                    "{}{}",
                    "  ".repeat(l.depth as usize),
                    l.tokens.iter().map(|t| t.text.as_str()).collect::<String>()
                )
            })
            .collect()
    }

    #[test]
    fn prints_and_folds() {
        let (lines, cut) = json_outline(
            r#"{"id":"u_1","tags":["a",2],"meta":{},"ok":true,"n":null}"#,
            100,
        )
        .unwrap();
        assert!(!cut);
        assert_eq!(
            render(&lines),
            [
                "{",
                "  \"id\": \"u_1\",",
                "  \"tags\": [",
                "    \"a\",",
                "    2",
                "  ],",
                "  \"meta\": {},",
                "  \"ok\": true,",
                "  \"n\": null",
                "}",
            ]
        );
        assert_eq!(lines[0].fold_end, 9);
        assert_eq!(lines[2].fold_end, 5);
        assert_eq!(lines[1].fold_end, -1);
    }

    #[test]
    fn key_order_is_preserved() {
        let (lines, _) = json_outline(r#"{"z":1,"a":2}"#, 10).unwrap();
        assert!(render(&lines)[1].contains("\"z\""));
    }

    #[test]
    fn truncates_at_limit() {
        let big = format!(
            "[{}]",
            (0..100)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        let (lines, cut) = json_outline(&big, 10).unwrap();
        assert!(cut);
        assert!(lines.len() <= 10);
    }

    #[test]
    fn rejects_non_json() {
        assert!(json_outline("not json", 10).is_none());
    }

    #[test]
    fn scalars_work() {
        let (lines, _) = json_outline("\"x\"", 10).unwrap();
        assert_eq!(render(&lines), ["\"x\""]);
    }
}
