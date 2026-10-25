//! Bulk edit: a table as `name: value` lines, one per row.

use crate::KeyValue;

/// A line starting with `//` is a disabled row; a line with no colon is a name
/// with an empty value; blank lines are skipped.
pub fn bulk_parse(text: &str) -> Vec<KeyValue> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let (disabled, line) = match line.strip_prefix("//") {
                Some(rest) => (true, rest.trim_start()),
                None => (false, line),
            };
            let (name, value) = match line.split_once(':') {
                Some((n, v)) => (n.trim(), v.trim()),
                None => (line, ""),
            };
            if name.is_empty() {
                return None;
            }
            Some(KeyValue {
                name: name.to_string(),
                value: value.to_string(),
                disabled,
                ..Default::default()
            })
        })
        .collect()
}

pub fn bulk_format(rows: &[KeyValue]) -> String {
    rows.iter()
        .filter(|r| !r.name.is_empty())
        .map(|r| {
            let line = format!("{}: {}", r.name, r.value);
            if r.disabled {
                format!("// {line}")
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_rows_with_disabled_and_colons_in_values() {
        let rows = bulk_parse("Accept: */*\n// X-Off: 1\n\nurl: http://x:80/a\nflag");
        assert_eq!(rows.len(), 4);
        assert!(rows[1].disabled && rows[1].name == "X-Off");
        assert_eq!(rows[2].value, "http://x:80/a");
        assert_eq!(rows[3], KeyValue::new("flag", ""));
        assert_eq!(
            bulk_format(&rows),
            "Accept: */*\n// X-Off: 1\nurl: http://x:80/a\nflag: "
        );
    }
}
