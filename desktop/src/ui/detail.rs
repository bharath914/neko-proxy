use super::icons::Icon;
use super::widgets::{self, Kind};
use crate::app::{App, Section, Tab};
use crate::backend::ConnState;
use crate::fmt;
use crate::motion::{self, MOVE};
use crate::paint::{self, cr};
use crate::theme::{radius, Role, Style};
use egui::text::{LayoutJob, TextFormat};
use egui::{
    pos2, vec2, Align2, Color32, Frame, Margin, Rect, ScrollArea, Sense, Stroke, Ui, UiBuilder,
};
use proto::pb;
use std::sync::Arc;

const DETAIL_MIN: f32 = 340.0;
const TABLE_MIN: f32 = 440.0;

fn job(text: &str, role: Role, color: Color32) -> LayoutJob {
    let s = role.spec();
    LayoutJob::single_section(
        text.to_owned(),
        TextFormat {
            font_id: role.font(),
            color,
            extra_letter_spacing: s.tracking,
            line_height: Some(s.line),
            ..Default::default()
        },
    )
}

fn selectable(ui: &mut Ui, text: &str, role: Role, color: Color32) {
    ui.add(
        egui::Label::new(job(text, role, color))
            .wrap()
            .selectable(true),
    );
}

fn caption(ui: &mut Ui, st: &Style, text: &str) {
    ui.add(
        egui::Label::new(job(&text.to_uppercase(), Role::Overline, st.p.text3)).selectable(false),
    );
}

fn card(ui: &mut Ui, st: &Style, add: impl FnOnce(&mut Ui)) {
    let shadow = if st.p.is_dark() {
        egui::epaint::Shadow::NONE
    } else {
        egui::epaint::Shadow {
            offset: [0, 1],
            blur: 3,
            spread: 0,
            color: Color32::from_black_alpha(14),
        }
    };
    Frame::new()
        .fill(st.p.card)
        .stroke(Stroke::new(1.0, st.p.separator))
        .shadow(shadow)
        .corner_radius(cr(radius::PANEL))
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

fn kv(ui: &mut Ui, st: &Style, label: &str, value: &str, mono: bool) {
    ui.horizontal_top(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(108.0, 20.0), Sense::hover());
        paint::text(
            ui.painter(),
            ui,
            pos2(r.left(), r.center().y),
            Align2::LEFT_CENTER,
            label,
            Role::Callout,
            st.p.text2,
        );
        let role = if mono { Role::Mono } else { Role::Body };
        selectable(
            ui,
            if value.is_empty() { "—" } else { value },
            role,
            st.p.text,
        );
    });
}

fn banner(ui: &mut Ui, st: &Style, color: Color32, icon: Icon, title: &str, body: &str) {
    Frame::new()
        .fill(st.p.tint(color, 0.14))
        .stroke(Stroke::new(1.0, st.p.tint(color, 0.38)))
        .corner_radius(cr(radius::CARD))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                super::icons::paint(ui.painter(), icon, r.center(), 16.0, color);
                ui.vertical(|ui| {
                    ui.add(
                        egui::Label::new(job(title, Role::Headline, st.p.text)).selectable(false),
                    );
                    ui.add(
                        egui::Label::new(job(body, Role::Callout, st.p.text2))
                            .wrap()
                            .selectable(true),
                    );
                });
            });
        });
}

