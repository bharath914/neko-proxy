use super::icons::{self, Icon};
use crate::app::App;
use crate::fmt;
use crate::motion::{self, QUICK};
use crate::paint::{self, cr};
use crate::theme::{radius, Role};
use egui::{pos2, vec2, Align2, Color32, Rect, Sense, Ui, UiBuilder};
use proto::pb;

pub const ROW_H: f32 = 30.0;
pub const HEADER_H: f32 = 30.0;

struct Cols {
    id: f32,
    method: f32,
    host: f32,
    path: f32,
    status: f32,
    kind: f32,
    size: f32,
    time: f32,
}

/// Columns drop away as the table narrows, least important first.
fn cols(w: f32) -> Cols {
    let kind = if w >= 720.0 { 54.0 } else { 0.0 };
    let size = if w >= 600.0 { 68.0 } else { 0.0 };
    let (id, method, status, time) = (42.0, 66.0, 60.0, 66.0);
    let fixed = id + method + status + time + kind + size;
    let flex = (w - fixed - 28.0).max(120.0);
    Cols {
        id,
        method,
        host: flex * 0.5,
        path: flex * 0.5,
        status,
        kind,
        size,
        time,
    }
}

impl App {
    /// Scroll so `row` is fully visible.
    pub fn reveal_row(&mut self, row: usize) {
        let top = row as f32 * ROW_H;
        let bottom = top + ROW_H;
        let view_h = (self.view_height - 0.0).max(ROW_H);
        if top < self.view_offset {
            self.scroll_to = Some(top);
        } else if bottom > self.view_offset + view_h {
            self.scroll_to = Some(bottom - view_h);
        }
        self.follow = false;
    }

