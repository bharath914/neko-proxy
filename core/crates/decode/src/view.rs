use crate::codec::{decode_content, DEFAULT_DECODE_LIMIT};
use crate::html::{html_view, Block, SourceLine};
use crate::json::{json_outline, JsonLine};
use crate::kind::{classify, parse_form, sniff_image, Kind};

pub struct ViewOptions {
    pub text_limit: usize,
    pub json_lines: usize,
    pub html_blocks: usize,
    pub image_limit: usize,
    pub decode_limit: usize,
}

impl Default for ViewOptions {
    fn default() -> Self {
        ViewOptions {
            text_limit: 512 * 1024,
            json_lines: 20_000,
            html_blocks: 4_000,
            image_limit: 4 * 1024 * 1024,
            decode_limit: DEFAULT_DECODE_LIMIT,
        }
    }
}

/// A body ready to render.
#[derive(Clone, Debug)]
pub struct BodyView {
    pub kind: Kind,
    pub content_type: String,
    pub content_encoding: String,
    pub wire_size: u64,
    pub decoded_size: u64,
    pub capture_truncated: bool,
    pub preview_truncated: bool,
    pub text: String,
    pub json: Vec<JsonLine>,
    pub html: Vec<Block>,
    pub html_title: String,
    pub source: Vec<SourceLine>,
    pub form: Vec<(String, String)>,
    pub image: Vec<u8>,
    pub note: String,
    pub events: Vec<crate::SseEvent>,
    pub protocol: crate::ProtocolView,
}

impl BodyView {
    fn empty(content_type: &str, wire_size: u64) -> BodyView {
        BodyView {
            kind: Kind::Empty,
            content_type: content_type.to_string(),
            content_encoding: String::new(),
            wire_size,
            decoded_size: 0,
            capture_truncated: false,
            preview_truncated: false,
            text: String::new(),
            json: Vec::new(),
            html: Vec::new(),
            html_title: String::new(),
            source: Vec::new(),
            form: Vec::new(),
            image: Vec::new(),
            note: String::new(),
            events: Vec::new(),
            protocol: crate::ProtocolView::default(),
        }
    }
}

fn charset(content_type: &str) -> Option<String> {
    content_type.split(';').skip(1).find_map(|p| {
        let (k, v) = p.split_once('=')?;
        k.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| v.trim().trim_matches('"').to_ascii_lowercase())
    })
}