impl App {
    /// The Inspector screen: toolbar, flow table, divider, detail pane.
    pub fn inspector(&mut self, ui: &mut Ui) {
        let st = self.st.clone();
        let mut body = ui.max_rect();

        // Connection problems are said once, at the top, with a way out.
        if let ConnState::Failed(msg) = self.conn.clone() {
            let r = Rect::from_min_size(body.min, vec2(body.width(), 46.0));
            ui.painter()
                .rect_filled(r, 0.0, st.p.tint(st.p.red, 0.14));
            paint::hairline(ui.painter(), &st, r.left_bottom(), r.right_bottom());
            super::icons::paint(
                ui.painter(),
                Icon::Warning,
                pos2(r.left() + 24.0, r.center().y),
                16.0,
                st.p.red,
            );
            paint::text(
                ui.painter(),
                ui,
                pos2(r.left() + 44.0, r.center().y - 8.0),
                Align2::LEFT_CENTER,
                "The proxy core is not running",
                Role::Headline,
                st.p.text,
            );
            paint::text_fit(
                ui.painter(),
                ui,
                pos2(r.left() + 44.0, r.center().y + 9.0),
                r.width() - 60.0,
                &msg,
                Role::Callout,
                st.p.text2,
            );
            body.min.y = r.bottom();
        }

        // Divider: the pane follows the pointer 1:1, resists past its bounds,
        // and settles on a spring that inherits the release velocity.
        let max_w = (body.width() - TABLE_MIN).max(DETAIL_MIN);
        // Until the person drags it, the pane takes about a third of the room:
        // the list is the main content.
        if self.detail_raw <= 0.0 {
            self.detail_raw = (body.width() * 0.34).clamp(DETAIL_MIN, 440.0);
        }
        let did = self.id("detail-width");
        let hit = Rect::from_min_max(
            pos2(
                body.right() - self.detail_raw.clamp(DETAIL_MIN, max_w) - 5.0,
                body.top(),
            ),
            pos2(
                body.right() - self.detail_raw.clamp(DETAIL_MIN, max_w) + 5.0,
                body.bottom(),
            ),
        );
        let shown_now = motion::with_spring(
            ui.ctx(),
            did,
            self.detail_raw.clamp(DETAIL_MIN, max_w),
            |s| s.value,
        );
        let hit = if shown_now.is_finite() {
            Rect::from_min_max(
                pos2(body.right() - shown_now - 5.0, body.top()),
                pos2(body.right() - shown_now + 5.0, body.bottom()),
            )
        } else {
            hit
        };
        let resp = ui.interact(hit, self.id("divider"), Sense::click_and_drag());
        let ctx = ui.ctx().clone();
        if resp.dragged() {
            self.detail_raw -= resp.drag_delta().x;
            let clamped = self.detail_raw.clamp(DETAIL_MIN, max_w);
            let over = self.detail_raw - clamped;
            let shown = clamped + motion::rubber_band(over, 240.0);
            motion::with_spring(&ctx, did, shown, |s| {
                s.value = shown;
                s.target = shown;
                s.velocity = 0.0;
            });
        } else if resp.drag_stopped() {
            let vel = -ctx.input(|i| i.pointer.velocity().x);
            let at = self.detail_raw.clamp(DETAIL_MIN, max_w);
            // A flick throws the pane to the snap point nearest where its
            // momentum would carry it; a slow release stays where it was put.
            let flick = vel.abs() > 700.0;
            let target = if flick {
                let landing = at + motion::project(vel).clamp(-600.0, 600.0);
                [DETAIL_MIN, (body.width() * 0.34).clamp(DETAIL_MIN, max_w), max_w]
                    .into_iter()
                    .min_by(|a, b| (a - landing).abs().total_cmp(&(b - landing).abs()))
                    .unwrap_or(at)
            } else {
                at
            };
            self.detail_raw = target;
            self.prefs.detail_width = target;
            self.prefs.save();
            ctx.data_mut(|d| d.insert_temp(did.with("flick"), flick));
            motion::with_spring(&ctx, did, at, |s| {
                s.velocity = vel;
                s.target = target;
            });
        }
        if resp.drag_started() {
            ctx.data_mut(|d| d.insert_temp(did.with("flick"), false));
        }
        if resp.double_clicked() {
            self.detail_raw = (body.width() * 0.34).clamp(DETAIL_MIN, 440.0);
        }
        let target = self.detail_raw.clamp(DETAIL_MIN, max_w);
        let detail_w = if resp.dragged() {
            motion::with_spring(&ctx, did, target, |s| s.value)
        } else {
            // Under-damped only when the release carried momentum.
            let flick: bool = ctx.data(|d| d.get_temp(did.with("flick"))).unwrap_or(false);
            let cfg = if flick { motion::MOMENTUM } else { MOVE };
            motion::animate(&ctx, did, target, cfg, st.reduced_motion)
        };
        let split_x = body.right() - detail_w;
        let table_rect = Rect::from_min_max(body.min, pos2(split_x, body.bottom()));
        let detail_rect = Rect::from_min_max(pos2(split_x, body.top()), body.max);

        self.table(ui, table_rect);
        self.detail(ui, detail_rect);

        let p = ui.painter();
        paint::hairline(
            p,
            &st,
            pos2(split_x, body.top()),
            pos2(split_x, body.bottom()),
        );
        let a = motion::fade(
            &ctx,
            self.id("divider-fade"),
            resp.hovered() || resp.dragged(),
        );
        if a > 0.0 {
            p.line_segment(
                [pos2(split_x, body.top()), pos2(split_x, body.bottom())],
                Stroke::new(2.0, paint::with_alpha(st.p.accent, a * 0.8)),
            );
        }
        if resp.hovered() || resp.dragged() {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
    }

    pub fn detail(&mut self, ui: &mut Ui, rect: Rect) {
        let st = self.st.clone();
        ui.painter().rect_filled(rect, 0.0, st.p.panel);
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect.shrink2(vec2(0.0, 0.0))));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));

        let Some(id) = self.selected else {
            let c = rect.center();
            let p = child.painter().clone();
            paint::text(
                &p,
                &child,
                c + vec2(0.0, -6.0),
                Align2::CENTER_CENTER,
                "No flow selected",
                Role::Title,
                st.p.text,
            );
            paint::text(
                &p,
                &child,
                c + vec2(0.0, 18.0),
                Align2::CENTER_CENTER,
                "Select a request to see its headers, body and timing.",
                Role::Callout,
                st.p.text2,
            );
            return;
        };
        let summary = self
            .table
            .row_of(id)
            .and_then(|r| self.table.get(r))
            .cloned();
        let entry_summary = self.details.get(&id).and_then(|e| e.detail.summary.clone());
        let Some(s) = summary.or(entry_summary) else {
            let p = child.painter().clone();
            paint::text(
                &p,
                &child,
                rect.center(),
                Align2::CENTER_CENTER,
                "This flow is no longer in the current list.",
                Role::Callout,
                st.p.text2,
            );
            return;
        };

        // Header block.
        let pad = 16.0;
        let content = rect.shrink2(vec2(pad, 0.0));
        let mut head = child.new_child(UiBuilder::new().max_rect(Rect::from_min_max(
            pos2(content.left(), rect.top() + 14.0),
            pos2(content.right(), rect.bottom()),
        )));
        head.spacing_mut().item_spacing = vec2(8.0, 6.0);
        let failed = s.state == pb::FlowState::Failed as i32;
        head.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let mc = st.p.method(&s.method);
            widgets::badge(ui, &s.method, mc, st.p.tint(mc, 0.16));
            let in_flight = s.state == pb::FlowState::Pending as i32
                || s.state == pb::FlowState::Receiving as i32;
            if s.status != 0 {
                let c = st.p.status(s.status, failed);
                let txt = format!("{} {}", s.status, fmt::status_text(s.status))
                    .trim()
                    .to_string();
                widgets::badge(ui, &txt, c, st.p.tint(c, 0.16));
            } else if failed {
                widgets::badge(ui, "Failed", st.p.red, st.p.tint(st.p.red, 0.16));
            } else if !in_flight {
                let label = if s.flags & 1 != 0 {
                    "Pinned tunnel"
                } else {
                    "Tunnel"
                };
                widgets::badge(ui, label, st.p.orange, st.p.tint(st.p.orange, 0.18));
            } else {
                ui.spacing_mut().item_spacing.x = 6.0;
                widgets::spinner(ui, &st, 14.0);
                ui.add(
                    egui::Label::new(job("Waiting for response", Role::Callout, st.p.text2))
                        .selectable(false),
                );
            }
            if !s.protocol.is_empty() && s.protocol != "tunnel" {
                widgets::badge(ui, &s.protocol, st.p.text2, st.p.fill);
            }
        });
        let host_port = if s.port != 443 && s.port != 80 && s.port != 0 {
            format!("{}:{}", s.host, s.port)
        } else {
            s.host.clone()
        };
        head.add(
            egui::Label::new(job(&host_port, Role::Title, st.p.text))
                .wrap()
                .selectable(true),
        );
        if !s.path.is_empty() {
            head.add(
                egui::Label::new(job(&s.path, Role::Callout, st.p.text2))
                    .wrap()
                    .selectable(true),
            );
        }
        head.add_space(2.0);
        let detail_ready = self.details.contains_key(&id);
        head.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let curl = self
                .details
                .get(&id)
                .map(|e| e.detail.curl.clone())
                .unwrap_or_default();
            let can_curl = detail_ready && !curl.is_empty() && s.protocol != "tunnel";
            if widgets::button(
                ui,
                &st,
                "Copy as cURL",
                Kind::Secondary,
                Some(Icon::Terminal),
                can_curl,
            )
            .clicked()
            {
                ui.ctx().copy_text(curl);
                self.toast(ui.ctx(), "Copied as cURL", true);
            }
            if widgets::button(
                ui,
                &st,
                "Copy URL",
                Kind::Secondary,
                Some(Icon::Copy),
                detail_ready,
            )
            .clicked()
            {
                let url = self
                    .details
                    .get(&id)
                    .map(|e| e.detail.url.clone())
                    .unwrap_or_default();
                ui.ctx().copy_text(url);
                self.toast(ui.ctx(), "URL copied", true);
            }
        });
        head.add_space(6.0);
        let tab_idx = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
        let labels: Vec<&str> = Tab::ALL.iter().map(|t| t.label()).collect();
        let tab_w = head.available_width();
        if let Some(i) = widgets::segmented_w(&mut head, &st, self.id("tabs"), &labels, tab_idx, Some(tab_w)) {
            self.tab = Tab::ALL[i];
        }
        head.add_space(8.0);
        let y = head.min_rect().bottom();
        paint::hairline(
            ui.painter(),
            &st,
            pos2(rect.left(), y),
            pos2(rect.right(), y),
        );

        let body_rect = Rect::from_min_max(pos2(rect.left(), y + 1.0), rect.right_bottom());
        let mut bu = ui.new_child(UiBuilder::new().max_rect(body_rect.shrink2(vec2(pad, 0.0))));
        bu.set_clip_rect(body_rect.intersect(ui.clip_rect()));

        // Content materializes when the selection or tab changes: a short
        // fade up from partial opacity. Reduced motion skips it.
        let key = (id, self.tab as u8);
        let key_id = self.id("detail-key");
        let fade_id = self.id("detail-fade");
        let ctx = ui.ctx().clone();
        let last: Option<(u64, u8)> = ctx.data(|d| d.get_temp(key_id));
        if last != Some(key) {
            ctx.data_mut(|d| d.insert_temp(key_id, key));
            motion::with_spring(&ctx, fade_id, 1.0, |sp| {
                sp.value = 0.25;
                sp.velocity = 0.0;
                sp.target = 1.0;
            });
        }
        let fade = motion::animate(&ctx, fade_id, 1.0, motion::SMOOTH, st.reduced_motion);
        bu.set_opacity(fade.clamp(0.0, 1.0));

        if !self.details.contains_key(&id) {
            let c = body_rect.center();
            let mut sp = bu
                .new_child(UiBuilder::new().max_rect(Rect::from_center_size(c, vec2(120.0, 24.0))));
            widgets::spinner(&mut sp, &st, 18.0);
            return;
        }
        match self.tab {
            Tab::Overview => self.tab_overview(&mut bu, &st, id, &s),
            Tab::Request => self.tab_http(&mut bu, &st, id, false),
            Tab::Response => self.tab_http(&mut bu, &st, id, true),
            Tab::Timing => self.tab_timing(&mut bu, &st, id, &s),
        }
    }

    fn tab_overview(&mut self, ui: &mut Ui, st: &Style, id: u64, s: &pb::FlowSummary) {
        let Some(entry) = self.details.get(&id) else {
            return;
        };
        let d = &entry.detail;
        let url = d.url.clone();
        let mut open_settings = false;
        ScrollArea::vertical().id_salt("overview").auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(12.0);
            if s.flags & 1 != 0 {
                banner(
                    ui,
                    st,
                    st.p.orange,
                    Icon::Lock,
                    "Not inspectable",
                    "The client rejected the proxy certificate, so traffic to this host is tunneled untouched. \
                     Install the certificate, or the app may pin its own.",
                );
                ui.add_space(4.0);
                if widgets::button(ui, st, "Certificate settings", Kind::Secondary, Some(Icon::Shield), true).clicked() {
                    open_settings = true;
                }
                ui.add_space(10.0);
            } else if !s.error.is_empty() {
                banner(ui, st, st.p.red, Icon::Warning, "Request failed", &s.error);
                ui.add_space(10.0);
            }
            caption(ui, st, "Request");
            ui.add_space(4.0);
            card(ui, st, |ui| {
                kv(ui, st, "URL", &url, true);
                kv(ui, st, "Method", &s.method, false);
                kv(ui, st, "Protocol", &s.protocol, false);
                kv(ui, st, "Client", &s.app, false);
                kv(ui, st, "Sent", &fmt::size(s.req_size), false);
            });
            ui.add_space(14.0);
            caption(ui, st, "Response");
            ui.add_space(4.0);
            card(ui, st, |ui| {
                let status = if s.status == 0 {
                    String::new()
                } else {
                    format!("{} {}", s.status, fmt::status_text(s.status))
                };
                kv(ui, st, "Status", &status, false);
                kv(ui, st, "Type", &s.content_type, false);
                kv(ui, st, "Size", &fmt::size(s.res_size), false);
                kv(ui, st, "Duration", &fmt::duration(s.duration_ms), false);
            });
            ui.add_space(14.0);
            caption(ui, st, "Capture");
            ui.add_space(4.0);
            card(ui, st, |ui| {
                kv(ui, st, "Started", &fmt::date_clock(s.started_at_ms), false);
                kv(ui, st, "Flow", &format!("#{}", s.id), false);
            });
            ui.add_space(16.0);
        });
        if open_settings {
            self.section = Section::Settings;
        }
    }

    fn tab_http(&mut self, ui: &mut Ui, st: &Style, id: u64, response: bool) {
        let which = response as u8;
        let Some(entry) = self.details.get(&id) else {
            return;
        };
        let headers: Vec<pb::Header> = if response {
            entry.detail.response_headers.clone()
        } else {
            entry.detail.request_headers.clone()
        };
        let view = if response {
            entry.detail.response_body.clone()
        } else {
            entry.detail.request_body.clone()
        };
        let lines = if response {
            entry.res_lines.clone()
        } else {
            entry.req_lines.clone()
        };
        let tex = if response {
            entry.res_tex.clone()
        } else {
            entry.req_tex.clone()
        };
        let title = if response { "Response" } else { "Request" };
        let pending = response
            && self
                .table
                .row_of(id)
                .and_then(|r| self.table.get(r))
                .map(|s| {
                    s.state == pb::FlowState::Pending as i32
                        || s.state == pb::FlowState::Receiving as i32
                })
                .unwrap_or(false);

        let pane_h = ui.available_height();
        let mut toggle_headers = false;
        ScrollArea::vertical()
            .id_salt(("http", which))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(10.0);
                // Section header: folds the list so the body can have the room.
                let (hr, hresp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
                let chev = if self.headers_open {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                };
                let hot = hresp.hovered();
                super::icons::paint(
                    ui.painter(),
                    chev,
                    pos2(hr.left() + 8.0, hr.center().y),
                    11.0,
                    if hot { st.p.text } else { st.p.text3 },
                );
                paint::text(
                    ui.painter(),
                    ui,
                    pos2(hr.left() + 22.0, hr.center().y),
                    Align2::LEFT_CENTER,
                    &format!("{title} headers").to_uppercase(),
                    Role::Overline,
                    if hot { st.p.text2 } else { st.p.text3 },
                );
                paint::text(
                    ui.painter(),
                    ui,
                    pos2(hr.right() - 4.0, hr.center().y),
                    Align2::RIGHT_CENTER,
                    &headers.len().to_string(),
                    Role::Caption,
                    st.p.text3,
                );
                if hresp.clicked() {
                    toggle_headers = true;
                }
                hresp.on_hover_cursor(egui::CursorIcon::PointingHand);
                ui.add_space(2.0);
                if self.headers_open {
                    if headers.is_empty() {
                        let msg = if pending {
                            "Waiting for the response…"
                        } else {
                            "No headers"
                        };
                        ui.add(
                            egui::Label::new(job(msg, Role::Callout, st.p.text2)).selectable(false),
                        );
                    } else {
                        card(ui, st, |ui| {
                            for (i, h) in headers.iter().enumerate() {
                                if i > 0 {
                                    ui.add_space(4.0);
                                }
                                ui.horizontal_top(|ui| {
                                    ui.spacing_mut().item_spacing.x = 10.0;
                                    let w = (ui.available_width() * 0.34).clamp(90.0, 170.0);
                                    ui.allocate_ui(vec2(w, 18.0), |ui| {
                                        ui.set_width(w);
                                        ui.add(
                                            egui::Label::new(job(
                                                &h.name,
                                                Role::MonoSmall,
                                                st.p.syn.key,
                                            ))
                                            .wrap()
                                            .selectable(true),
                                        );
                                    });
                                    selectable(ui, &h.value, Role::MonoSmall, st.p.text);
                                });
                            }
                        });
                    }
                }
                ui.add_space(14.0);
                caption(ui, st, &format!("{title} body"));
                ui.add_space(6.0);
                match view {
                    Some(v) if v.kind != pb::BodyKind::Empty as i32 || !v.note.is_empty() => {
                        self.body_panel(ui, st, id, which, &v, lines, tex, pane_h);
                    }
                    _ => {
                        let msg = if pending {
                            "Waiting for the response…"
                        } else {
                            "No body"
                        };
                        ui.add(
                            egui::Label::new(job(msg, Role::Callout, st.p.text2)).selectable(false),
                        );
                    }
                }
                ui.add_space(18.0);
            });
        if toggle_headers {
            self.headers_open = !self.headers_open;
        }
    }

    /// The body card: a header bar (type, size, mode, copy) over a viewer that
    /// always gets real height, so a body is readable the moment its section
    /// scrolls into view. `pane_h` is the height of the detail pane.
    #[allow(clippy::too_many_arguments)]
    fn body_panel(
        &mut self,
        ui: &mut Ui,
        st: &Style,
        id: u64,
        which: u8,
        v: &pb::BodyView,
        lines: Option<Arc<Vec<String>>>,
        tex: Option<egui::TextureHandle>,
        pane_h: f32,
    ) {
        use super::viewer;
        const BAR_H: f32 = 40.0;
        let kind = pb::BodyKind::try_from(v.kind).unwrap_or(pb::BodyKind::Empty);
        let (label, chip) = match kind {
            pb::BodyKind::Json => ("JSON", st.p.syn.key),
            pb::BodyKind::Html => ("HTML", st.p.syn.tag),
            pb::BodyKind::Text => ("TEXT", st.p.teal),
            pb::BodyKind::Form => ("FORM", st.p.orange),
            pb::BodyKind::Image => ("IMAGE", st.p.purple),
            pb::BodyKind::Binary => ("BINARY", st.p.text2),
            pb::BodyKind::Empty => ("EMPTY", st.p.text2),
        };
        // Short form first; the wire detail joins when the bar has room.
        let short = if v.content_encoding.is_empty() {
            fmt::size(v.decoded_size)
        } else {
            format!("{} · {}", fmt::size(v.decoded_size), v.content_encoding)
        };
        let long = if v.content_encoding.is_empty() {
            short.clone()
        } else {
            format!("{short} · {} on the wire", fmt::size(v.wire_size))
        };

        let is_json = kind == pb::BodyKind::Json && !v.json.is_empty();
        let is_html = kind == pb::BodyKind::Html && !v.html.is_empty();
        let show_tree = is_json && !self.raw_view;
        let show_preview = is_html && !self.html_source;
        let show_source = kind == pb::BodyKind::Html && !show_preview && !v.source.is_empty();

        // Height: the content's own height, but never less than something
        // useful, and never more than the pane (the viewer scrolls inside).
        // With the headers folded the whole card fits on screen at once; with
        // them open it fills a screen of its own once scrolled into view.
        let above = if self.headers_open { 64.0 } else { 150.0 };
        let fill_h = (pane_h - above).clamp(300.0, 900.0);
        let width = ui.available_width();
        let folded = self.folds.entry((id, which)).or_default().clone();
        let visible: Vec<usize> = if show_tree {
            viewer::visible_json(&v.json, &folded)
        } else {
            Vec::new()
        };
        let rows_h = |n: usize| n as f32 * viewer::ROW_H + 16.0;
        let content_h = if show_tree {
            rows_h(visible.len())
        } else if show_preview {
            viewer::estimate_preview_height(&v.html, width)
        } else if show_source {
            rows_h(v.source.len())
        } else if kind == pb::BodyKind::Image {
            tex.as_ref()
                .map(|t| {
                    let s = t.size_vec2();
                    let w = (width - 32.0).min(s.x.max(40.0));
                    (w * s.y / s.x.max(1.0)).min(420.0) + 56.0
                })
                .unwrap_or(96.0)
        } else if kind == pb::BodyKind::Form && !v.form.is_empty() && !self.raw_view {
            v.form.len() as f32 * 30.0 + 20.0
        } else if matches!(kind, pb::BodyKind::Binary | pb::BodyKind::Empty) {
            84.0
        } else {
            rows_h(lines.as_ref().map(|l| l.len()).unwrap_or(1))
        };
        let min_h = if matches!(kind, pb::BodyKind::Binary | pb::BodyKind::Empty) {
            84.0
        } else {
            200.0
        };
        let h = content_h.clamp(min_h, fill_h.max(min_h));

        let (card_rect, _) = ui.allocate_exact_size(vec2(width, BAR_H + h), Sense::hover());
        let bar = Rect::from_min_size(card_rect.min, vec2(width, BAR_H));
        let view = Rect::from_min_max(pos2(card_rect.left(), bar.bottom()), card_rect.max);
        {
            let p = ui.painter();
            if !st.p.is_dark() {
                paint::shadow(p, st, card_rect, radius::PANEL, 0);
            }
            p.rect_filled(card_rect, cr(radius::PANEL), st.p.code);
            p.rect_filled(
                bar,
                egui::CornerRadius {
                    nw: radius::PANEL,
                    ne: radius::PANEL,
                    sw: 0,
                    se: 0,
                },
                st.p.toolbar,
            );
            paint::hairline(p, st, bar.left_bottom(), bar.right_bottom());
        }

        // Header bar. The right group lays out first so the left text knows
        // how much room it has.
        let inner = bar.shrink2(vec2(10.0, 0.0));
        let mut right = ui.new_child(
            UiBuilder::new()
                .max_rect(inner)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        right.spacing_mut().item_spacing.x = 6.0;
        let mut copy = false;
        if !v.text.is_empty()
            && widgets::icon_button(&mut right, st, Icon::Copy, "Copy body", false, true).clicked()
        {
            copy = true;
        }
        if is_json {
            let sel = self.raw_view as usize;
            if let Some(i) =
                widgets::segmented(&mut right, st, self.id("body-mode"), &["Tree", "Raw"], sel)
            {
                self.raw_view = i == 1;
            }
        } else if kind == pb::BodyKind::Html && is_html {
            let sel = self.html_source as usize;
            if let Some(i) = widgets::segmented(
                &mut right,
                st,
                self.id("html-mode"),
                &["Preview", "Source"],
                sel,
            ) {
                self.html_source = i == 1;
            }
        }
        let left_limit = right.min_rect().left() - 8.0;
        {
            let p = ui.painter();
            let g = paint::galley(ui, label, Role::Caption, chip);
            let chip_r = Rect::from_min_size(
                pos2(bar.left() + 12.0, bar.center().y - 10.0),
                vec2(g.size().x + 14.0, 20.0),
            );
            p.rect_filled(chip_r, cr(6), st.p.tint(chip, 0.16));
            p.galley(
                pos2(chip_r.left() + 7.0, chip_r.center().y - g.size().y / 2.0),
                g,
                chip,
            );
            let room = left_limit - chip_r.right() - 10.0;
            let meta = if paint::galley(ui, &long, Role::Callout, st.p.text2).size().x <= room {
                &long
            } else {
                &short
            };
            paint::text_fit(
                p,
                ui,
                pos2(chip_r.right() + 10.0, bar.center().y),
                room,
                meta,
                Role::Callout,
                st.p.text2,
            );
        }
        if copy {
            ui.ctx().copy_text(v.text.clone());
            self.toast(ui.ctx(), "Body copied", true);
        }

        // Viewer.
        let ctx = ui.ctx().clone();
        let mode = (self.raw_view as u64) | ((self.html_source as u64) << 1) | ((which as u64) << 2);
        let fade = viewer::materialize(&ctx, st, self.id("body-fade"), id.wrapping_mul(31) ^ mode);
        let inner_view = view.shrink2(vec2(0.0, 6.0));
        let salt = (id, which, mode);
        if show_tree {
            let rows = viewer::JsonRows {
                lines: &v.json,
                visible: &visible,
                folded: &folded,
            };
            if let Some(l) = viewer::json_view(ui, st, ("json", salt), inner_view, &rows, fade) {
                let set = self.folds.entry((id, which)).or_default();
                if !set.remove(&l) {
                    set.insert(l);
                }
            }
        } else if show_preview {
            let mut sub = ui.new_child(UiBuilder::new().max_rect(view));
            sub.set_clip_rect(view.intersect(ui.clip_rect()));
            // The page sits on the content color, like paper under the code chrome.
            sub.painter().rect_filled(
                view,
                egui::CornerRadius {
                    nw: 0,
                    ne: 0,
                    sw: radius::PANEL,
                    se: radius::PANEL,
                },
                st.p.content,
            );
            if let Some(href) = viewer::html_preview(
                &mut sub,
                st,
                ("html", salt),
                view.shrink2(vec2(0.0, 2.0)),
                &v.html_title,
                &v.html,
                fade,
            ) {
                ui.ctx().copy_text(href);
                self.toast(ui.ctx(), "Link copied", true);
            }
        } else if show_source {
            viewer::source_view(ui, st, ("src", salt), inner_view, &v.source, fade);
        } else {
            let mut sub = ui.new_child(UiBuilder::new().max_rect(view.shrink(14.0)));
            sub.set_opacity(fade);
            match kind {
                pb::BodyKind::Image => {
                    if let Some(t) = tex {
                        let size = t.size_vec2();
                        let w = sub.available_width().min(size.x.max(40.0));
                        let ih = w * size.y / size.x.max(1.0);
                        sub.add(
                            egui::Image::new(&t)
                                .fit_to_exact_size(vec2(w, ih.min(420.0)))
                                .corner_radius(cr(8)),
                        );
                        sub.add_space(6.0);
                        sub.add(
                            egui::Label::new(job(
                                &format!("{} × {} px", size.x as u32, size.y as u32),
                                Role::Caption,
                                st.p.text3,
                            ))
                            .selectable(false),
                        );
                    } else {
                        sub.add(
                            egui::Label::new(job(
                                "Preview not available",
                                Role::Callout,
                                st.p.text2,
                            ))
                            .selectable(false),
                        );
                    }
                }
                pb::BodyKind::Form if !v.form.is_empty() && !self.raw_view => {
                    for f in &v.form {
                        sub.horizontal_top(|ui| {
                            let w = (ui.available_width() * 0.34).clamp(80.0, 160.0);
                            ui.allocate_ui(vec2(w, 22.0), |ui| {
                                ui.set_width(w);
                                ui.add(
                                    egui::Label::new(job(&f.name, Role::Mono, st.p.syn.key))
                                        .wrap()
                                        .selectable(true),
                                );
                            });
                            selectable(ui, &f.value, Role::Mono, st.p.syn.string);
                        });
                    }
                }
                pb::BodyKind::Binary | pb::BodyKind::Empty => {
                    let msg = if v.note.is_empty() {
                        "Binary data"
                    } else {
                        &v.note
                    };
                    sub.add(egui::Label::new(job(msg, Role::Callout, st.p.text2)).selectable(false));
                }
                _ => match lines {
                    Some(lines) => viewer::text_view(ui, st, ("text", salt), inner_view, &lines, fade),
                    None => {
                        sub.add(egui::Label::new(job("Empty", Role::Callout, st.p.text2)).selectable(false));
                    }
                },
            }
        }

        // Card outline last, so it sits over the viewer edges.
        ui.painter().rect_stroke(
            card_rect,
            cr(radius::PANEL),
            Stroke::new(1.0, st.p.separator),
            egui::StrokeKind::Inside,
        );
        if !v.note.is_empty() && !matches!(kind, pb::BodyKind::Binary | pb::BodyKind::Empty) {
            ui.add_space(6.0);
            ui.add(
                egui::Label::new(job(&v.note, Role::Callout, st.p.text3))
                    .wrap()
                    .selectable(false),
            );
        }
    }

    fn tab_timing(&mut self, ui: &mut Ui, st: &Style, id: u64, s: &pb::FlowSummary) {
        let Some(entry) = self.details.get(&id) else {
            return;
        };
        let t = entry.detail.timings.clone().unwrap_or_default();
        let phases: [(&str, f64, Color32, &str); 6] = [
            ("DNS", t.dns_ms, st.p.teal, "Resolving the host name"),
            (
                "Connect",
                t.connect_ms,
                st.p.orange,
                "Opening the TCP connection",
            ),
            ("TLS", t.tls_ms, st.p.purple, "Handshake with the server"),
            ("Send", t.send_ms, st.p.accent, "Uploading the request"),
            ("Wait", t.wait_ms, st.p.green, "Time to first byte"),
            (
                "Receive",
                t.receive_ms,
                st.p.text2,
                "Downloading the response",
            ),
        ];
        let sum: f64 = phases.iter().map(|p| p.1).sum();
        let total = t.total_ms.max(sum).max(0.001);
        let pending = s.state == pb::FlowState::Pending as i32;
        ScrollArea::vertical()
            .id_salt("timing")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(job(
                            &fmt::ms_precise(t.total_ms),
                            Role::LargeTitle,
                            st.p.text,
                        ))
                        .selectable(false),
                    );
                    ui.add_space(6.0);
                    let note = if pending {
                        "in progress"
                    } else if t.reused_connection {
                        "connection reused"
                    } else {
                        "new connection"
                    };
                    ui.add(
                        egui::Label::new(job(note, Role::Callout, st.p.text2)).selectable(false),
                    );
                });
                ui.add_space(10.0);

                // Summary bar.
                let (bar, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 12.0), Sense::hover());
                ui.painter().rect_filled(bar, cr(6), st.p.fill);
                let mut x = bar.left();
                for (_, v, c, _) in &phases {
                    let w = (*v / total) as f32 * bar.width();
                    if w > 0.5 {
                        let r = Rect::from_min_size(pos2(x, bar.top()), vec2(w, bar.height()));
                        ui.painter()
                            .rect_filled(r.shrink2(vec2(0.5, 0.0)), cr(3), *c);
                        x += w;
                    }
                }
                ui.add_space(16.0);
                caption(ui, st, "Waterfall");
                ui.add_space(4.0);
                card(ui, st, |ui| {
                    let mut offset = 0.0f64;
                    for (name, v, c, hint) in &phases {
                        let (row, resp) = ui
                            .allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
                        let label_w = 70.0;
                        let value_w = 76.0;
                        paint::text(
                            ui.painter(),
                            ui,
                            pos2(row.left(), row.center().y),
                            Align2::LEFT_CENTER,
                            name,
                            Role::Callout,
                            st.p.text2,
                        );
                        let track = Rect::from_min_max(
                            pos2(row.left() + label_w, row.center().y - 5.0),
                            pos2(row.right() - value_w, row.center().y + 5.0),
                        );
                        ui.painter()
                            .rect_filled(track, cr(5), paint::with_alpha(st.p.fill, 0.6));
                        if *v > 0.0 {
                            let x0 = track.left() + (offset / total) as f32 * track.width();
                            let w = ((*v / total) as f32 * track.width()).max(3.0);
                            ui.painter().rect_filled(
                                Rect::from_min_size(pos2(x0, track.top()), vec2(w, track.height())),
                                cr(5),
                                *c,
                            );
                            offset += *v;
                        }
                        let val = if *v > 0.0 {
                            fmt::ms_precise(*v)
                        } else {
                            "—".into()
                        };
                        paint::text(
                            ui.painter(),
                            ui,
                            pos2(row.right(), row.center().y),
                            Align2::RIGHT_CENTER,
                            &val,
                            Role::MonoSmall,
                            if *v > 0.0 { st.p.text } else { st.p.text3 },
                        );
                        resp.on_hover_text(*hint);
                    }
                });
                ui.add_space(10.0);
                if t.reused_connection {
                    ui.add(egui::Label::new(job(
                    "DNS, connect and TLS are zero because this request reused an open connection.",
                    Role::Callout,
                    st.p.text3,
                )).wrap().selectable(false));
                }
                ui.add_space(16.0);
            });
    }
}
