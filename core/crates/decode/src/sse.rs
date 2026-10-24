//! Bounded WHATWG event-stream decoding. Incomplete final events are not dispatched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseEvent {
    pub event: String,
    pub id: String,
    pub data: String,
    pub retry_ms: Option<u64>,
}

pub fn parse_sse(text: &str, limit: usize) -> (Vec<SseEvent>, bool) {
    let mut events = Vec::new();
    let mut id = String::new();
    let mut event = String::new();
    let mut data = String::new();
    let mut has_data = false;
    let mut retry_ms = None;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // CRLF is one delimiter; lone CR and LF are delimiters too.
    let mut lines = text.split_inclusive(['\r', '\n']).peekable();
    while let Some(line) = lines.next() {
        if !line.ends_with(['\r', '\n']) { break; }
        if line.ends_with('\r') && lines.peek() == Some(&"\n") { lines.next(); }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            if has_data {
                if events.len() == limit { return (events, true); }
                data.pop(); // the last data newline
                events.push(SseEvent { event: if event.is_empty() { "message".into() } else { std::mem::take(&mut event) }, id: id.clone(), data: std::mem::take(&mut data), retry_ms });
            }
            event.clear(); data.clear(); has_data = false;
            continue;
        }
        if line.starts_with(':') { continue; }
        let (key, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match key {
            "data" => { data.push_str(value); data.push('\n'); has_data = true; }
            "event" => event = value.into(),
            "id" if !value.contains('\0') => id = value.into(),
            "retry" if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) => { if let Ok(n) = value.parse() { retry_ms = Some(n); } }
            _ => {}
        }
    }
    (events, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiline_crlf_ids_comments_and_retry() {
        let (e, cut) = parse_sse("\u{feff}: ping\r\nid: 4\r\nretry: 300\r\nevent: update\r\ndata: one\r\ndata: two\r\n\r\ndata:\n\nid: bad\0\ndata: next\n\ndata: unfinished", 10);
        assert!(!cut); assert_eq!(e.len(), 3);
        assert_eq!(e[0], SseEvent { event: "update".into(), id: "4".into(), data: "one\ntwo".into(), retry_ms: Some(300) });
        assert_eq!(e[1].data, ""); assert_eq!(e[2].id, "4");
    }
    #[test]
    fn bounded_and_lone_cr() {
        let (e, cut) = parse_sse("data: a\r\rdata: b\r\r", 1);
        assert_eq!(e.len(), 1); assert!(cut);
        assert!(parse_sse("data: incomplete\n", 10).0.is_empty());
    }
}