    pub fn table(&mut self, ui: &mut Ui, rect: Rect) {
        let st = self.st.clone();
        let p = ui.painter().clone();
        let c = cols(rect.width());
        let header = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER_H));
        let body = Rect::from_min_max(header.left_bottom(), rect.right_bottom());

        // Rows first, so the header sits on top and content scrolls under it.
        let total = self.table.total();
        let mut child = ui.new_child(UiBuilder::new().max_rect(body));
        child.set_clip_rect(body.intersect(ui.clip_rect()));

        if total == 0 {
            // The total is learned from the first page, so ask for it even
            // though no row is drawn yet.
            for (gen, page) in self.table.wanted(0..0) {
                self.request_page(gen, page);
            }
            self.empty_state(&mut child, body);
        } else {
            let mut clicked: Option<u64> = None;
            let want_bottom = self.follow && total != self.last_total;
            let mut area = egui::ScrollArea::vertical()
                .id_salt("flow-rows")
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded);
            if let Some(y) = self.scroll_to.take() {
                area = area.vertical_scroll_offset(y.max(0.0));
            } else if want_bottom {
                let content = total as f32 * ROW_H;
                area = area.vertical_scroll_offset((content - body.height()).max(0.0));
            }
            child.spacing_mut().item_spacing.y = 0.0;
            let out = area.show_rows(&mut child, ROW_H, total, |ui, range| {
                for (gen, page) in self.table.wanted(range.clone()) {
                    self.request_page(gen, page);
                }
                for row in range {
                    let Some(s) = self.table.get(row).cloned() else {
                        // Placeholder while the page is on its way.
                        let (r, _) = ui
                            .allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
                        if row % 2 == 1 {
                            ui.painter().rect_filled(r, 0.0, st.p.row_alt);
                        }
                        continue;
                    };
                    if self.row(ui, row, &s, &c) {
                        clicked = Some(s.id);
                    }
                }
            });
            self.view_offset = out.state.offset.y;
            self.view_height = out.inner_rect.height();
            let content_h = out.content_size.y;
            let at_bottom = self.view_offset + self.view_height >= content_h - 6.0;
            if total > self.last_total && !self.follow {
                self.unseen += total - self.last_total;
            }
            self.follow = at_bottom || want_bottom;
            if self.follow {
                self.unseen = 0;
            }
            self.last_total = total;
            if let Some(id) = clicked {
                self.select(Some(id));
                ui.memory_mut(|m| m.surrender_focus(self.id("search")));
            }
        }

        // Keep selection reachable when it scrolls out of the loaded set.
        if self.table.total() == 0 {
            self.last_total = 0;
        }

        // Header: a material layer over the rows, with an edge fade instead of a rule.
        p.rect_filled(header, 0.0, st.p.panel);
        let mut x = rect.left() + 14.0;
        let y = header.center().y;
        let mut head = |label: &str, w: f32, right: bool| {
            let anchor = if right {
                Align2::RIGHT_CENTER
            } else {
                Align2::LEFT_CENTER
            };
            let px = if right { x + w - 8.0 } else { x };
            paint::text(
                &p,
                ui,
                pos2(px, y),
                anchor,
                label,
                Role::Overline,
                st.p.text3,
            );
            x += w;
        };
        head("#", c.id, false);
        head("METHOD", c.method, false);
        head("HOST", c.host, false);
        head("PATH", c.path, false);
        head("STATUS", c.status, false);
        if c.kind > 0.0 {
            head("TYPE", c.kind, false);
        }
        if c.size > 0.0 {
            head("SIZE", c.size, true);
        }
        head("TIME", c.time, true);
        paint::hairline(&p, &st, header.left_bottom(), header.right_bottom());
        paint::edge_fade_below(&p, &st, header, 6.0);

        // "Latest" pill when new flows arrived below the fold.
        if !self.follow && self.unseen > 0 {
            let label = format!("{} new", fmt::count(self.unseen));
            let g = paint::galley(ui, &label, Role::BodyMedium, st.p.on_accent);
            let w = g.size().x + 44.0;
            let r =
                Rect::from_center_size(pos2(rect.center().x, rect.bottom() - 28.0), vec2(w, 32.0));
            let resp = ui.interact(r, self.id("latest"), Sense::click());
            let press = motion::animate(
                ui.ctx(),
                self.id("latest-p"),
                resp.is_pointer_button_down_on() as u8 as f32,
                QUICK,
                st.reduced_motion,
            );
            let rr = paint::scale_rect(r, 1.0 - 0.04 * press);
            paint::shadow(&p, &st, rr, 16, 1);
            p.rect_filled(rr, cr(255), st.p.accent);
            icons::paint(
                &p,
                Icon::ChevronDown,
                pos2(rr.left() + 18.0, rr.center().y),
                14.0,
                st.p.on_accent,
            );
            p.galley(
                pos2(rr.left() + 32.0, rr.center().y - g.size().y / 2.0),
                g,
                st.p.on_accent,
            );
            if resp.clicked() {
                self.follow = true;
                self.unseen = 0;
                let content = self.table.total() as f32 * ROW_H;
                self.scroll_to = Some((content - self.view_height).max(0.0));
            }
            resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        }
    }

    /// One row. Returns true when it was pressed.
    fn row(&mut self, ui: &mut Ui, row: usize, s: &pb::FlowSummary, c: &Cols) -> bool {
        let st = &self.st;
        let (rect, resp) =
            ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
        if !ui.is_rect_visible(rect) {
            return false;
        }
        let selected = self.selected == Some(s.id);
        let p = ui.painter();
        // Respond on pointer-down: the row selects before the button comes up.
        let pressed = resp.is_pointer_button_down_on();
        if selected {
            p.rect_filled(
                rect.shrink2(vec2(6.0, 1.0)),
                cr(radius::CONTROL),
                st.p.accent,
            );
        } else if pressed {
            p.rect_filled(
                rect.shrink2(vec2(6.0, 1.0)),
                cr(radius::CONTROL),
                st.p.fill_press,
            );
        } else if resp.hovered() {
            p.rect_filled(rect.shrink2(vec2(6.0, 1.0)), cr(radius::CONTROL), st.p.fill);
        } else if row % 2 == 1 {
            p.rect_filled(rect, 0.0, st.p.row_alt);
        }

        let on = |normal: Color32| if selected { st.p.on_accent } else { normal };
        let dim = |normal: Color32| {
            if selected {
                paint::with_alpha(st.p.on_accent, 0.78)
            } else {
                normal
            }
        };
        let failed = s.state == pb::FlowState::Failed as i32;
        let pending =
            s.state == pb::FlowState::Pending as i32 || s.state == pb::FlowState::Receiving as i32;
        let mid = rect.center().y;
        let mut x = rect.left() + 14.0;

        paint::text_fit(
            p,
            ui,
            pos2(x, mid),
            c.id - 8.0,
            &s.id.to_string(),
            Role::MonoSmall,
            dim(st.p.text3),
        );
        x += c.id;
        paint::text_fit(
            p,
            ui,
            pos2(x, mid),
            c.method - 6.0,
            &s.method,
            Role::Caption,
            on(st.p.method(&s.method)),
        );
        x += c.method;

        // Pinned and tunnel markers share the host cell.
        let mut host_x = x;
        if s.flags & 1 != 0 {
            icons::paint(
                p,
                Icon::Lock,
                pos2(host_x + 6.0, mid),
                13.0,
                on(st.p.orange),
            );
            host_x += 18.0;
        }
        paint::text_fit(
            p,
            ui,
            pos2(host_x, mid),
            c.host - 12.0 - (host_x - x),
            &s.host,
            Role::Body,
            on(st.p.text),
        );
        x += c.host;
        let path = if s.path.is_empty() { "tunnel" } else { &s.path };
        paint::text_fit(
            p,
            ui,
            pos2(x, mid),
            c.path - 12.0,
            path,
            Role::Body,
            dim(st.p.text2),
        );
        x += c.path;

        // Status: dot + code. Pending flows show a quiet ellipsis.
        if pending && s.status == 0 {
            paint::text(
                p,
                ui,
                pos2(x, mid),
                Align2::LEFT_CENTER,
                "···",
                Role::Mono,
                dim(st.p.text3),
            );
        } else if s.status == 0 {
            let col = if failed { st.p.red } else { st.p.text3 };
            paint::text(
                p,
                ui,
                pos2(x, mid),
                Align2::LEFT_CENTER,
                "—",
                Role::Mono,
                on(col),
            );
        } else {
            let col = st.p.status(s.status, failed);
            p.circle_filled(
                pos2(x + 4.0, mid),
                3.5,
                if selected { st.p.on_accent } else { col },
            );
            paint::text(
                p,
                ui,
                pos2(x + 14.0, mid),
                Align2::LEFT_CENTER,
                &s.status.to_string(),
                Role::Mono,
                on(st.p.text),
            );
        }
        x += c.status;
        if c.kind > 0.0 {
            paint::text_fit(
                p,
                ui,
                pos2(x, mid),
                c.kind - 6.0,
                &s.content_type,
                Role::Callout,
                dim(st.p.text2),
            );
            x += c.kind;
        }
        if c.size > 0.0 {
            let size = if s.res_size > 0 {
                fmt::size(s.res_size)
            } else {
                "—".into()
            };
            paint::text_fit_right(
                p,
                ui,
                pos2(x + c.size - 8.0, mid),
                c.size - 8.0,
                &size,
                Role::MonoSmall,
                dim(st.p.text2),
            );
            x += c.size;
        }
        let slow = s.duration_ms >= 1000.0;
        let tcol = if slow { st.p.orange } else { st.p.text2 };
        // A flow with no response has no meaningful duration to show.
        let time = if (pending && s.duration_ms == 0.0) || (s.status == 0 && s.duration_ms < 1.0) {
            "".into()
        } else {
            fmt::duration(s.duration_ms)
        };
        paint::text_fit_right(
            p,
            ui,
            pos2(x + c.time - 8.0, mid),
            c.time - 8.0,
            &time,
            Role::MonoSmall,
            if selected { st.p.on_accent } else { tcol },
        );

        resp.is_pointer_button_down_on()
    }

    fn empty_state(&mut self, ui: &mut Ui, area: Rect) {
        let st = self.st.clone();
        let p = ui.painter().clone();
        let center = area.center() + vec2(0.0, -20.0);
        let filtered = !self.search.is_empty() || self.table.kind != 0;
        let (title, body) = if !self.table.loaded() {
            ("Loading flows…", String::new())
        } else if filtered {
            (
                "No matching flows",
                "Try a different search, or switch the filter back to All.".to_string(),
            )
        } else if matches!(self.conn, crate::backend::ConnState::Connected) {
            (
                "Waiting for traffic",
                format!(
                    "Point a client at {} and requests appear here as they happen.",
                    self.proxy_addr()
                ),
            )
        } else {
            (
                "Connecting to the core…",
                "This takes a moment on first launch.".to_string(),
            )
        };
        p.circle_filled(center, 30.0, st.p.fill);
        icons::paint(&p, Icon::List, center, 28.0, st.p.text3);
        paint::text(
            &p,
            ui,
            center + vec2(0.0, 52.0),
            Align2::CENTER_CENTER,
            title,
            Role::Title,
            st.p.text,
        );
        let g = paint::galley_wrapped(ui, &body, Role::Body, st.p.text2, 360.0, 3, false);
        p.galley(
            pos2(center.x - g.size().x / 2.0, center.y + 76.0),
            g.clone(),
            st.p.text2,
        );
        // One clear next step.
        if !filtered
            && matches!(self.conn, crate::backend::ConnState::Connected)
            && self.table.loaded()
        {
            let r = Rect::from_center_size(
                center + vec2(0.0, 76.0 + g.size().y + 30.0),
                vec2(190.0, 30.0),
            );
            let mut child = ui.new_child(UiBuilder::new().max_rect(r));
            if super::widgets::button(
                &mut child,
                &st,
                "Set up a browser or terminal",
                super::widgets::Kind::Secondary,
                None,
                true,
            )
            .clicked()
            {
                self.section = crate::app::Section::Settings;
            }
        }
    }
}
