//! Controls. Every one responds on pointer-down, animates on springs that
//! retarget from their live value, and keeps a visible keyboard focus ring.

use super::icons::{self, Icon};
use crate::motion::{self, MOVE, QUICK};
use crate::paint::{self, cr};
use crate::theme::{radius, Role, Style};
use egui::{pos2, vec2, Align2, Color32, Id, Rect, Response, Sense, Ui};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Primary,
    Secondary,
    Plain,
    Destructive,
}

/// Press, hover and focus animation shared by buttons.
struct Feel {
    hover: f32,
    press: f32,
}

fn feel(ui: &Ui, st: &Style, resp: &Response, enabled: bool) -> Feel {
    let ctx = ui.ctx();
    let down = enabled && resp.is_pointer_button_down_on();
    // Instant on press: feedback never waits for release.
    let press = motion::animate(
        ctx,
        resp.id.with("press"),
        down as u8 as f32,
        QUICK,
        st.reduced_motion,
    );
    let hover = motion::animate(
        ctx,
        resp.id.with("hover"),
        (enabled && resp.hovered()) as u8 as f32,
        QUICK,
        st.reduced_motion,
    );
    Feel { hover, press }
}

pub fn button(
    ui: &mut Ui,
    st: &Style,
    label: &str,
    kind: Kind,
    icon: Option<Icon>,
    enabled: bool,
) -> Response {
    let g = paint::galley(ui, label, Role::BodyMedium, Color32::WHITE);
    let icon_w = if icon.is_some() { 16.0 + 6.0 } else { 0.0 };
    let w = g.size().x + icon_w + 24.0;
    let (rect, resp) = ui.allocate_exact_size(
        vec2(w.max(60.0), 28.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let f = feel(ui, st, &resp, enabled);
    let p = ui.painter();
    let (bg, fg) = match kind {
        Kind::Primary => (
            paint::mix(st.p.accent, Color32::BLACK, 0.12 * f.press + 0.0),
            st.p.on_accent,
        ),
        Kind::Secondary => (
            paint::mix(
                paint::mix(st.p.fill, st.p.fill_hover, f.hover),
                st.p.fill_press,
                f.press,
            ),
            st.p.text,
        ),
        Kind::Plain => (
            paint::with_alpha(st.p.fill, f.hover * 0.9 + f.press * 0.4),
            st.p.accent,
        ),
        Kind::Destructive => (
            paint::mix(
                paint::mix(st.p.fill, st.p.fill_hover, f.hover),
                st.p.fill_press,
                f.press,
            ),
            st.p.red,
        ),
    };
    let fg = if enabled { fg } else { st.p.text3 };
    let r = paint::scale_rect(rect, 1.0 - 0.03 * f.press);
    if kind == Kind::Primary && f.hover > 0.0 {
        p.rect_filled(
            r,
            cr(radius::CONTROL),
            paint::mix(bg, Color32::WHITE, 0.08 * f.hover),
        );
    } else {
        p.rect_filled(r, cr(radius::CONTROL), bg);
    }
    let mut x = r.left() + 12.0;
    if let Some(ic) = icon {
        icons::paint(p, ic, pos2(x + 8.0, r.center().y), 15.0, fg);
        x += icon_w;
    }
    p.galley(
        pos2(x, r.center().y - g.size().y / 2.0),
        paint::galley(ui, label, Role::BodyMedium, fg),
        fg,
    );
    if resp.has_focus() {
        paint::ring(p, st, r, radius::CONTROL);
    }
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

pub fn icon_button(
    ui: &mut Ui,
    st: &Style,
    icon: Icon,
    tip: &str,
    active: bool,
    enabled: bool,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(
        vec2(30.0, 28.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let f = feel(ui, st, &resp, enabled);
    let p = ui.painter();
    let r = paint::scale_rect(rect, 1.0 - 0.04 * f.press);
    let bg = if active {
        paint::mix(st.p.accent_soft, st.p.fill_press, f.press * 0.5)
    } else {
        paint::with_alpha(
            paint::mix(st.p.fill_hover, st.p.fill_press, f.press),
            f.hover.max(f.press),
        )
    };
    p.rect_filled(r, cr(radius::CONTROL), bg);
    let fg = if !enabled {
        st.p.text3
    } else if active {
        st.p.accent
    } else {
        paint::mix(st.p.text2, st.p.text, f.hover)
    };
    icons::paint(p, icon, r.center(), 16.0, fg);
    if resp.has_focus() {
        paint::ring(p, st, r, radius::CONTROL);
    }
    resp.on_hover_text(tip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Segmented control with a thumb that slides on a spring and re-targets
/// mid-flight when clicked again.
pub fn segmented(
    ui: &mut Ui,
    st: &Style,
    id: Id,
    labels: &[&str],
    selected: usize,
) -> Option<usize> {
    segmented_w(ui, st, id, labels, selected, None)
}

/// Like [`segmented`], but spread over `fill` pixels when given, with equal
/// segments, so it never overflows a narrow pane.
pub fn segmented_w(
    ui: &mut Ui,
    st: &Style,
    id: Id,
    labels: &[&str],
    selected: usize,
    fill: Option<f32>,
) -> Option<usize> {
    let pad = 14.0;
    let widths: Vec<f32> = match fill {
        Some(w) => vec![((w - 4.0) / labels.len() as f32).max(40.0); labels.len()],
        None => labels
            .iter()
            .map(|l| paint::galley(ui, l, Role::BodyMedium, Color32::WHITE).size().x + pad * 2.0)
            .collect(),
    };
    let total: f32 = widths.iter().sum::<f32>() + 4.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, 28.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, cr(radius::CONTROL + 1), st.p.fill);

    let mut xs = Vec::new();
    let mut x = rect.left() + 2.0;
    for w in &widths {
        xs.push((x, *w));
        x += w;
    }
    let (tx, tw) = xs[selected.min(xs.len() - 1)];
    let ctx = ui.ctx().clone();
    let thumb_x = motion::animate(&ctx, id.with("x"), tx, MOVE, st.reduced_motion);
    let thumb_w = motion::animate(&ctx, id.with("w"), tw, MOVE, st.reduced_motion);
    let thumb = Rect::from_min_size(pos2(thumb_x, rect.top() + 2.0), vec2(thumb_w, 24.0));
    paint::shadow(p, st, thumb, radius::CONTROL, 0);
    let thumb_fill = if st.p.is_dark() {
        Color32::from_rgb(0x63, 0x63, 0x68)
    } else {
        Color32::WHITE
    };
    p.rect_filled(thumb, cr(radius::CONTROL - 1), thumb_fill);

    let mut out = None;
    for (i, (label, (x, w))) in labels.iter().zip(xs.iter()).enumerate() {
        let cell = Rect::from_min_size(pos2(*x, rect.top() + 2.0), vec2(*w, 24.0));
        let r = ui.interact(cell, id.with(i), Sense::click());
        let down = r.is_pointer_button_down_on() && i != selected;
        let color = if i == selected || down {
            st.p.text
        } else {
            paint::mix(st.p.text2, st.p.text, if r.hovered() { 1.0 } else { 0.0 })
        };
        paint::text(
            p,
            ui,
            cell.center(),
            Align2::CENTER_CENTER,
            label,
            Role::BodyMedium,
            color,
        );
        if r.clicked() && i != selected {
            out = Some(i);
        }
        r.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    out
}

pub fn toggle(ui: &mut Ui, st: &Style, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(40.0, 24.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let ctx = ui.ctx();
    let t = motion::animate(
        ctx,
        resp.id.with("t"),
        *on as u8 as f32,
        MOVE,
        st.reduced_motion,
    );
    let press = motion::animate(
        ctx,
        resp.id.with("p"),
        resp.is_pointer_button_down_on() as u8 as f32,
        QUICK,
        st.reduced_motion,
    );
    let p = ui.painter();
    let off = st.p.fill_press;
    let green = if st.p.is_dark() {
        Color32::from_rgb(0x30, 0xD1, 0x58)
    } else {
        Color32::from_rgb(0x34, 0xC7, 0x59)
    };
    p.rect_filled(
        rect,
        cr(radius::PILL),
        paint::mix(off, green, t.clamp(0.0, 1.0)),
    );
    let knob_w = 20.0 + 4.0 * press;
    let x = rect.left() + 2.0 + t * (rect.width() - 4.0 - knob_w);
    let knob = Rect::from_min_size(pos2(x, rect.top() + 2.0), vec2(knob_w, 20.0));
    paint::shadow(p, st, knob, 10, 0);
    p.rect_filled(knob, cr(10), Color32::WHITE);
    if resp.has_focus() {
        paint::ring(p, st, rect, 12);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// One row of a context menu. The hovered row fills with the accent and its
/// text turns white, as on macOS; disabled rows stay quiet and inert.
pub fn menu_item(
    ui: &mut Ui,
    st: &Style,
    icon: Option<Icon>,
    label: &str,
    hint: &str,
    enabled: bool,
) -> Response {
    let w = ui.available_width().max(248.0);
    let (rect, resp) = ui.allocate_exact_size(
        vec2(w, 28.0),
        if enabled { Sense::click() } else { Sense::hover() },
    );
    let hot = enabled && (resp.hovered() || resp.has_focus());
    let p = ui.painter();
    if hot {
        p.rect_filled(rect.shrink2(vec2(4.0, 0.0)), cr(6), st.p.accent);
    }
    let (fg, sub) = if hot {
        (st.p.on_accent, paint::with_alpha(st.p.on_accent, 0.8))
    } else if enabled {
        (st.p.text, st.p.text3)
    } else {
        (st.p.text3, st.p.text3)
    };
    if let Some(ic) = icon {
        icons::paint(p, ic, pos2(rect.left() + 20.0, rect.center().y), 15.0, fg);
    }
    paint::text(
        p,
        ui,
        pos2(rect.left() + 38.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        Role::Body,
        fg,
    );
    if !hint.is_empty() {
        paint::text(
            p,
            ui,
            pos2(rect.right() - 14.0, rect.center().y),
            Align2::RIGHT_CENTER,
            hint,
            Role::Callout,
            sub,
        );
    }
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::Default)
    } else {
        resp
    }
}

/// A hairline between groups of menu items.
pub fn menu_separator(ui: &mut Ui, st: &Style) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width().max(248.0), 9.0), Sense::hover());
    let y = r.center().y;
    ui.painter().line_segment(
        [pos2(r.left() + 10.0, y), pos2(r.right() - 10.0, y)],
        egui::Stroke::new(1.0, st.p.separator),
    );
}

/// A small heading inside a menu.
pub fn menu_title(ui: &mut Ui, st: &Style, text: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width().max(248.0), 24.0), Sense::hover());
    paint::text(
        ui.painter(),
        ui,
        pos2(r.left() + 14.0, r.center().y + 2.0),
        Align2::LEFT_CENTER,
        text,
        Role::Overline,
        st.p.text3,
    );
}

/// Rounded search field with a leading magnifier and a clear button.
pub fn search_field(
    ui: &mut Ui,
    st: &Style,
    id: Id,
    text: &mut String,
    hint: &str,
    width: f32,
    want_focus: bool,
) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 28.0), Sense::hover());
    let ctx = ui.ctx().clone();
    let focused = ctx.memory(|m| m.has_focus(id));
    let focus_t = motion::animate(
        &ctx,
        id.with("focus"),
        focused as u8 as f32,
        QUICK,
        st.reduced_motion,
    );
    let p = ui.painter().clone();
    p.rect_filled(
        rect,
        cr(radius::CONTROL + 1),
        paint::mix(st.p.fill, st.p.content, 0.6 * focus_t),
    );
    p.rect_stroke(
        rect,
        cr(radius::CONTROL + 1),
        egui::Stroke::new(
            1.0 + focus_t,
            paint::mix(Color32::TRANSPARENT, st.p.accent, focus_t),
        ),
        egui::StrokeKind::Inside,
    );
    icons::paint(
        &p,
        Icon::Search,
        pos2(rect.left() + 16.0, rect.center().y),
        14.0,
        st.p.text2,
    );

    let clear_w = if text.is_empty() { 0.0 } else { 24.0 };
    let edit_rect = Rect::from_min_max(
        pos2(rect.left() + 30.0, rect.top() + 2.0),
        pos2(
            rect.right()
                - 8.0
                - clear_w
                - if focused || !text.is_empty() {
                    0.0
                } else {
                    22.0
                },
            rect.bottom() - 2.0,
        ),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(edit_rect));
    let resp = child.add(
        egui::TextEdit::singleline(text)
            .id(id)
            .font(Role::Body.font())
            .hint_text(egui::RichText::new(hint).color(st.p.text3))
            .text_color(st.p.text)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::symmetric(0, 4))
            .desired_width(edit_rect.width()),
    );
    if want_focus {
        resp.request_focus();
    }
    if !focused && text.is_empty() {
        let kbd =
            Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), vec2(18.0, 18.0));
        p.rect_stroke(
            kbd,
            cr(4),
            egui::Stroke::new(1.0, st.p.separator),
            egui::StrokeKind::Inside,
        );
        paint::text(
            &p,
            ui,
            kbd.center(),
            Align2::CENTER_CENTER,
            "/",
            Role::Caption,
            st.p.text3,
        );
    }
    if !text.is_empty() {
        let c = pos2(rect.right() - 14.0, rect.center().y);
        let cr_rect = Rect::from_center_size(c, vec2(20.0, 20.0));
        let r = ui.interact(cr_rect, id.with("clear"), Sense::click());
        p.circle_filled(
            c,
            7.0,
            paint::mix(st.p.text3, st.p.text2, r.hovered() as u8 as f32),
        );
        icons::paint(&p, Icon::Close, c, 9.0, st.p.content);
        if r.clicked() {
            text.clear();
        }
        r.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Small rounded label, e.g. a method or a flag.
pub fn badge(ui: &mut Ui, text: &str, fg: Color32, bg: Color32) -> Response {
    let g = paint::galley(ui, text, Role::Caption, fg);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 12.0, 20.0), Sense::hover());
    ui.painter().rect_filled(rect, cr(5), bg);
    ui.painter().galley(
        pos2(rect.left() + 6.0, rect.center().y - g.size().y / 2.0),
        g,
        fg,
    );
    resp
}

/// Indeterminate progress. Rotates once a second; it is a status indicator,
/// not ambient decoration, so it is allowed under reduced motion as a
/// slower rotation.
pub fn spinner(ui: &mut Ui, st: &Style, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let t = ui.input(|i| i.time) as f32;
    let speed = if st.reduced_motion { 0.5 } else { 1.0 };
    let start = t * speed * std::f32::consts::TAU;
    let pts: Vec<_> = (0..=18)
        .map(|i| {
            let a = start + i as f32 / 18.0 * std::f32::consts::TAU * 0.75;
            rect.center() + vec2(a.cos(), a.sin()) * (size / 2.0 - 1.5)
        })
        .collect();
    ui.painter()
        .add(egui::Shape::line(pts, egui::Stroke::new(2.0, st.p.text2)));
    ui.ctx().request_repaint();
}
