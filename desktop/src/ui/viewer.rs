//! Body viewers. The core hands over tokens and blocks; this module only
//! lays them out: a virtualised code surface with a line-number gutter and
//! indent guides, and a simplified page preview for HTML.

use super::icons::{self, Icon};
use crate::motion;
use crate::paint::{self, cr};
use crate::theme::{semibold, Role, Style};
use egui::text::{LayoutJob, TextFormat};
use egui::{
    pos2, vec2, Align2, Color32, FontFamily, FontId, Id, Rect, Response, ScrollArea, Sense, Stroke,
    Ui, UiBuilder,
};
use proto::pb;
use std::hash::Hash;

pub const ROW_H: f32 = 20.0;
pub const GUTTER_W: f32 = 46.0;
/// Advance of one monospace column at the code size.
pub const CHAR_W: f32 = 7.3;
pub const INDENT: f32 = 16.0;

const STYLE_BOLD: u32 = 1;
const STYLE_ITALIC: u32 = 2;
const STYLE_CODE: u32 = 4;
const STYLE_LINK: u32 = 8;

/// Fade a freshly shown surface in from partial opacity on a spring. `key`
/// changes whenever the content does (selection, tab, mode).
pub fn materialize(ctx: &egui::Context, st: &Style, id: Id, key: u64) -> f32 {
    let key_id = id.with("key");
    let last: Option<u64> = ctx.data(|d| d.get_temp(key_id));
    if last != Some(key) {
        ctx.data_mut(|d| d.insert_temp(key_id, key));
        motion::with_spring(ctx, id, 1.0, |sp| {
            sp.value = 0.2;
            sp.velocity = 0.0;
            sp.target = 1.0;
        });
    }
    motion::animate(ctx, id, 1.0, motion::SMOOTH, st.reduced_motion).clamp(0.0, 1.0)
}

/// Shorten a token for painting; the full text is still in Copy.
fn clip(s: &str, max_chars: usize) -> std::borrow::Cow<'_, str> {
    if s.len() <= max_chars {
        return s.into();
    }
    match s.char_indices().nth(max_chars) {
        Some((i, _)) => format!("{}…", &s[..i]).into(),
        None => s.into(),
    }
}

pub fn gutter_number(ui: &Ui, st: &Style, row: Rect, n: usize) {
    paint::text(
        ui.painter(),
        ui,
        pos2(row.left() + GUTTER_W - 12.0, row.center().y),
        Align2::RIGHT_CENTER,
        &n.to_string(),
        Role::MonoSmall,
        st.p.syn.gutter,
    );
}

/// A scrolling code surface: only visible rows are built. `paint_row` gets
/// the full-width row rect, its index and the row's response.
#[allow(clippy::too_many_arguments)]
pub fn code_area(
    ui: &mut Ui,
    st: &Style,
    id: impl Hash + std::fmt::Debug,
    rect: Rect,
    rows: usize,
    content_w: f32,
    opacity: f32,
    mut paint_row: impl FnMut(&Ui, Rect, usize, &Response),
) {
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    child.set_opacity(opacity);
    // `show_rows` sizes its range from the parent's spacing, so zero it here.
    child.spacing_mut().item_spacing.y = 0.0;
    ScrollArea::both()
        .id_salt(id)
        .auto_shrink([false, false])
        .show_rows(&mut child, ROW_H, rows, |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let w = ui.available_width().max(content_w);
            for i in range {
                let (r, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, 0.0, st.p.syn.hover);
                }
                paint_row(ui, r, i, &resp);
            }
        });
}

/// Width a run of `chars` columns needs, with the gutter and some air.
pub fn width_for(chars: usize, indent: f32) -> f32 {
    GUTTER_W + 20.0 + indent + chars as f32 * CHAR_W + 24.0
}

/// Plain text, one row per line.
pub fn text_view(
    ui: &mut Ui,
    st: &Style,
    id: impl Hash + std::fmt::Debug,
    rect: Rect,
    lines: &[String],
    opacity: f32,
) {
    let max = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    code_area(
        ui,
        st,
        id,
        rect,
        lines.len(),
        width_for(max, 0.0),
        opacity,
        |ui, r, i, _| {
            gutter_number(ui, st, r, i + 1);
            paint::text(
                ui.painter(),
                ui,
                pos2(r.left() + GUTTER_W, r.center().y),
                Align2::LEFT_CENTER,
                &lines[i],
                Role::Code,
                st.p.syn.text,
            );
        },
    );
}

