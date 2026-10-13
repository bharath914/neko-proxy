//! A lenient HTML scanner. It never executes or fetches anything: it turns
//! markup into highlighted source lines and a simplified, readable preview
//! (headings, paragraphs, lists, links, code). Scripts and styles are dropped.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Plain,
    Tag,
    Attr,
    Value,
    Comment,
    Punct,
    Entity,
}

#[derive(Clone, Debug)]
pub struct SourceToken {
    pub kind: SourceKind,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct SourceLine {
    pub tokens: Vec<SourceToken>,
}

pub const BOLD: u32 = 1;
pub const ITALIC: u32 = 2;
pub const CODE: u32 = 4;
pub const LINK: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading,
    ListItem,
    Quote,
    Code,
    Rule,
    Image,
}

#[derive(Clone, Debug)]
pub struct Span {
    pub text: String,
    pub style: u32,
    pub href: String,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub kind: BlockKind,
    /// Heading level 1-6, list or quote depth (0 based for lists).
    pub level: u32,
    /// List marker, e.g. a bullet or `3.`.
    pub marker: String,
    pub spans: Vec<Span>,
}

#[derive(Clone, Debug, Default)]
pub struct HtmlView {
    pub title: String,
    pub blocks: Vec<Block>,
    pub source: Vec<SourceLine>,
    pub blocks_cut: bool,
    pub lines_cut: bool,
}

/// First non-space bytes say "this is a page", for bodies with no useful type.
pub fn looks_like_html(body: &[u8]) -> bool {
    let head = &body[..body.len().min(512)];
    let start = head
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(0);
    let lower: Vec<u8> = head[start..]
        .iter()
        .take(16)
        .map(|b| b.to_ascii_lowercase())
        .collect();
    lower.starts_with(b"<!doctype html") || lower.starts_with(b"<html")
}

