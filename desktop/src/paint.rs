//! Painting helpers shared by widgets and screens.

use crate::theme::{Role, Style};
use egui::epaint::{Mesh, Shadow, Vertex};
use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, Galley, Painter, Pos2, Rect, Stroke, StrokeKind, Ui,
};
use std::sync::Arc;

pub fn cr(r: u8) -> CornerRadius {
    CornerRadius::same(r)
}

/// Lay out one line of text in a role, with its tracking.
pub fn galley(ui: &Ui, text: &str, role: Role, color: Color32) -> Arc<Galley> {
    galley_wrapped(ui, text, role, color, f32::INFINITY, 1, false)
}

pub fn galley_wrapped(
    ui: &Ui,
    text: &str,
    role: Role,
    color: Color32,
    max_width: f32,
    max_rows: usize,
    break_anywhere: bool,
) -> Arc<Galley> {
    let spec = role.spec();
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat {
            font_id: role.font(),
            color,
            extra_letter_spacing: spec.tracking,
            line_height: Some(spec.line),
            ..Default::default()
        },
    );
    job.wrap = TextWrapping {
        max_width,
        max_rows,
        break_anywhere,
        overflow_character: if max_rows == 1 { Some('…') } else { None },
    };
    ui.ctx().fonts_mut(|f| f.layout_job(job))
}

/// Paint text anchored at `pos`; returns the rect it covers.
pub fn text(
    painter: &Painter,
    ui: &Ui,
    pos: Pos2,
    anchor: Align2,
    s: &str,
    role: Role,
    color: Color32,
) -> Rect {
    let g = galley(ui, s, role, color);
    let rect = anchor.anchor_size(pos, g.size());
    painter.galley(rect.min, g, color);
    rect
}

/// Single line, truncated with an ellipsis to fit `max_w`, vertically centered.
pub fn text_fit(
    painter: &Painter,
    ui: &Ui,
    left_center: Pos2,
    max_w: f32,
    s: &str,
    role: Role,
    color: Color32,
) {
    if max_w <= 2.0 || s.is_empty() {
        return;
    }
    let g = galley_wrapped(ui, s, role, color, max_w, 1, true);
    let pos = pos2(left_center.x, left_center.y - g.size().y / 2.0);
    painter.galley(pos, g, color);
}

pub fn text_fit_right(
    painter: &Painter,
    ui: &Ui,
    right_center: Pos2,
    max_w: f32,
    s: &str,
    role: Role,
    color: Color32,
) {
    if max_w <= 2.0 || s.is_empty() {
        return;
    }
    let g = galley_wrapped(ui, s, role, color, max_w, 1, true);
    let pos = pos2(
        right_center.x - g.size().x,
        right_center.y - g.size().y / 2.0,
    );
    painter.galley(pos, g, color);
}

/// Vertical gradient between two colors.
pub fn vgradient(painter: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    let uv = egui::epaint::WHITE_UV;
    mesh.vertices.push(Vertex {
        pos: rect.left_top(),
        uv,
        color: top,
    });
    mesh.vertices.push(Vertex {
        pos: rect.right_top(),
        uv,
        color: top,
    });
    mesh.vertices.push(Vertex {
        pos: rect.right_bottom(),
        uv,
        color: bottom,
    });
    mesh.vertices.push(Vertex {
        pos: rect.left_bottom(),
        uv,
        color: bottom,
    });
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
}

/// A soft edge where content passes under chrome, instead of a hard rule.
pub fn edge_fade_below(painter: &Painter, st: &Style, rect: Rect, height: f32) {
    let c = if st.p.is_dark() {
        Color32::from_black_alpha(70)
    } else {
        Color32::from_black_alpha(16)
    };
    let r = Rect::from_min_size(rect.left_bottom(), vec2(rect.width(), height));
    vgradient(painter, r, c, Color32::TRANSPARENT);
}

/// Depth for floating surfaces: bigger surfaces cast deeper shadows.
pub fn shadow(painter: &Painter, st: &Style, rect: Rect, radius: u8, level: u8) {
    let (blur, offset, alpha) = match level {
        0 => (6u8, 1i8, 28u8),
        1 => (16, 4, 40),
        _ => (36, 12, 64),
    };
    let alpha = if st.p.is_dark() {
        alpha.saturating_mul(2)
    } else {
        alpha
    };
    let s = Shadow {
        offset: [0, offset],
        blur,
        spread: 0,
        color: Color32::from_black_alpha(alpha),
    };
    painter.add(s.as_shape(rect, cr(radius)));
}

pub fn hairline(painter: &Painter, st: &Style, a: Pos2, b: Pos2) {
    painter.line_segment([a, b], Stroke::new(1.0, st.p.separator));
}

pub fn ring(painter: &Painter, st: &Style, rect: Rect, radius: u8) {
    painter.rect_stroke(
        rect.expand(1.0),
        cr(radius + 1),
        Stroke::new(2.0, st.p.accent),
        StrokeKind::Outside,
    );
}

pub fn scale_rect(rect: Rect, s: f32) -> Rect {
    Rect::from_center_size(rect.center(), rect.size() * s)
}

pub fn with_alpha(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a.clamp(0.0, 1.0))
}

/// Linear blend of two colors.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}