fn source_color(st: &Style, kind: i32) -> (Color32, Role) {
    let s = &st.p.syn;
    match pb::SourceTokenKind::try_from(kind).unwrap_or(pb::SourceTokenKind::SrcPlain) {
        pb::SourceTokenKind::SrcPlain => (s.text, Role::Code),
        pb::SourceTokenKind::SrcTag => (s.tag, Role::CodeMedium),
        pb::SourceTokenKind::SrcAttr => (s.attr, Role::Code),
        pb::SourceTokenKind::SrcValue => (s.value, Role::Code),
        pb::SourceTokenKind::SrcComment => (s.comment, Role::Code),
        pb::SourceTokenKind::SrcPunct => (s.punct, Role::Code),
        pb::SourceTokenKind::SrcEntity => (s.entity, Role::Code),
    }
}

/// Highlighted markup, one row per display line.
pub fn source_view(
    ui: &mut Ui,
    st: &Style,
    id: impl Hash + std::fmt::Debug,
    rect: Rect,
    lines: &[pb::SourceLine],
    opacity: f32,
) {
    let max = lines
        .iter()
        .map(|l| {
            l.tokens
                .iter()
                .map(|t| t.text.chars().count())
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0);
    code_area(
        ui,
        st,
        id,
        rect,
        lines.len(),
        width_for(max, 0.0),
        opacity,
        |ui, r, i, _| {
            gutter_number(ui, st, r, i + 1);
            let mut x = r.left() + GUTTER_W;
            for t in &lines[i].tokens {
                let (color, role) = source_color(st, t.kind);
                let rr = paint::text(
                    ui.painter(),
                    ui,
                    pos2(x, r.center().y),
                    Align2::LEFT_CENTER,
                    &t.text,
                    role,
                    color,
                );
                x = rr.right();
            }
        },
    );
}

/// What one JSON row shows.
pub struct JsonRows<'a> {
    pub lines: &'a [pb::JsonLine],
    /// Indices into `lines` that are visible given the folds.
    pub visible: &'a [usize],
    pub folded: &'a std::collections::HashSet<usize>,
}

/// Indices of lines that are not hidden inside a folded container.
pub fn visible_json(
    lines: &[pb::JsonLine],
    folded: &std::collections::HashSet<usize>,
) -> Vec<usize> {
    let mut visible = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        visible.push(i);
        if folded.contains(&i) && lines[i].fold_end > i as i32 {
            i = lines[i].fold_end as usize + 1;
        } else {
            i += 1;
        }
    }
    visible
}

fn json_color(st: &Style, kind: i32) -> (Color32, Role) {
    let s = &st.p.syn;
    match pb::JsonTokenKind::try_from(kind).unwrap_or(pb::JsonTokenKind::Punct) {
        pb::JsonTokenKind::Key => (s.key, Role::CodeMedium),
        pb::JsonTokenKind::String => (s.string, Role::Code),
        pb::JsonTokenKind::Number => (s.number, Role::CodeMedium),
        pb::JsonTokenKind::Literal => (s.literal, Role::CodeMedium),
        pb::JsonTokenKind::Punct => (s.punct, Role::Code),
    }
}