#[derive(Clone, Copy, Debug)]
enum Seg<'a> {
    Text(&'a str),
    Comment(&'a str),
    Decl(&'a str),
    Open(&'a str),
    Name(&'a str),
    AttrName(&'a str),
    Eq,
    AttrValue(&'a str),
    Ws(&'a str),
    Close(&'a str),
    Raw(&'a str),
}

fn is_tag_start(s: &str) -> bool {
    let b = s.as_bytes();
    match b.get(1) {
        Some(c) if c.is_ascii_alphabetic() => true,
        Some(b'/') => b.get(2).is_some_and(|c| c.is_ascii_alphabetic()),
        _ => false,
    }
}

/// A `<` that begins a tag, comment, declaration or processing instruction.
fn markup_at(b: &[u8], j: usize) -> bool {
    b[j] == b'<'
        && match b.get(j + 1) {
            Some(c) if c.is_ascii_alphabetic() => true,
            Some(b'/') => b.get(j + 2).is_some_and(|c| c.is_ascii_alphabetic()),
            Some(b'!') | Some(b'?') => true,
            _ => false,
        }
}

fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    let h = hay.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    (0..=h.len() - n.len()).find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n))
}

fn scan(s: &str) -> Vec<Seg<'_>> {
    let b = s.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let rest = &s[i..];
        if b[i] != b'<' {
            // Text runs until the next real tag start.
            let mut j = i + 1;
            while j < n && !markup_at(b, j) {
                j += 1;
            }
            // `j` sits on an ASCII byte or the end: a char boundary.
            out.push(Seg::Text(&s[i..j]));
            i = j;
            continue;
        }
        if rest.starts_with("<!--") {
            let end = rest.find("-->").map(|e| e + 3).unwrap_or(rest.len());
            out.push(Seg::Comment(&rest[..end]));
            i += end;
        } else if rest.starts_with("<!") || rest.starts_with("<?") {
            let end = rest.find('>').map(|e| e + 1).unwrap_or(rest.len());
            out.push(Seg::Decl(&rest[..end]));
            i += end;
        } else if is_tag_start(rest) {
            let closing = b[i + 1] == b'/';
            let open_len = if closing { 2 } else { 1 };
            out.push(Seg::Open(&s[i..i + open_len]));
            i += open_len;
            let ns = i;
            while i < n && !b[i].is_ascii_whitespace() && b[i] != b'>' && b[i] != b'/' {
                i += 1;
            }
            let name = &s[ns..i];
            out.push(Seg::Name(name));
            let mut self_closed = false;
            while i < n {
                let ws = i;
                while i < n && b[i].is_ascii_whitespace() {
                    i += 1;
                }
                if i > ws {
                    out.push(Seg::Ws(&s[ws..i]));
                }
                if i >= n {
                    break;
                }
                if s[i..].starts_with("/>") {
                    out.push(Seg::Close("/>"));
                    self_closed = true;
                    i += 2;
                    break;
                }
                if b[i] == b'>' {
                    out.push(Seg::Close(">"));
                    i += 1;
                    break;
                }
                if b[i] == b'/' {
                    out.push(Seg::Ws("/"));
                    i += 1;
                    continue;
                }
                let a = i;
                while i < n && !b[i].is_ascii_whitespace() && !matches!(b[i], b'=' | b'>' | b'/') {
                    i += 1;
                }
                if i == a {
                    // A stray `=`: make progress.
                    out.push(Seg::Eq);
                    i += 1;
                    continue;
                }
                out.push(Seg::AttrName(&s[a..i]));
                let ws = i;
                let mut k = i;
                while k < n && b[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k < n && b[k] == b'=' {
                    if k > ws {
                        out.push(Seg::Ws(&s[ws..k]));
                    }
                    out.push(Seg::Eq);
                    k += 1;
                    let ws2 = k;
                    while k < n && b[k].is_ascii_whitespace() {
                        k += 1;
                    }
                    if k > ws2 {
                        out.push(Seg::Ws(&s[ws2..k]));
                    }
                    let vs = k;
                    if k < n && (b[k] == b'"' || b[k] == b'\'') {
                        let q = b[k];
                        k += 1;
                        while k < n && b[k] != q {
                            k += 1;
                        }
                        k = (k + 1).min(n);
                    } else {
                        while k < n && !b[k].is_ascii_whitespace() && b[k] != b'>' {
                            k += 1;
                        }
                    }
                    out.push(Seg::AttrValue(&s[vs..k]));
                    i = k;
                }
            }
            if !closing && !self_closed {
                let lname = name.to_ascii_lowercase();
                if lname == "script" || lname == "style" {
                    let end_tag = format!("</{lname}");
                    let end = find_ci(&s[i..], &end_tag).unwrap_or(n - i);
                    if end > 0 {
                        out.push(Seg::Raw(&s[i..i + end]));
                    }
                    i += end;
                }
            }
        } else {
            out.push(Seg::Text(&s[i..i + 1]));
            i += 1;
        }
    }
    out
}

fn entity_char(name: &str) -> Option<&'static str> {
    Some(match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => " ",
        "copy" => "©",
        "reg" => "®",
        "trade" => "™",
        "mdash" => "—",
        "ndash" => "–",
        "hellip" => "…",
        "lsquo" => "‘",
        "rsquo" => "’",
        "ldquo" => "“",
        "rdquo" => "”",
        "bull" => "•",
        "middot" => "·",
        "rarr" => "→",
        "larr" => "←",
        "times" => "×",
        "euro" => "€",
        "pound" => "£",
        "laquo" => "«",
        "raquo" => "»",
        _ => return None,
    })
}

/// Length in bytes of a well-formed entity at the start of `s` (`&amp;`,
/// `&#39;`, `&#x1F600;`), or None.
fn entity_len(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    if b.first() != Some(&b'&') {
        return None;
    }
    let end = b[..b.len().min(12)].iter().position(|&c| c == b';')?;
    if end < 2 {
        return None;
    }
    let body = &s[1..end];
    let ok = if let Some(num) = body.strip_prefix('#') {
        let hex = num.strip_prefix(['x', 'X']);
        match hex {
            Some(h) => !h.is_empty() && h.bytes().all(|c| c.is_ascii_hexdigit()),
            None => !num.is_empty() && num.bytes().all(|c| c.is_ascii_digit()),
        }
    } else {
        body.bytes().all(|c| c.is_ascii_alphanumeric())
    };
    ok.then_some(end + 1)
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s.as_bytes()[i] == b'&' {
            if let Some(len) = entity_len(&s[i..]) {
                let body = &s[i + 1..i + len - 1];
                let decoded = if let Some(num) = body.strip_prefix('#') {
                    let code = match num.strip_prefix(['x', 'X']) {
                        Some(h) => u32::from_str_radix(h, 16).ok(),
                        None => num.parse::<u32>().ok(),
                    };
                    code.and_then(char::from_u32).map(|c| c.to_string())
                } else {
                    entity_char(body).map(str::to_string)
                };
                if let Some(d) = decoded {
                    out.push_str(&d);
                    i += len;
                    continue;
                }
            }
        }
        let ch = s[i..].chars().next().unwrap_or('&');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

// ---------------------------------------------------------------- source

const SOFT_WRAP: usize = 140;
const HARD_WRAP: usize = 220;

struct SourceBuilder {
    lines: Vec<SourceLine>,
    cur: SourceLine,
    cur_len: usize,
    max_lines: usize,
    cut: bool,
}

impl SourceBuilder {
    fn new(max_lines: usize) -> Self {
        SourceBuilder {
            lines: Vec::new(),
            cur: SourceLine::default(),
            cur_len: 0,
            max_lines,
            cut: false,
        }
    }

    fn newline(&mut self) {
        let line = std::mem::take(&mut self.cur);
        self.lines.push(line);
        self.cur_len = 0;
        if self.lines.len() >= self.max_lines {
            self.cut = true;
        }
    }

    fn push(&mut self, kind: SourceKind, text: &str) {
        if self.cut {
            return;
        }
        let mut first = true;
        for piece in text.split('\n') {
            if !first {
                self.newline();
                if self.cut {
                    return;
                }
            }
            first = false;
            let piece = piece.trim_end_matches('\r');
            if piece.is_empty() {
                continue;
            }
            let piece = piece.replace('\t', "    ");
            let mut chars = piece.chars().peekable();
            while chars.peek().is_some() {
                let room = HARD_WRAP.saturating_sub(self.cur_len).max(1);
                let chunk: String = chars.by_ref().take(room).collect();
                self.cur_len += chunk.chars().count();
                self.cur.tokens.push(SourceToken { kind, text: chunk });
                if chars.peek().is_some() {
                    self.newline();
                    if self.cut {
                        return;
                    }
                }
            }
        }
    }

    /// Break minified markup at a tag boundary once a line has grown long.
    fn soft_break(&mut self) {
        if self.cur_len > SOFT_WRAP && !self.cut {
            self.newline();
        }
    }

    fn text(&mut self, t: &str) {
        let mut i = 0;
        let mut plain = 0;
        while i < t.len() {
            if t.as_bytes()[i] == b'&' {
                if let Some(len) = entity_len(&t[i..]) {
                    if plain < i {
                        self.push(SourceKind::Plain, &t[plain..i]);
                    }
                    self.push(SourceKind::Entity, &t[i..i + len]);
                    i += len;
                    plain = i;
                    continue;
                }
            }
            i += 1;
        }
        if plain < t.len() {
            self.push(SourceKind::Plain, &t[plain..]);
        }
    }

    fn finish(mut self) -> (Vec<SourceLine>, bool) {
        if !self.cur.tokens.is_empty() && !self.cut {
            self.newline();
        }
        (self.lines, self.cut)
    }
}

// --------------------------------------------------------------- preview

struct Preview {
    title: String,
    blocks: Vec<Block>,
    max_blocks: usize,
    cut: bool,
    spans: Vec<Span>,
    kind: Option<(BlockKind, u32, String)>,
    bold: u32,
    italic: u32,
    code: u32,
    links: Vec<String>,
    pre: u32,
    quote: u32,
    lists: Vec<Option<u32>>,
    in_head: bool,
    in_title: bool,
}

impl Preview {
    fn new(max_blocks: usize) -> Self {
        Preview {
            title: String::new(),
            blocks: Vec::new(),
            max_blocks,
            cut: false,
            spans: Vec::new(),
            kind: None,
            bold: 0,
            italic: 0,
            code: 0,
            links: Vec::new(),
            pre: 0,
            quote: 0,
            lists: Vec::new(),
            in_head: false,
            in_title: false,
        }
    }

    fn style(&self) -> (u32, String) {
        let mut s = 0;
        if self.bold > 0 {
            s |= BOLD;
        }
        if self.italic > 0 {
            s |= ITALIC;
        }
        if self.code > 0 || self.pre > 0 {
            s |= CODE;
        }
        let href = self.links.last().cloned().unwrap_or_default();
        if !self.links.is_empty() {
            s |= LINK;
        }
        (s, href)
    }

    fn add_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let (style, href) = self.style();
        let piece = if self.pre > 0 {
            text.to_string()
        } else {
            // Collapse whitespace runs; no leading space at a block start.
            let at_start = self.spans.iter().all(|s| s.text.is_empty());
            let prev_space = self
                .spans
                .last()
                .map(|s| s.text.ends_with(char::is_whitespace))
                .unwrap_or(true);
            let mut out = String::with_capacity(text.len());
            let mut space = at_start || prev_space;
            for c in text.chars() {
                if c.is_whitespace() {
                    if !space {
                        out.push(' ');
                        space = true;
                    }
                } else {
                    out.push(c);
                    space = false;
                }
            }
            out
        };
        if piece.is_empty() {
            return;
        }
        if let Some(last) = self.spans.last_mut() {
            if last.style == style && last.href == href {
                last.text.push_str(&piece);
                return;
            }
        }
        self.spans.push(Span {
            text: piece,
            style,
            href,
        });
    }

    fn flush(&mut self) {
        let mut spans = std::mem::take(&mut self.spans);
        let explicit = self.kind.take();
        if self.pre == 0 {
            if let Some(last) = spans.last_mut() {
                last.text = last.text.trim_end().to_string();
            }
        } else if let Some(last) = spans.last_mut() {
            last.text = last.text.trim_end_matches('\n').to_string();
        }
        spans.retain(|s| !s.text.is_empty());
        if spans.is_empty() {
            return;
        }
        if self.blocks.len() >= self.max_blocks {
            self.cut = true;
            return;
        }
        let (kind, level, marker) = match explicit {
            Some(k) => k,
            None if self.pre > 0 => (BlockKind::Code, 0, String::new()),
            None if self.quote > 0 => (BlockKind::Quote, self.quote, String::new()),
            None => (BlockKind::Paragraph, 0, String::new()),
        };
        self.blocks.push(Block {
            kind,
            level,
            marker,
            spans,
        });
    }

    fn standalone(&mut self, kind: BlockKind, level: u32, spans: Vec<Span>) {
        self.flush();
        if self.blocks.len() >= self.max_blocks {
            self.cut = true;
            return;
        }
        self.blocks.push(Block {
            kind,
            level,
            marker: String::new(),
            spans,
        });
    }

    fn tag(&mut self, name: &str, closing: bool, attrs: &[(String, String)]) {
        let attr = |k: &str| {
            attrs
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        match name {
            "head" => {
                self.flush();
                self.in_head = !closing;
            }
            "title" => {
                self.in_title = !closing;
            }
            "br" => {
                if self.pre == 0 {
                    self.spans.push(Span {
                        text: "\n".into(),
                        style: 0,
                        href: String::new(),
                    });
                }
            }
            "hr" => self.standalone(BlockKind::Rule, 0, Vec::new()),
            "img" if !self.in_head => {
                let alt = attr("alt");
                let src = attr("src");
                let label = if !alt.is_empty() { alt } else { "Image".into() };
                let href = if src.starts_with("data:") {
                    "inline image".to_string()
                } else {
                    src
                };
                self.standalone(
                    BlockKind::Image,
                    0,
                    vec![Span {
                        text: label,
                        style: 0,
                        href,
                    }],
                );
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.flush();
                if !closing {
                    let n = (name.as_bytes()[1] - b'0') as u32;
                    self.kind = Some((BlockKind::Heading, n, String::new()));
                }
            }
            "li" => {
                self.flush();
                if !closing {
                    let depth = self.lists.len().saturating_sub(1) as u32;
                    let marker = match self.lists.last_mut() {
                        Some(Some(n)) => {
                            *n += 1;
                            format!("{n}.")
                        }
                        _ => "•".to_string(),
                    };
                    self.kind = Some((BlockKind::ListItem, depth, marker));
                }
            }
            "ul" | "ol" | "menu" => {
                self.flush();
                if closing {
                    self.lists.pop();
                } else {
                    self.lists.push(if name == "ol" { Some(0) } else { None });
                }
            }
            "blockquote" => {
                self.flush();
                if closing {
                    self.quote = self.quote.saturating_sub(1);
                } else {
                    self.quote += 1;
                }
            }
            "pre" => {
                self.flush();
                if closing {
                    self.pre = self.pre.saturating_sub(1);
                } else {
                    self.pre += 1;
                }
            }
            "a" => {
                if closing {
                    self.links.pop();
                } else {
                    self.links.push(attr("href"));
                }
            }
            "b" | "strong" => {
                self.bold = if closing {
                    self.bold.saturating_sub(1)
                } else {
                    self.bold + 1
                }
            }
            "i" | "em" | "cite" | "dfn" => {
                self.italic = if closing {
                    self.italic.saturating_sub(1)
                } else {
                    self.italic + 1
                }
            }
            "code" | "kbd" | "samp" | "tt" => {
                self.code = if closing {
                    self.code.saturating_sub(1)
                } else {
                    self.code + 1
                }
            }
            "th" => {
                if !closing && !self.spans.is_empty() {
                    self.add_text("  ");
                }
                self.bold = if closing {
                    self.bold.saturating_sub(1)
                } else {
                    self.bold + 1
                };
            }
            "td" => {
                if !closing && !self.spans.is_empty() {
                    self.add_text("  ");
                }
            }
            "p" | "div" | "section" | "article" | "header" | "footer" | "nav" | "main"
            | "aside" | "form" | "table" | "thead" | "tbody" | "tfoot" | "tr" | "caption"
            | "figure" | "figcaption" | "dl" | "dt" | "dd" | "address" | "fieldset" | "details"
            | "summary" | "body" | "html" | "option" | "textarea" | "button" => self.flush(),
            _ => {}
        }
    }
}

/// Scan `text` once into highlighted source lines and a preview.
pub fn html_view(text: &str, max_lines: usize, max_blocks: usize) -> HtmlView {
    let segs = scan(text);

    let mut src = SourceBuilder::new(max_lines);
    for seg in &segs {
        match *seg {
            Seg::Text(t) => src.text(t),
            Seg::Raw(t) => src.push(SourceKind::Plain, t),
            Seg::Comment(t) | Seg::Decl(t) => src.push(SourceKind::Comment, t),
            Seg::Open(t) => {
                src.soft_break();
                src.push(SourceKind::Punct, t);
            }
            Seg::Name(t) => src.push(SourceKind::Tag, t),
            Seg::AttrName(t) => src.push(SourceKind::Attr, t),
            Seg::Eq => src.push(SourceKind::Punct, "="),
            Seg::AttrValue(t) => src.push(SourceKind::Value, t),
            Seg::Ws(t) => src.push(SourceKind::Plain, t),
            Seg::Close(t) => src.push(SourceKind::Punct, t),
        }
    }
    let (source, lines_cut) = src.finish();

    let mut pv = Preview::new(max_blocks);
    // Tag under construction: name, is a closing tag, attributes.
    type OpenTag = (String, bool, Vec<(String, String)>);
    let mut tag: Option<OpenTag> = None;
    for seg in &segs {
        match *seg {
            Seg::Open(o) => tag = Some((String::new(), o.len() == 2, Vec::new())),
            Seg::Name(n) => {
                if let Some(t) = tag.as_mut() {
                    t.0 = n.to_ascii_lowercase();
                }
            }
            Seg::AttrName(a) => {
                if let Some(t) = tag.as_mut() {
                    t.2.push((a.to_ascii_lowercase(), String::new()));
                }
            }
            Seg::AttrValue(v) => {
                if let Some(last) = tag.as_mut().and_then(|t| t.2.last_mut()) {
                    let v = v.trim_matches(|c| c == '"' || c == '\'');
                    last.1 = decode_entities(v);
                }
            }
            Seg::Close(_) => {
                if let Some((name, closing, attrs)) = tag.take() {
                    pv.tag(&name, closing, &attrs);
                }
            }
            Seg::Text(t) => {
                let t = decode_entities(t);
                if pv.in_title {
                    pv.title.push_str(&t);
                } else if !pv.in_head {
                    pv.add_text(&t);
                }
            }
            Seg::Comment(_) | Seg::Decl(_) | Seg::Raw(_) | Seg::Eq | Seg::Ws(_) => {}
        }
        if pv.cut {
            break;
        }
    }
    pv.flush();
    let title = pv.title.split_whitespace().collect::<Vec<_>>().join(" ");

    HtmlView {
        title,
        blocks: pv.blocks,
        source,
        blocks_cut: pv.cut,
        lines_cut,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(v: &HtmlView) -> Vec<(BlockKind, String)> {
        v.blocks
            .iter()
            .map(|b| (b.kind, b.spans.iter().map(|s| s.text.as_str()).collect()))
            .collect()
    }

    #[test]
    fn headings_paragraphs_and_entities() {
        let v = html_view(
            "<html><head><title>Hi  there</title><style>p{color:red}</style></head>\
             <body><h1>Welcome</h1><p>Fish &amp; <b>chips</b>\n  &#39;n&#39; peas</p></body></html>",
            100,
            100,
        );
        assert_eq!(v.title, "Hi there");
        let t = texts(&v);
        assert_eq!(t[0], (BlockKind::Heading, "Welcome".into()));
        assert_eq!(t[1], (BlockKind::Paragraph, "Fish & chips 'n' peas".into()));
        assert_eq!(v.blocks[1].spans[1].style, BOLD);
        assert!(!v
            .blocks
            .iter()
            .any(|b| b.spans.iter().any(|s| s.text.contains("color:red"))));
    }

    #[test]
    fn scripts_never_reach_the_preview() {
        let v = html_view(
            "<p>a</p><script>alert('<b>x</b>')</script><p>b</p>",
            100,
            100,
        );
        let t = texts(&v);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].1, "a");
        assert_eq!(t[1].1, "b");
    }

    #[test]
    fn lists_links_images_and_code() {
        let v = html_view(
            "<ol><li>one</li><li><a href=\"/x?a=1&amp;b=2\">two</a></li></ol>\
             <ul><li>dot</li></ul><img src=a.png alt='Logo'><pre>fn  main() {\n}\n</pre><hr>",
            100,
            100,
        );
        assert_eq!(v.blocks[0].marker, "1.");
        assert_eq!(v.blocks[1].marker, "2.");
        assert_eq!(v.blocks[1].spans[0].href, "/x?a=1&b=2");
        assert_eq!(v.blocks[1].spans[0].style, LINK);
        assert_eq!(v.blocks[2].marker, "•");
        assert_eq!(v.blocks[3].kind, BlockKind::Image);
        assert_eq!(v.blocks[3].spans[0].text, "Logo");
        assert_eq!(v.blocks[4].kind, BlockKind::Code);
        assert_eq!(v.blocks[4].spans[0].text, "fn  main() {\n}");
        assert_eq!(v.blocks[5].kind, BlockKind::Rule);
    }

    #[test]
    fn source_tokens_cover_the_input() {
        let src = "<!doctype html>\n<a href=\"/x\" data-k=v>t &amp; u</a><!-- c -->";
        let v = html_view(src, 100, 100);
        let rebuilt: Vec<String> = v
            .source
            .iter()
            .map(|l| l.tokens.iter().map(|t| t.text.as_str()).collect::<String>())
            .collect();
        assert_eq!(rebuilt.join("\n"), src);
        let kinds: Vec<SourceKind> = v.source[1].tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&SourceKind::Tag));
        assert!(kinds.contains(&SourceKind::Attr));
        assert!(kinds.contains(&SourceKind::Value));
        assert!(kinds.contains(&SourceKind::Entity));
        assert!(kinds.contains(&SourceKind::Comment));
    }

    #[test]
    fn minified_markup_is_broken_into_short_lines() {
        let src = "<div>x</div>".repeat(400);
        let v = html_view(&src, 1000, 1000);
        assert!(v.source.len() > 10);
        assert!(v.source.iter().all(|l| l
            .tokens
            .iter()
            .map(|t| t.text.chars().count())
            .sum::<usize>()
            <= 220));
    }

    #[test]
    fn malformed_input_terminates() {
        for s in [
            "<",
            "<a",
            "<a href=",
            "<a href=\"x",
            "</",
            "<!--",
            "<script>",
            "&",
            "<p <b>",
        ] {
            let _ = html_view(s, 10, 10);
        }
    }

    #[test]
    fn random_markup_soup_never_panics() {
        // Deterministic noise over the characters that matter to the scanner,
        // including multi-byte text right next to delimiters.
        let alphabet: Vec<&str> = vec![
            "<", ">", "/", "=", "\"", "'", " ", "&", ";", "!", "-", "#", "x", "a", "p", "\n",
            "script", "style", "pre", "li", "é", "漢", "😀", "</", "<!--", "-->", "&amp;", "&#",
        ];
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        for round in 0..200 {
            let mut s = String::new();
            for _ in 0..400 {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                s.push_str(alphabet[(state >> 33) as usize % alphabet.len()]);
            }
            let v = html_view(&s, 5_000, 5_000);
            // Source tokens always rebuild the input (minus line breaks we add).
            let joined: String = v
                .source
                .iter()
                .flat_map(|l| l.tokens.iter().map(|t| t.text.as_str()))
                .collect();
            let strip = |x: &str| x.chars().filter(|c| !matches!(c, '\n' | '\r' | '\t')).collect::<String>();
            assert_eq!(
                strip(&joined).replace("    ", ""),
                strip(&s).replace("\t", "").replace("    ", ""),
                "round {round}"
            );
        }
    }

    #[test]
    fn a_large_page_is_fast_and_bounded() {
        let page = "<div class=\"row\"><p>Row <b>text</b> &amp; <a href=\"/u\">link</a></p></div>\n"
            .repeat(7_000);
        assert!(page.len() > 450_000);
        let t = std::time::Instant::now();
        let v = html_view(&page, 20_000, 4_000);
        assert!(t.elapsed() < std::time::Duration::from_secs(2), "{:?}", t.elapsed());
        assert!(v.blocks.len() <= 4_000);
        assert!(v.blocks_cut);
        assert!(v.source.len() <= 20_000);
    }

    #[test]
    fn sniffing() {
        assert!(looks_like_html(b"  <!DOCTYPE html><html>"));
        assert!(looks_like_html(b"<HTML>"));
        assert!(!looks_like_html(b"<svg/>"));
        assert!(!looks_like_html(b"{\"a\":1}"));
    }
}
