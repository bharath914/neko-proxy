//! Vector icons painted with strokes on a unit square, so they stay crisp at
//! any scale and take the text color.

use egui::{pos2, Color32, Painter, Pos2, Rect, Shape, Stroke};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Search,
    Trash,
    Copy,
    Gear,
    List,
    Pause,
    Check,
    ChevronRight,
    ChevronDown,
    Close,
    Download,
    Lock,
    Globe,
    Terminal,
    Warning,
    Shield,
    Folder,
    Sun,
    Moon,
    Auto,
    Focus,
    File,
    Plus,
    Unlock,
    Sliders,
}

fn arc(c: Pos2, r: f32, from: f32, to: f32, n: usize) -> Vec<Pos2> {
    (0..=n)
        .map(|i| {
            let a = from + (to - from) * i as f32 / n as f32;
            pos2(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect()
}

pub fn paint(p: &Painter, icon: Icon, center: Pos2, size: f32, color: Color32) {
    let w = (size / 12.0).clamp(1.2, 2.0);
    let st = Stroke::new(w, color);
    let m = |x: f32, y: f32| pos2(center.x + x * size, center.y + y * size);
    let line = |pts: Vec<Pos2>| {
        p.add(Shape::line(pts, st));
    };
    let seg = |a: Pos2, b: Pos2| {
        p.line_segment([a, b], st);
    };
    let rrect = |x0: f32, y0: f32, x1: f32, y1: f32, r: f32| {
        p.rect_stroke(
            Rect::from_min_max(m(x0, y0), m(x1, y1)),
            r * size,
            st,
            egui::StrokeKind::Middle,
        );
    };
    match icon {
        Icon::Search => {
            p.circle_stroke(m(-0.08, -0.08), 0.28 * size, st);
            seg(m(0.13, 0.13), m(0.38, 0.38));
        }
        Icon::Trash => {
            seg(m(-0.36, -0.26), m(0.36, -0.26));
            line(vec![
                m(-0.12, -0.26),
                m(-0.12, -0.38),
                m(0.12, -0.38),
                m(0.12, -0.26),
            ]);
            line(vec![
                m(-0.27, -0.26),
                m(-0.22, 0.38),
                m(0.22, 0.38),
                m(0.27, -0.26),
            ]);
            seg(m(-0.09, -0.12), m(-0.08, 0.26));
            seg(m(0.09, -0.12), m(0.08, 0.26));
        }
        Icon::Copy => {
            rrect(-0.12, -0.12, 0.38, 0.38, 0.1);
            line(vec![
                m(-0.12, 0.2),
                m(-0.3, 0.2),
                m(-0.3, -0.3),
                m(0.2, -0.3),
                m(0.2, -0.12),
            ]);
        }
        Icon::Gear => {
            p.circle_stroke(m(0.0, 0.0), 0.17 * size, st);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let (s, c) = a.sin_cos();
                seg(m(c * 0.27, s * 0.27), m(c * 0.4, s * 0.4));
            }
            p.circle_stroke(m(0.0, 0.0), 0.27 * size, st);
        }
        Icon::List => {
            for y in [-0.25f32, 0.0, 0.25] {
                p.circle_filled(m(-0.32, y), 0.035 * size + 0.4, color);
                seg(m(-0.16, y), m(0.36, y));
            }
        }
        Icon::Pause => {
            for x in [-0.15f32, 0.15] {
                p.rect_filled(
                    Rect::from_center_size(m(x, 0.0), egui::vec2(0.14 * size, 0.5 * size)),
                    0.04 * size,
                    color,
                );
            }
        }
        Icon::Check => line(vec![m(-0.3, 0.02), m(-0.1, 0.22), m(0.32, -0.24)]),
        Icon::ChevronRight => line(vec![m(-0.1, -0.28), m(0.16, 0.0), m(-0.1, 0.28)]),
        Icon::ChevronDown => line(vec![m(-0.28, -0.1), m(0.0, 0.16), m(0.28, -0.1)]),
        Icon::Close => {
            seg(m(-0.24, -0.24), m(0.24, 0.24));
            seg(m(0.24, -0.24), m(-0.24, 0.24));
        }
        Icon::Download => {
            seg(m(0.0, -0.36), m(0.0, 0.16));
            line(vec![m(-0.2, -0.06), m(0.0, 0.16), m(0.2, -0.06)]);
            line(vec![
                m(-0.34, 0.14),
                m(-0.34, 0.36),
                m(0.34, 0.36),
                m(0.34, 0.14),
            ]);
        }
        Icon::Lock => {
            rrect(-0.28, -0.04, 0.28, 0.36, 0.08);
            let mut pts = arc(
                m(0.0, -0.04),
                0.17 * size,
                std::f32::consts::PI,
                std::f32::consts::TAU,
                12,
            );
            pts.insert(0, m(-0.17, -0.04));
            line(pts);
        }
        Icon::Globe => {
            p.circle_stroke(m(0.0, 0.0), 0.36 * size, st);
            seg(m(-0.36, 0.0), m(0.36, 0.0));
            let mut e = Vec::new();
            for i in 0..=20 {
                let a = i as f32 / 20.0 * std::f32::consts::TAU;
                e.push(m(0.16 * a.cos(), 0.36 * a.sin()));
            }
            line(e);
        }
        Icon::Terminal => {
            rrect(-0.38, -0.3, 0.38, 0.3, 0.1);
            line(vec![m(-0.2, -0.1), m(-0.04, 0.02), m(-0.2, 0.14)]);
            seg(m(0.04, 0.16), m(0.2, 0.16));
        }
        Icon::Warning => {
            line(vec![
                m(0.0, -0.36),
                m(0.38, 0.3),
                m(-0.38, 0.3),
                m(0.0, -0.36),
            ]);
            seg(m(0.0, -0.1), m(0.0, 0.1));
            p.circle_filled(m(0.0, 0.2), 0.03 * size + 0.4, color);
        }
        Icon::Shield => {
            line(vec![
                m(0.0, -0.38),
                m(0.32, -0.26),
                m(0.3, 0.06),
                m(0.0, 0.38),
                m(-0.3, 0.06),
                m(-0.32, -0.26),
                m(0.0, -0.38),
            ]);
            line(vec![m(-0.12, 0.0), m(-0.02, 0.1), m(0.15, -0.1)]);
        }
        Icon::Sun => {
            p.circle_stroke(m(0.0, 0.0), 0.17 * size, st);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let (s, c) = a.sin_cos();
                seg(m(c * 0.28, s * 0.28), m(c * 0.4, s * 0.4));
            }
        }
        Icon::Moon => {
            // A crescent: the outer circle's arc, closed by a smaller inner arc.
            let mut pts = arc(m(0.0, 0.0), 0.34 * size, -2.2, 2.2 + 0.0, 24);
            let inner = arc(m(0.17, -0.08), 0.27 * size, 1.5, -1.4, 18);
            pts.extend(inner);
            line(pts);
        }
        Icon::Auto => {
            p.circle_stroke(m(0.0, 0.0), 0.34 * size, st);
            // Right half filled: "follows the system".
            let mut half = arc(m(0.0, 0.0), 0.34 * size, -std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2, 16);
            half.push(m(0.0, -0.34));
            p.add(Shape::convex_polygon(half, color, Stroke::NONE));
        }
        Icon::Focus => {
            p.circle_stroke(m(0.0, 0.0), 0.24 * size, st);
            p.circle_filled(m(0.0, 0.0), 0.06 * size + 0.4, color);
            for (dx, dy) in [(0.0f32, -1.0f32), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)] {
                seg(m(dx * 0.3, dy * 0.3), m(dx * 0.42, dy * 0.42));
            }
        }
        Icon::File => {
            line(vec![
                m(-0.26, -0.4),
                m(0.1, -0.4),
                m(0.28, -0.22),
                m(0.28, 0.4),
                m(-0.26, 0.4),
                m(-0.26, -0.4),
            ]);
            line(vec![m(0.1, -0.4), m(0.1, -0.22), m(0.28, -0.22)]);
            seg(m(-0.12, 0.02), m(0.14, 0.02));
            seg(m(-0.12, 0.2), m(0.14, 0.2));
        }
        Icon::Plus => {
            seg(m(-0.3, 0.0), m(0.3, 0.0));
            seg(m(0.0, -0.3), m(0.0, 0.3));
        }
        Icon::Unlock => {
            rrect(-0.28, -0.04, 0.28, 0.36, 0.08);
            let mut pts = arc(
                m(0.0, -0.04),
                0.17 * size,
                std::f32::consts::PI,
                std::f32::consts::TAU - 0.5,
                12,
            );
            pts.insert(0, m(-0.17, -0.04));
            line(pts);
        }
        Icon::Sliders => {
            for (y, x) in [(-0.22f32, -0.12f32), (0.0, 0.14), (0.22, -0.04)] {
                seg(m(-0.36, y), m(0.36, y));
                p.circle_filled(m(x, y), 0.07 * size + 0.5, color);
            }
        }
        Icon::Folder => {
            line(vec![
                m(-0.38, -0.2),
                m(-0.38, 0.3),
                m(0.38, 0.3),
                m(0.38, -0.12),
                m(0.0, -0.12),
                m(-0.1, -0.26),
                m(-0.3, -0.26),
                m(-0.38, -0.2),
            ]);
        }
    }
}