/// A foldable JSON outline. Returns the line whose fold was toggled.
pub fn json_view(
    ui: &mut Ui,
    st: &Style,
    id: impl Hash + std::fmt::Debug,
    rect: Rect,
    rows: &JsonRows,
    opacity: f32,
) -> Option<usize> {
    let mut toggled = None;
    let content_w = rows
        .visible
        .iter()
        .map(|&li| {
            let l = &rows.lines[li];
            let chars: usize = l
                .tokens
                .iter()
                .map(|t| t.text.chars().count().min(600))
                .sum();
            width_for(chars, l.depth as f32 * INDENT)
        })
        .fold(0.0f32, f32::max);
    code_area(
        ui,
        st,
        id,
        rect,
        rows.visible.len(),
        content_w,
        opacity,
        |ui, r, vi, resp| {
            let li = rows.visible[vi];
            let line = &rows.lines[li];
            let p = ui.painter();
            let mid = r.center().y;
            gutter_number(ui, st, r, li + 1);

            let base = r.left() + GUTTER_W;
            let indent = line.depth as f32 * INDENT;
            // Guides run under the parent's chevron, row after row.
            for d in 0..line.depth {
                let gx = (base + 7.0 + d as f32 * INDENT).round() + 0.5;
                p.line_segment(
                    [pos2(gx, r.top()), pos2(gx, r.bottom())],
                    Stroke::new(1.0, st.p.syn.guide),
                );
            }
            let foldable = line.fold_end >= 0 && (line.fold_end as usize) > li;
            let is_folded = rows.folded.contains(&li);
            if foldable {
                let icon = if is_folded {
                    Icon::ChevronRight
                } else {
                    Icon::ChevronDown
                };
                icons::paint(
                    p,
                    icon,
                    pos2(base + 7.0 + indent, mid),
                    10.0,
                    st.p.syn.punct,
                );
                if resp.clicked() {
                    toggled = Some(li);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
            let mut x = base + 16.0 + indent;
            for t in &line.tokens {
                let (color, role) = json_color(st, t.kind);
                let text = clip(&t.text, 600);
                let rr = paint::text(p, ui, pos2(x, mid), Align2::LEFT_CENTER, &text, role, color);
                x = rr.right();
            }
            if is_folded && foldable {
                let close: String = rows
                    .lines
                    .get(line.fold_end as usize)
                    .map(|l| l.tokens.iter().map(|t| t.text.as_str()).collect())
                    .unwrap_or_default();
                let hidden = line.fold_end as usize - li;
                let label = format!(" … {hidden} lines {close}");
                let g = paint::galley(ui, &label, Role::Code, st.p.syn.comment);
                let pill =
                    Rect::from_min_size(pos2(x + 4.0, mid - 8.0), vec2(g.size().x + 10.0, 16.0));
                p.rect_filled(pill, cr(5), st.p.fill);
                p.galley(
                    pos2(pill.left() + 5.0, mid - g.size().y / 2.0),
                    g,
                    st.p.syn.comment,
                );
            }
        },
    );
    toggled
}

// ------------------------------------------------------------ HTML preview

struct Typeset<'a> {
    st: &'a Style,
}

impl Typeset<'_> {
    fn body_font(&self) -> FontId {
        FontId::new(14.0, FontFamily::Proportional)
    }

    fn format(&self, span: &pb::HtmlSpan, base: &FontId, color: Color32) -> TextFormat {
        let p = &self.st.p;
        let mut font = base.clone();
        let mut fmt = TextFormat {
            font_id: font.clone(),
            color,
            line_height: Some(base.size * 1.5),
            ..Default::default()
        };
        if span.style & STYLE_BOLD != 0 && base.family == FontFamily::Proportional {
            font.family = semibold();
        }
        if span.style & STYLE_CODE != 0 {
            font = FontId::new(base.size - 1.0, FontFamily::Monospace);
            fmt.background = p.fill;
            fmt.color = p.syn.value;
        }
        if span.style & STYLE_ITALIC != 0 {
            fmt.italics = true;
        }
        if span.style & STYLE_LINK != 0 {
            fmt.color = p.accent;
            fmt.underline = Stroke::new(1.0, paint::with_alpha(p.accent, 0.55));
        }
        fmt.font_id = font;
        fmt
    }

    fn job(&self, spans: &[pb::HtmlSpan], base: FontId, color: Color32, width: f32) -> LayoutJob {
        let mut job = LayoutJob::default();
        for s in spans {
            job.append(&s.text, 0.0, self.format(s, &base, color));
        }
        job.wrap.max_width = width;
        job
    }
}

/// The href under `char_index`, if the pointer is over a link span.
fn link_at(spans: &[pb::HtmlSpan], char_index: usize) -> Option<&str> {
    let mut start = 0;
    for s in spans {
        let n = s.text.chars().count();
        if char_index >= start && char_index < start + n {
            return (s.style & STYLE_LINK != 0 && !s.href.is_empty()).then_some(s.href.as_str());
        }
        start += n;
    }
    None
}

