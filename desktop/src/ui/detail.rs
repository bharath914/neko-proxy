use super::icons::Icon;
use super::widgets::{self, Kind};
use crate::app::{App, Section, Tab};
use crate::backend::ConnState;
use crate::fmt;
use crate::motion;
use crate::paint::{self, cr};
use crate::theme::{radius, Role, Style};
use egui::text::{LayoutJob, TextFormat};
use egui::{
    pos2, vec2, Align2, Color32, Frame, Margin, Rect, ScrollArea, Sense, Stroke, Ui, UiBuilder,
};
use proto::pb;
use std::sync::Arc;

const TRAFFIC_MIN: f32 = 140.0;
const PANE_MIN_H: f32 = 180.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Request,
    Response,
}

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

/// A body that is not in the core: still on the device, skipped, or gone.
fn lazy_notice(ui: &mut Ui, st: &Style, state: i32, size: u64, loading: bool) {
    card(ui, st, |ui| {
        ui.horizontal(|ui| {
            match state {
                1 => {
                    widgets::spinner(ui, st, 14.0);
                    let msg = if loading {
                        format!("Loading {} from the device…", if size > 0 { fmt::size(size) } else { "the body".into() })
                    } else {
                        "Asking the device for the body…".to_string()
                    };
                    ui.add(egui::Label::new(job(&msg, Role::Callout, st.p.text2)).selectable(false));
                }
                2 => {
                    ui.add(egui::Label::new(job(
                        &format!("Not captured{}. The app left it out on purpose (a media type, or Strict redaction).", if size > 0 { format!(" ({})", fmt::size(size)) } else { String::new() }),
                        Role::Callout,
                        st.p.text2,
                    )).wrap().selectable(false));
                }
                _ => {
                    ui.add(egui::Label::new(job(
                        "Body no longer available. The device dropped it (its buffer filled, or the app restarted) before it was opened.",
                        Role::Callout,
                        st.p.text2,
                    )).wrap().selectable(false));
                }
            }
        });
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
            ui.painter().rect_filled(r, 0.0, st.p.tint(st.p.red, 0.14));
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

        // Device and app chips, when SDK devices are paired.
        body.min.y += self.device_chips(ui, body);

        // Traffic on top, request and response side by side below. Both
        // dividers are plain drags; double-click resets them.
        let ctx = ui.ctx().clone();
        let auto_top = (body.height() * 0.42).clamp(TRAFFIC_MIN, body.height() - PANE_MIN_H);
        if self.split_top <= 0.0 {
            self.split_top = auto_top;
        }
        let max_top = (body.height() - PANE_MIN_H).max(TRAFFIC_MIN);
        self.split_top = self.split_top.clamp(TRAFFIC_MIN, max_top);
        let split_y = body.top() + self.split_top;
        let hit = Rect::from_min_max(
            pos2(body.left(), split_y - 4.0),
            pos2(body.right(), split_y + 4.0),
        );
        let resp = ui.interact(hit, self.id("divider-h"), Sense::click_and_drag());
        if resp.dragged() {
            self.split_top = (self.split_top + resp.drag_delta().y).clamp(TRAFFIC_MIN, max_top);
        }
        if resp.double_clicked() {
            self.split_top = 0.0;
        }
        let top_rect = Rect::from_min_max(body.min, pos2(body.right(), split_y));
        let bottom = Rect::from_min_max(pos2(body.left(), split_y), body.max);

        self.split_mid = self.split_mid.clamp(0.25, 0.75);
        let mid_x = bottom.left() + bottom.width() * self.split_mid;
        let vhit = Rect::from_min_max(
            pos2(mid_x - 4.0, bottom.top()),
            pos2(mid_x + 4.0, bottom.bottom()),
        );
        let vresp = ui.interact(vhit, self.id("divider-v"), Sense::click_and_drag());
        if vresp.dragged() {
            self.split_mid =
                ((mid_x + vresp.drag_delta().x - bottom.left()) / bottom.width()).clamp(0.25, 0.75);
        }
        if vresp.double_clicked() {
            self.split_mid = 0.5;
        }
        let mid_x = bottom.left() + bottom.width() * self.split_mid;
        let req_rect = Rect::from_min_max(bottom.min, pos2(mid_x, bottom.bottom()));
        let res_rect = Rect::from_min_max(pos2(mid_x, bottom.top()), bottom.max);

        // A slim strip switches the top section between the table and the sequence.
        let strip = Rect::from_min_size(top_rect.min, vec2(top_rect.width(), 38.0));
        let top_rect = Rect::from_min_max(pos2(top_rect.left(), strip.bottom()), top_rect.max);
        ui.painter().rect_filled(strip, 0.0, st.p.content);
        let mut sc = ui.new_child(
            UiBuilder::new()
                .max_rect(strip.shrink2(vec2(14.0, 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        if let Some(i) = widgets::segmented(
            &mut sc,
            &st,
            self.id("traffic-view"),
            &["Traffic", "Sequence"],
            self.traffic_seq as usize,
        ) {
            self.traffic_seq = i == 1;
        }
        if self.traffic_seq {
            self.sequence(ui, top_rect);
        } else {
            self.table(ui, top_rect);
        }
        self.detail(ui, req_rect, Pane::Request);
        self.detail(ui, res_rect, Pane::Response);

        let p = ui.painter();
        paint::hairline(
            p,
            &st,
            pos2(body.left(), split_y),
            pos2(body.right(), split_y),
        );
        paint::hairline(
            p,
            &st,
            pos2(mid_x, bottom.top()),
            pos2(mid_x, bottom.bottom()),
        );
        for (r, rect, icon) in [
            (&resp, hit, egui::CursorIcon::ResizeVertical),
            (&vresp, vhit, egui::CursorIcon::ResizeHorizontal),
        ] {
            let a = motion::fade(
                &ctx,
                self.id(if rect == hit { "fade-h" } else { "fade-v" }),
                r.hovered() || r.dragged(),
            );
            if a > 0.0 {
                let (c, d) = if rect == hit {
                    (rect.center().y, true)
                } else {
                    (rect.center().x, false)
                };
                let seg = if d {
                    [pos2(rect.left(), c), pos2(rect.right(), c)]
                } else {
                    [pos2(c, rect.top()), pos2(c, rect.bottom())]
                };
                p.line_segment(
                    seg,
                    Stroke::new(2.0, paint::with_alpha(st.p.accent, a * 0.8)),
                );
            }
            if r.hovered() || r.dragged() {
                ctx.set_cursor_icon(icon);
            }
        }
    }

    pub fn detail(&mut self, ui: &mut Ui, rect: Rect, pane: Pane) {
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
            if pane == Pane::Request {
                widgets::badge(ui, &s.method, mc, st.p.tint(mc, 0.16));
            }
            let in_flight = s.state == pb::FlowState::Pending as i32
                || s.state == pb::FlowState::Receiving as i32;
            if pane == Pane::Request {
                // Method, protocol and source are the request's half of the story.
            } else if s.status != 0 {
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
            if pane == Pane::Response {
                let mut parts = Vec::new();
                if s.res_size > 0 {
                    parts.push(fmt::size(s.res_size));
                }
                if s.duration_ms > 0.0 && s.protocol != "tunnel" {
                    parts.push(fmt::duration(s.duration_ms));
                }
                if !parts.is_empty() {
                    ui.add(
                        egui::Label::new(job(&parts.join("  ·  "), Role::Callout, st.p.text2))
                            .selectable(false),
                    );
                }
                return;
            }
            if !s.protocol.is_empty() && s.protocol != "tunnel" {
                widgets::badge(ui, &s.protocol, st.p.text2, st.p.fill);
            }
            match pb::FlowSource::try_from(s.source).unwrap_or_default() {
                pb::FlowSource::Sdk => {
                    widgets::badge(ui, "SDK", st.p.purple, st.p.tint(st.p.purple, 0.16));
                }
                pb::FlowSource::Merged => {
                    widgets::badge(ui, "SDK + proxy", st.p.teal, st.p.tint(st.p.teal, 0.16));
                }
                pb::FlowSource::Proxy => {}
            }
        });
        if pane == Pane::Request {
            if !s.name.is_empty() {
                let who = if s.owner.is_empty() {
                    s.name.clone()
                } else {
                    format!("{}  ·  {}", s.name, s.owner)
                };
                head.add(egui::Label::new(job(&who, Role::Headline, st.p.text)).selectable(true));
            }
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
            head.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
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
                let dio = self
                    .details
                    .get(&id)
                    .map(|e| e.detail.dio.clone())
                    .unwrap_or_default();
                if widgets::button(
                    ui,
                    &st,
                    "Copy as Dio",
                    Kind::Secondary,
                    Some(Icon::Copy),
                    !dio.is_empty(),
                )
                .clicked()
                {
                    ui.ctx().copy_text(dio);
                    self.toast(ui.ctx(), "Copied as Dio", true);
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
                // Flows that are plain HTTP exchanges can be sent again or turned
                // into a rule that matches them.
                let exchange = s.protocol != "tunnel" && s.flags & 3 == 0;
                if widgets::button(
                    ui,
                    &st,
                    "Replay",
                    Kind::Secondary,
                    None,
                    detail_ready && exchange,
                )
                .on_hover_text("Send this request again as a new flow")
                .clicked()
                {
                    self.replay(id);
                }
                if widgets::button(ui, &st, "Mock", Kind::Secondary, None, exchange)
                    .on_hover_text("New mock rule for this request")
                    .clicked()
                {
                    let mut f = crate::edit::RuleForm::for_flow(crate::edit::RuleKind::Mock, &s);
                    f.status = s.status.max(200).to_string();
                    self.rule_form = Some(f);
                    self.section = Section::Rules;
                }
                if widgets::button(ui, &st, "Break", Kind::Secondary, None, exchange)
                    .on_hover_text("New breakpoint for requests like this")
                    .clicked()
                {
                    self.rule_form = Some(crate::edit::RuleForm::for_flow(
                        crate::edit::RuleKind::Breakpoint,
                        &s,
                    ));
                    self.section = Section::Rules;
                }
            });
            head.add_space(6.0);
            // A merged flow has two views: the app's, and the wire's.
            let has_wire = self.details.get(&id).is_some_and(|e| e.wire.is_some());
            if has_wire {
                let sel = self.wire_view as usize;
                let w = head.available_width();
                if let Some(i) = widgets::segmented_w(
                    &mut head,
                    &st,
                    self.id("app-wire"),
                    &["App", "Wire"],
                    sel,
                    Some(w),
                ) {
                    self.wire_view = i == 1;
                }
                head.add_space(6.0);
            } else if self.wire_view {
                self.wire_view = false;
            }
        }
        let tabs: [Tab; 2] = match pane {
            Pane::Request => [Tab::Request, Tab::Overview],
            Pane::Response => [Tab::Response, Tab::Timing],
        };
        let cur = if pane == Pane::Request {
            self.tab
        } else {
            self.tab_res
        };
        let tab_idx = tabs.iter().position(|t| *t == cur).unwrap_or(0);
        let labels: Vec<&str> = tabs.iter().map(|t| t.label()).collect();
        let tab_w = head.available_width();
        if let Some(i) = widgets::segmented_w(
            &mut head,
            &st,
            self.id(if pane == Pane::Request {
                "tabs-req"
            } else {
                "tabs-res"
            }),
            &labels,
            tab_idx,
            Some(tab_w),
        ) {
            self.set_tab(tabs[i]);
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
        let key = (id, cur as u8);
        let salt = pane as u8;
        let key_id = self.id("detail-key").with(salt);
        let fade_id = self.id("detail-fade").with(salt);
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
        match cur {
            Tab::Overview => self.tab_overview(&mut bu, &st, id, &s),
            Tab::Request => self.tab_http(&mut bu, &st, id, false),
            Tab::Response => self.tab_http(&mut bu, &st, id, true),
            Tab::Timing => self.tab_timing(&mut bu, &st, id, &s),
        }
    }

    fn tab_overview(&mut self, ui: &mut Ui, st: &Style, id: u64, s: &pb::FlowSummary) {
        let Some(entry) = self.view_entry(id) else {
            return;
        };
        let d = &entry.detail;
        let url = d.url.clone();
        let issues = d.issues.clone();
        // The SDK's context belongs to the App half; the Wire half is the proxy's.
        let wire_half = self.wire_view && self.details.get(&id).is_some_and(|e| e.wire.is_some());
        let sdk = if wire_half {
            None
        } else {
            self.details.get(&id).and_then(|e| e.detail.sdk.clone())
        };
        let mut open_settings = false;
        let mut open_device: Option<(String, String)> = None;
        ScrollArea::vertical().id_salt("overview").auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(12.0);
            if !issues.is_empty() {
                ui.strong("Detected issues");
                for issue in &issues { ui.label(issue); }
                ui.label("Heuristics; inspect locally before drawing conclusions.");
                ui.separator();
            }
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
            if let Some(c) = &sdk {
                caption(ui, st, "From the app");
                ui.add_space(4.0);
                card(ui, st, |ui| {
                    kv(ui, st, "Name", &c.name, false);
                    kv(ui, st, "Owner", &c.owner, false);
                    kv(ui, st, "Tag", &c.tag, false);
                    kv(ui, st, "Screen", &c.screen, false);
                    kv(ui, st, "Route", &c.route_template, true);
                    kv(ui, st, "Client", &c.client_stack, false);
                    let app = if c.app_name.is_empty() { c.app_id.clone() } else { format!("{} ({})", c.app_name, c.app_id) };
                    kv(ui, st, "App", &app, false);
                    let ver = if c.build.is_empty() { c.app_version.clone() } else { format!("{} (build {})", c.app_version, c.build) };
                    kv(ui, st, "Version", &ver, false);
                    kv(ui, st, "Device", &format!("{} · {} · {}", c.device_name, c.device_model, c.os_version), false);
                    kv(ui, st, "SDK", &format!("{} {}", c.sdk_name, c.sdk_version), false);
                    if c.from_cache {
                        kv(ui, st, "Cache", "Answered from the app's HTTP cache", false);
                    }
                });
                if c.request_redacted || c.response_redacted {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        widgets::badge(ui, "Redacted", st.p.orange, st.p.tint(st.p.orange, 0.16));
                        ui.add(egui::Label::new(job("Some values were hidden on the device.", Role::Callout, st.p.text2)).selectable(false));
                        if widgets::button(ui, st, "Redaction settings", Kind::Plain, None, true).clicked() {
                            open_device = Some((c.device_id.clone(), c.app_id.clone()));
                        }
                    });
                }
                ui.add_space(14.0);
            } else if wire_half {
                ui.add(egui::Label::new(job("This is what the proxy saw on the wire: real headers, TLS and timings.", Role::Callout, st.p.text2)).wrap().selectable(false));
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
        if let Some((d, a)) = open_device {
            self.open_device_settings(&d, &a);
        }
    }

    fn tab_http(&mut self, ui: &mut Ui, st: &Style, id: u64, response: bool) {
        let wire_half = self.wire_view && self.details.get(&id).is_some_and(|e| e.wire.is_some());
        let which = response as u8 + if wire_half { 2 } else { 0 };
        let Some(entry) = self.view_entry(id) else {
            return;
        };
        let sdk = if wire_half {
            None
        } else {
            self.details.get(&id).and_then(|e| e.detail.sdk.clone())
        };
        let body_state = sdk.as_ref().map(|c| {
            if response {
                c.response_body_state
            } else {
                c.request_body_state
            }
        });
        let body_size = self
            .table
            .row_of(id)
            .and_then(|r| self.table.get(r))
            .map(|s| if response { s.res_size } else { s.req_size })
            .unwrap_or(0);
        let loading = self.fetching.contains(&(id, response));
        let mut need_fetch = false;
        let mut open_device: Option<(String, String)> = None;
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
                                    if h.value == "[redacted]" {
                                        let r = widgets::badge(ui, "Redacted", st.p.orange, st.p.tint(st.p.orange, 0.16))
                                            .on_hover_text("Hidden on the device before it was sent. Click to change the redaction setting.")
                                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                                        if r.interact(Sense::click()).clicked() {
                                            if let Some(c) = &sdk {
                                                open_device = Some((c.device_id.clone(), c.app_id.clone()));
                                            }
                                        }
                                    } else {
                                        selectable(ui, &h.value, Role::MonoSmall, st.p.text);
                                    }
                                });
                            }
                        });
                    }
                }
                ui.add_space(14.0);
                caption(ui, st, &format!("{title} body"));
                ui.add_space(6.0);
                match view {
                    _ if matches!(body_state, Some(1)) => {
                        // Kept on the device: ask for it, show that we are.
                        if !loading {
                            need_fetch = true;
                        }
                        lazy_notice(ui, st, 1, body_size, loading);
                    }
                    _ if matches!(body_state, Some(2) | Some(3)) => {
                        lazy_notice(ui, st, body_state.unwrap_or(2), body_size, false);
                    }
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
        if need_fetch && !self.wire_view {
            self.fetch_body(id, response);
        }
        if let Some((d, a)) = open_device {
            self.open_device_settings(&d, &a);
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

        let decoded = v.protocol.as_ref().filter(|p| !p.title.is_empty());
        let show_decoded = decoded.is_some() && !self.raw_view;
        let show_events = !v.events.is_empty() && !self.raw_view;
        let is_json = kind == pb::BodyKind::Json && !v.json.is_empty();
        let is_html = kind == pb::BodyKind::Html && !v.html.is_empty();
        let show_tree = is_json && !self.raw_view && !show_decoded;
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
        let content_h = if show_decoded || show_events {
            fill_h
        } else if show_tree {
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
        if decoded.is_some() || !v.events.is_empty() {
            let labels = [
                if decoded.is_some() {
                    "Decoded"
                } else {
                    "Events"
                },
                "Raw",
            ];
            if let Some(i) = widgets::segmented(
                &mut right,
                st,
                self.id("decoded-mode"),
                &labels,
                self.raw_view as usize,
            ) {
                self.raw_view = i == 1;
            }
        } else if is_json {
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
        let mode =
            (self.raw_view as u64) | ((self.html_source as u64) << 1) | ((which as u64) << 2);
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
        } else if show_decoded {
            let decoded = decoded.unwrap();
            let mut sub = ui.new_child(UiBuilder::new().max_rect(view.shrink(14.0)));
            sub.set_opacity(fade);
            egui::ScrollArea::vertical()
                .id_salt(("protocol", salt))
                .show(&mut sub, |ui| {
                    ui.strong(&decoded.title);
                    if !decoded.note.is_empty() {
                        ui.label(&decoded.note);
                    }
                    for (i, field) in decoded.fields.iter().enumerate() {
                        ui.push_id(i, |ui| {
                            ui.indent(field.depth, |ui| {
                                ui.label(&field.label);
                                ui.add(
                                    egui::Label::new(egui::RichText::new(&field.value).monospace())
                                        .selectable(true),
                                );
                            });
                        });
                    }
                });
        } else if show_events {
            let mut sub = ui.new_child(UiBuilder::new().max_rect(view.shrink(14.0)));
            sub.set_opacity(fade);
            egui::ScrollArea::vertical()
                .id_salt(("sse", salt))
                .show_rows(&mut sub, 90.0, v.events.len(), |ui, rows| {
                    for index in rows {
                        let event = &v.events[index];
                        let (rect, _) = ui
                            .allocate_exact_size(vec2(ui.available_width(), 90.0), Sense::hover());
                        let mut row = ui.new_child(UiBuilder::new().max_rect(rect));
                        row.set_clip_rect(rect.intersect(ui.clip_rect()));
                        row.horizontal(|ui| {
                            ui.label(format!(
                                "#{} · {} · id {}",
                                index + 1,
                                event.event,
                                event.id
                            ));
                            if ui.small_button("Copy event").clicked() {
                                ui.ctx().copy_text(event.data.clone());
                            }
                        });
                        if let Some(ms) = event.retry_ms {
                            row.small(format!("Retry: {ms} ms"));
                        }
                        row.add(
                            egui::Label::new(egui::RichText::new(&event.data).monospace())
                                .truncate()
                                .selectable(true),
                        );
                        row.separator();
                    }
                });
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
                    sub.add(
                        egui::Label::new(job(msg, Role::Callout, st.p.text2)).selectable(false),
                    );
                }
                _ => match lines {
                    Some(lines) => {
                        viewer::text_view(ui, st, ("text", salt), inner_view, &lines, fade)
                    }
                    None => {
                        sub.add(
                            egui::Label::new(job("Empty", Role::Callout, st.p.text2))
                                .selectable(false),
                        );
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
        // For a merged flow, show both, labelled: the client stack's own
        // timings and the proxy's. Unknown client phases (-1) show as dashes.
        let main = self.details.get(&id).map(|e| e.detail.clone());
        let (Some(main), Some(wire)) = (
            main.clone(),
            self.details.get(&id).and_then(|e| e.wire.as_ref()),
        ) else {
            let Some(entry) = self.view_entry(id) else {
                return;
            };
            let t = entry.detail.timings.clone().unwrap_or_default();
            let sdk_t = if self.wire_view {
                None
            } else {
                entry.detail.sdk.as_ref().and_then(|c| c.timings.clone())
            };
            let t = sdk_t.unwrap_or(t);
            let pending = s.state == pb::FlowState::Pending as i32;
            ScrollArea::vertical()
                .id_salt("timing")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    timing_block(ui, st, None, &t, pending);
                });
            return;
        };
        let app_t = main
            .sdk
            .as_ref()
            .and_then(|c| c.timings.clone())
            .or(main.timings.clone())
            .unwrap_or_default();
        let wire_t = wire.detail.timings.clone().unwrap_or_default();
        let pending = s.state == pb::FlowState::Pending as i32;
        ScrollArea::vertical()
            .id_salt("timing")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                timing_block(ui, st, Some("As the app saw it"), &app_t, pending);
                ui.add_space(8.0);
                timing_block(ui, st, Some("On the wire (proxy)"), &wire_t, pending);
            });
    }
}

/// One waterfall. Phases the source did not measure (negative) show as dashes.
fn timing_block(ui: &mut Ui, st: &Style, heading: Option<&str>, t: &pb::Timings, pending: bool) {
    {
        let t = &pb::Timings {
            dns_ms: t.dns_ms.max(0.0),
            connect_ms: t.connect_ms.max(0.0),
            tls_ms: t.tls_ms.max(0.0),
            send_ms: t.send_ms.max(0.0),
            wait_ms: t.wait_ms.max(0.0),
            receive_ms: t.receive_ms.max(0.0),
            ..t.clone()
        };
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
        {
            {
                ui.add_space(14.0);
                if let Some(h) = heading {
                    caption(ui, st, h);
                    ui.add_space(4.0);
                }
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
            }
        }
    }
}
