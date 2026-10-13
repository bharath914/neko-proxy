use percent_encoding::percent_decode_str;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Empty,
    Json,
    Text,
    Html,
    Form,
    Image,
    Binary,
}

/// Image mime type from magic bytes.
pub fn sniff_image(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if b.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("image/webp")
    } else if b.len() >= 12 && &b[4..8] == b"ftyp" && (&b[8..12] == b"avif" || &b[8..12] == b"avis")
    {
        Some("image/avif")
    } else if b.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        Some("image/x-icon")
    } else if b.starts_with(b"BM") && b.len() > 14 {
        Some("image/bmp")
    } else {
        None
    }
}

fn essence(ct: &str) -> String {
    ct.split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

fn looks_like_text(b: &[u8]) -> bool {
    let head = &b[..b.len().min(8192)];
    if head.contains(&0) {
        return false;
    }
    // Allow a partial multi-byte sequence at the cut.
    match std::str::from_utf8(head) {
        Ok(_) => true,
        Err(e) => e.error_len().is_none() && e.valid_up_to() + 4 > head.len(),
    }
}

/// Decide how to show a decoded body.
pub fn classify(content_type: &str, body: &[u8]) -> Kind {
    if body.is_empty() {
        return Kind::Empty;
    }
    let ct = essence(content_type);
    if ct.starts_with("image/") && ct != "image/svg+xml" {
        return Kind::Image;
    }
    if sniff_image(body).is_some() {
        return Kind::Image;
    }
    if ct == "application/json" || ct.ends_with("+json") || ct.ends_with("/json") {
        return Kind::Json;
    }
    if ct == "application/x-www-form-urlencoded" {
        return Kind::Form;
    }
    if ct == "text/html" || ct == "application/xhtml+xml" {
        return Kind::Html;
    }
    if ct.starts_with("text/")
        || ct.ends_with("+xml")
        || ct.ends_with("/xml")
        || ct == "application/javascript"
        || ct == "application/x-javascript"
        || ct == "image/svg+xml"
        || ct == "application/graphql"
    {
        return Kind::Text;
    }
    if !looks_like_text(body) {
        return Kind::Binary;
    }
    // Unlabelled or mislabelled text: look at the first non-space byte.
    let first = body.iter().find(|b| !b.is_ascii_whitespace()).copied();
    if matches!(first, Some(b'{') | Some(b'[')) {
        return Kind::Json;
    }
    if crate::html::looks_like_html(body) {
        return Kind::Html;
    }
    Kind::Text
}

/// Split `a=1&b=two%20words` into decoded pairs.
pub fn parse_form(s: &str) -> Vec<(String, String)> {
    s.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (decode_component(k), decode_component(v))
        })
        .collect()
}

fn decode_component(s: &str) -> String {
    let plus = s.replace('+', " ");
    percent_decode_str(&plus).decode_utf8_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies() {
        assert_eq!(classify("application/json", b"{}"), Kind::Json);
        assert_eq!(classify("", b"  [1,2]"), Kind::Json);
        assert_eq!(classify("text/html; charset=utf-8", b"<p>"), Kind::Html);
        assert_eq!(classify("", b"<!DOCTYPE html><html>"), Kind::Html);
        assert_eq!(classify("text/plain", b"<p>"), Kind::Text);
        assert_eq!(
            classify("application/octet-stream", b"\x00\x01\x02"),
            Kind::Binary
        );
        assert_eq!(
            classify("application/x-www-form-urlencoded", b"a=1"),
            Kind::Form
        );
        assert_eq!(classify("", b""), Kind::Empty);
        assert_eq!(
            classify("application/octet-stream", b"\x89PNG\r\n\x1a\n...."),
            Kind::Image
        );
        assert_eq!(classify("image/svg+xml", b"<svg/>"), Kind::Text);
        assert_eq!(classify("application/x-custom", b"plain words"), Kind::Text);
    }

    #[test]
    fn forms_decode() {
        let f = parse_form("a=1&b=two+words&c=%E2%9C%93&flag");
        assert_eq!(f[0], ("a".into(), "1".into()));
        assert_eq!(f[1], ("b".into(), "two words".into()));
        assert_eq!(f[2], ("c".into(), "✓".into()));
        assert_eq!(f[3], ("flag".into(), "".into()));
    }
}