fn to_text(bytes: &[u8], content_type: &str) -> String {
    match charset(content_type).as_deref() {
        Some("iso-8859-1") | Some("latin1") | Some("windows-1252") | Some("us-ascii") => {
            bytes.iter().map(|&b| b as char).collect()
        }
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

pub fn human_size(n: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else if v < 10.0 {
        format!("{v:.1} {}", UNITS[i])
    } else {
        format!("{v:.0} {}", UNITS[i])
    }
}

pub fn build_view(
    content_type: &str,
    content_encoding: &str,
    wire: &[u8],
    wire_size: u64,
    capture_truncated: bool,
    opts: &ViewOptions,
) -> BodyView {
    if wire.is_empty() && wire_size == 0 {
        return BodyView::empty(content_type, wire_size);
    }
    let decoded = decode_content(content_encoding, wire, opts.decode_limit);
    let mut notes: Vec<String> = Vec::new();
    if let Some(p) = &decoded.problem {
        notes.push(p.clone());
    }
    if capture_truncated {
        notes.push(format!(
            "Only the first {} of {} was captured.",
            human_size(wire.len() as u64),
            human_size(wire_size)
        ));
    }
    let body = decoded.bytes;
    let mut view = BodyView {
        kind: classify(content_type, &body),
        content_type: content_type.to_string(),
        content_encoding: decoded.applied.join(", "),
        wire_size,
        decoded_size: body.len() as u64,
        capture_truncated,
        preview_truncated: false,
        text: String::new(),
        json: Vec::new(),
        html: Vec::new(),
        html_title: String::new(),
        source: Vec::new(),
        form: Vec::new(),
        image: Vec::new(),
        note: String::new(),
            events: Vec::new(),
            protocol: crate::ProtocolView::default(),
    };

    view.protocol = crate::protocol_view(content_type, &body);

    match view.kind {
        Kind::Empty => {}
        Kind::Image => {
            let mime = sniff_image(&body).unwrap_or("image");
            if body.len() <= opts.image_limit {
                view.image = body;
            } else {
                view.preview_truncated = true;
                notes.push(format!("{mime}, too large to preview"));
            }
        }
        Kind::Binary => {
            notes.push(format!("Binary data, {}", human_size(body.len() as u64)));
        }
        Kind::Json | Kind::Text | Kind::Html | Kind::Form => {
            let cut = body.len() > opts.text_limit;
            let slice = if cut {
                &body[..opts.text_limit]
            } else {
                &body[..]
            };
            let text = if content_type.split(';').next().unwrap_or("").trim().eq_ignore_ascii_case("text/event-stream") {
                String::from_utf8_lossy(slice).into_owned()
            } else { to_text(slice, content_type) };
            if cut {
                view.preview_truncated = true;
                notes.push(format!(
                    "Showing the first {} of {}.",
                    human_size(opts.text_limit as u64),
                    human_size(body.len() as u64)
                ));
            }
            match view.kind {
                Kind::Json => match json_outline(&text, opts.json_lines) {
                    Some((lines, lines_cut)) if !cut => {
                        view.json = lines;
                        if lines_cut {
                            view.preview_truncated = true;
                            notes.push("Large document, outline cut.".into());
                        }
                    }
                    _ => {
                        // Truncated or invalid: show it as text.
                        view.kind = Kind::Text;
                        if !cut && !capture_truncated {
                            notes.push("Not valid JSON.".into());
                        }
                    }
                },
                Kind::Form => view.form = parse_form(&text),
                // Indexing asks for no outline and no preview: skip the scan.
                Kind::Html if opts.json_lines > 0 || opts.html_blocks > 0 => {
                    let h = html_view(&text, opts.json_lines, opts.html_blocks);
                    view.html = h.blocks;
                    view.html_title = h.title;
                    view.source = h.source;
                    if h.blocks_cut || h.lines_cut {
                        view.preview_truncated = true;
                        notes.push("Large page, preview cut.".into());
                    }
                }
                _ => {}
            }
            if content_type.split(';').next().unwrap_or("").trim().eq_ignore_ascii_case("text/event-stream") {
                let (events, events_cut) = crate::parse_sse(&text, opts.json_lines);
                view.events = events;
                view.preview_truncated |= events_cut;
                if events_cut { notes.push("Event limit reached.".into()); }
            }
            view.text = text;
        }
    }
    view.note = notes.join(" ");
    view
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_view_has_outline_and_text() {
        let v = build_view(
            "application/json",
            "",
            br#"{"a":1}"#,
            7,
            false,
            &ViewOptions::default(),
        );
        assert_eq!(v.kind, Kind::Json);
        assert_eq!(v.json.len(), 3);
        assert_eq!(v.text, r#"{"a":1}"#);
        assert_eq!(v.decoded_size, 7);
    }

    #[test]
    fn html_view_has_preview_source_and_text() {
        let v = build_view(
            "text/html; charset=utf-8",
            "",
            b"<title>T</title><h1>Hello</h1><p>world</p>",
            42,
            false,
            &ViewOptions::default(),
        );
        assert_eq!(v.kind, Kind::Html);
        assert_eq!(v.html_title, "T");
        assert_eq!(v.html.len(), 2);
        assert!(!v.source.is_empty());
        assert!(v.text.contains("<h1>"));
    }

    #[test]
    fn truncated_json_degrades_to_text() {
        let v = build_view(
            "application/json",
            "",
            br#"{"a":[1,2"#,
            100,
            true,
            &ViewOptions::default(),
        );
        assert_eq!(v.kind, Kind::Text);
        assert!(v.note.contains("Only the first"));
    }

    #[test]
    fn image_bytes_are_returned() {
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
        let v = build_view(
            "image/png",
            "",
            png,
            png.len() as u64,
            false,
            &ViewOptions::default(),
        );
        assert_eq!(v.kind, Kind::Image);
        assert_eq!(v.image, png);
    }

    #[test]
    fn latin1_text() {
        let v = build_view(
            "text/plain; charset=iso-8859-1",
            "",
            b"caf\xe9",
            4,
            false,
            &ViewOptions::default(),
        );
        assert_eq!(v.text, "café");
    }

    #[test]
    fn sizes() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(94210), "92 KB");
    }
}

#[cfg(test)] mod sse_charset_tests {
    #[test] fn event_stream_is_always_utf8() {
        let body="data: café\n\n".as_bytes();
        let view=super::build_view("text/event-stream; charset=latin1","",body,body.len() as u64,false,&super::ViewOptions::default());
        assert_eq!(view.events[0].data,"café");
    }
}