/// A rough height for sizing the preview card before it is drawn.
pub fn estimate_preview_height(blocks: &[pb::HtmlBlock], width: f32) -> f32 {
    let per_line = (width / 7.4).max(20.0);
    let mut h = 40.0;
    for b in blocks {
        let chars: usize = b.spans.iter().map(|s| s.text.chars().count()).sum();
        let lines = (chars as f32 / per_line).ceil().max(1.0)
            + b.spans
                .iter()
                .map(|s| s.text.matches('\n').count())
                .sum::<usize>() as f32;
        h += match pb::HtmlBlockKind::try_from(b.kind).unwrap_or(pb::HtmlBlockKind::HtmlParagraph) {
            pb::HtmlBlockKind::HtmlHeading => 44.0,
            pb::HtmlBlockKind::HtmlRule => 22.0,
            pb::HtmlBlockKind::HtmlImage => 66.0,
            _ => lines * 21.0 + 10.0,
        };
    }
    h
}

/// Render a page's blocks. Returns a link the person clicked, if any.
pub fn html_preview(
    ui: &mut Ui,
    st: &Style,
    id: impl Hash + std::fmt::Debug,
    rect: Rect,
    title: &str,
    blocks: &[pb::HtmlBlock],
    opacity: f32,
) -> Option<String> {
    let mut clicked: Option<String> = None;
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    child.set_opacity(opacity);
    let lay = Typeset { st };
    let p = &st.p;
    ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let pad = 22.0;
            let full_w = ui.available_width();
            let w = (full_w - pad * 2.0).clamp(120.0, 760.0);
            ui.add_space(16.0);

            if !title.is_empty() {
                let (r, _) = ui.allocate_exact_size(vec2(full_w, 24.0), Sense::hover());
                let g = paint::galley_wrapped(ui, title, Role::Caption, p.text3, w, 1, true);
                ui.painter().galley(
                    pos2(r.left() + pad, r.center().y - g.size().y / 2.0),
                    g,
                    p.text3,
                );
                ui.add_space(6.0);
            }

            for b in blocks.iter() {
                let kind =
                    pb::HtmlBlockKind::try_from(b.kind).unwrap_or(pb::HtmlBlockKind::HtmlParagraph);
                match kind {
                    pb::HtmlBlockKind::HtmlRule => {
                        let (r, _) = ui.allocate_exact_size(vec2(full_w, 24.0), Sense::hover());
                        let y = r.center().y;
                        ui.painter().line_segment(
                            [pos2(r.left() + pad, y), pos2(r.left() + pad + w, y)],
                            Stroke::new(1.0, p.separator),
                        );
                    }
                    pb::HtmlBlockKind::HtmlImage => {
                        let s = b.spans.first();
                        let alt = s.map(|s| s.text.as_str()).unwrap_or("Image");
                        let src = s.map(|s| s.href.as_str()).unwrap_or("");
                        let (r, _) = ui.allocate_exact_size(vec2(full_w, 58.0), Sense::hover());
                        let card =
                            Rect::from_min_size(pos2(r.left() + pad, r.top() + 4.0), vec2(w, 50.0));
                        let pa = ui.painter();
                        pa.rect_filled(card, cr(10), p.fill);
                        let thumb =
                            Rect::from_min_size(card.min + vec2(8.0, 8.0), vec2(34.0, 34.0));
                        pa.rect_filled(thumb, cr(7), paint::with_alpha(p.accent, 0.16));
                        paint::text(
                            pa,
                            ui,
                            thumb.center(),
                            Align2::CENTER_CENTER,
                            "IMG",
                            Role::Overline,
                            p.accent,
                        );
                        let tx = card.left() + 52.0;
                        paint::text_fit(
                            pa,
                            ui,
                            pos2(tx, card.center().y - 8.0),
                            card.right() - tx - 10.0,
                            alt,
                            Role::BodyMedium,
                            p.text,
                        );
                        paint::text_fit(
                            pa,
                            ui,
                            pos2(tx, card.center().y + 9.0),
                            card.right() - tx - 10.0,
                            if src.is_empty() { "No source" } else { src },
                            Role::Callout,
                            p.text3,
                        );
                    }
                    _ => {
                        let (indent, base, color, top, bottom) = match kind {
                            pb::HtmlBlockKind::HtmlHeading => {
                                let (size, top) = match b.level {
                                    1 => (26.0, 14.0),
                                    2 => (20.0, 12.0),
                                    3 => (16.5, 10.0),
                                    _ => (14.0, 8.0),
                                };
                                let col = if b.level >= 6 { p.text2 } else { p.text };
                                (0.0, FontId::new(size, semibold()), col, top, 6.0)
                            }
                            pb::HtmlBlockKind::HtmlListItem => (
                                18.0 + b.level as f32 * 20.0,
                                lay.body_font(),
                                p.text,
                                1.0,
                                3.0,
                            ),
                            pb::HtmlBlockKind::HtmlQuote => (
                                16.0 + (b.level.saturating_sub(1)) as f32 * 14.0,
                                lay.body_font(),
                                p.text2,
                                4.0,
                                8.0,
                            ),
                            pb::HtmlBlockKind::HtmlCode => (
                                12.0,
                                FontId::new(12.0, FontFamily::Monospace),
                                p.syn.text,
                                4.0,
                                10.0,
                            ),
                            _ => (0.0, lay.body_font(), p.text, 2.0, 8.0),
                        };
                        let tw = (w - indent).max(80.0);
                        let mut job = lay.job(&b.spans, base.clone(), color, tw);
                        if kind == pb::HtmlBlockKind::HtmlHeading {
                            for s in &mut job.sections {
                                s.format.line_height = Some(base.size * 1.25);
                                s.format.font_id = base.clone();
                                s.format.extra_letter_spacing =
                                    if b.level == 1 { -0.5 } else { -0.2 };
                            }
                        }
                        if kind == pb::HtmlBlockKind::HtmlCode {
                            for s in &mut job.sections {
                                s.format.line_height = Some(18.0);
                                s.format.font_id = base.clone();
                            }
                            job.wrap.break_anywhere = true;
                        }
                        let g = ui.ctx().fonts_mut(|f| f.layout_job(job));
                        ui.add_space(top);
                        let block_h = g.size().y
                            + if kind == pb::HtmlBlockKind::HtmlCode {
                                16.0
                            } else {
                                0.0
                            };
                        let (r, resp) =
                            ui.allocate_exact_size(vec2(full_w, block_h), Sense::click());
                        let pa = ui.painter();
                        let origin = pos2(r.left() + pad + indent, r.top());
                        match kind {
                            pb::HtmlBlockKind::HtmlListItem => {
                                paint::text(
                                    pa,
                                    ui,
                                    pos2(r.left() + pad + indent - 8.0, r.top() + 10.5),
                                    Align2::RIGHT_CENTER,
                                    &b.marker,
                                    Role::Body,
                                    p.text3,
                                );
                            }
                            pb::HtmlBlockKind::HtmlQuote => {
                                pa.rect_filled(
                                    Rect::from_min_size(
                                        pos2(r.left() + pad, r.top() + 1.0),
                                        vec2(3.0, block_h - 2.0),
                                    ),
                                    cr(2),
                                    paint::with_alpha(p.accent, 0.55),
                                );
                            }
                            pb::HtmlBlockKind::HtmlCode => {
                                let bg = Rect::from_min_size(
                                    pos2(r.left() + pad, r.top()),
                                    vec2(w, block_h),
                                );
                                pa.rect_filled(bg, cr(8), p.fill);
                            }
                            _ => {}
                        }
                        let text_origin = if kind == pb::HtmlBlockKind::HtmlCode {
                            origin + vec2(0.0, 8.0)
                        } else {
                            origin
                        };
                        let text_rect = Rect::from_min_size(text_origin, g.size());
                        pa.galley(text_origin, g.clone(), color);

                        // Links: hit-test the pointer against the span ranges.
                        if b.spans.iter().any(|s| s.style & STYLE_LINK != 0) {
                            if let Some(pos) = resp.hover_pos() {
                                if text_rect.contains(pos) {
                                    let cur = g.cursor_from_pos(pos - text_rect.min);
                                    if let Some(href) = link_at(&b.spans, cur.index.0) {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                        let href = href.to_string();
                                        resp.clone().on_hover_text(clip(&href, 140));
                                        if resp.clicked() {
                                            clicked = Some(href);
                                        }
                                    }
                                }
                            }
                        }
                        ui.add_space(bottom);
                    }
                }
            }
            ui.add_space(20.0);
        });
    clicked
}
