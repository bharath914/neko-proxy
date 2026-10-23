//! Sequence: the flow list on a shared time axis, in the order requests began.
//! Each row is one flow; its bar starts where the request started and runs for
//! its duration, so overlap, gaps and the order of calls read at a glance.

use super::table::ROW_H;
use crate::app::App;
use crate::fmt;
use crate::paint;
use crate::theme::Role;
use egui::{pos2, vec2, Align2, Rect, ScrollArea, Sense, Ui, UiBuilder};

const MAX_ROWS: usize = 20_000;

impl App {
    pub fn sequence(&mut self, ui: &mut Ui, full: Rect) {
        let st = self.st.clone();
        let total = self.table.total().min(MAX_ROWS);
        if total == 0 {
            for (gen, page) in self.table.wanted(0..0) {
                self.request_page(gen, page);
            }
            paint::text(
                ui.painter(),
                ui,
                full.center(),
                Align2::CENTER_CENTER,
                "No flows yet",
                Role::Title,
                st.p.text3,
            );
            return;
        }

        // The axis covers every loaded row; flows arrive in start order.
        let mut origin = i64::MAX;
        let mut end = i64::MIN;
        for row in 0..total {
            if let Some(s) = self.table.get(row) {
                origin = origin.min(s.started_at_ms);
                end = end.max(s.started_at_ms + s.duration_ms.max(0.0) as i64);
            }
        }
        if origin == i64::MAX {
            origin = 0;
            end = 1;
        }
        let span = (end - origin).max(1) as f32;

        let head_h = 30.0;
        let head = Rect::from_min_size(full.min, vec2(full.width(), head_h));
        let body = Rect::from_min_max(pos2(full.left(), head.bottom()), full.max);
        let pad = 16.0;
        let num_w = 44.0;
        let label_w = (full.width() * 0.34).clamp(220.0, 420.0);
        let bar_l = full.left() + pad + num_w + label_w;
        let bar_r = full.right() - pad - 72.0;

        let p = ui.painter().clone();
        p.rect_filled(head, 0.0, st.p.toolbar);
        paint::hairline(&p, &st, head.left_bottom(), head.right_bottom());
        let hc = head.center().y;
        paint::text(&p, ui, pos2(full.left() + pad, hc), Align2::LEFT_CENTER, "#", Role::Overline, st.p.text3);
        paint::text(&p, ui, pos2(full.left() + pad + num_w, hc), Align2::LEFT_CENTER, "REQUEST", Role::Overline, st.p.text3);
        for i in 0..=4 {
            let x = bar_l + (bar_r - bar_l) * i as f32 / 4.0;
            let ms = span as f64 * i as f64 / 4.0;
            let (anchor, x) = match i {
                0 => (Align2::LEFT_CENTER, x),
                4 => (Align2::RIGHT_CENTER, x),
                _ => (Align2::CENTER_CENTER, x),
            };
            let label = if i == 0 { "0".to_string() } else { fmt::duration(ms) };
            paint::text(&p, ui, pos2(x, hc), anchor, &label, Role::MonoSmall, st.p.text3);
        }

        let mut clicked: Option<u64> = None;
        let mut child = ui.new_child(UiBuilder::new().max_rect(body));
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        child.spacing_mut().item_spacing.y = 0.0;
        ScrollArea::vertical()
            .id_salt("sequence-rows")
            .auto_shrink([false, false])
            .show_rows(&mut child, ROW_H, total, |ui, range| {
                for (gen, page) in self.table.wanted(range.clone()) {
                    self.request_page(gen, page);
                }
                for row in range {
                    let (r, resp) =
                        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                    let p = ui.painter();
                    let Some(s) = self.table.get(row).cloned() else {
                        continue;
                    };
                    let selected = self.selected == Some(s.id);
                    if selected {
                        p.rect_filled(r, 0.0, st.p.accent_soft);
                    } else if resp.hovered() {
                        p.rect_filled(r, 0.0, st.p.fill);
                    } else if row % 2 == 1 {
                        p.rect_filled(r, 0.0, st.p.row_alt);
                    }
                    let cy = r.center().y;
                    paint::text(p, ui, pos2(r.left() + pad, cy), Align2::LEFT_CENTER, &s.id.to_string(), Role::MonoSmall, st.p.text3);
                    let method = paint::text(p, ui, pos2(r.left() + pad + num_w, cy), Align2::LEFT_CENTER, &s.method, Role::Caption, st.p.method(&s.method));
                    let tx = method.right() + 8.0;
                    let name = format!("{}{}", s.host, s.path);
                    paint::text_fit(p, ui, pos2(tx, cy), bar_l - tx - 12.0, &name, Role::Body, st.p.text);

                    let failed = s.state == pb::FlowState::Failed as i32;
                    let color = st.p.status(s.status, failed);
                    let x0 = bar_l + (bar_r - bar_l) * (s.started_at_ms - origin) as f32 / span;
                    let w = ((bar_r - bar_l) * s.duration_ms.max(0.0) as f32 / span).max(3.0);
                    let bar = Rect::from_min_size(pos2(x0, cy - 5.0), vec2(w, 10.0));
                    p.rect_filled(bar, 5.0, color);
                    paint::text_fit_right(p, ui, pos2(r.right() - pad, cy), 64.0, &fmt::duration(s.duration_ms), Role::MonoSmall, st.p.text2);
                    if resp.clicked() {
                        clicked = Some(s.id);
                    }
                }
            });
        if let Some(id) = clicked {
            self.select(Some(id));
            self.traffic_seq = false;
        }
    }
}

use proto::pb;
