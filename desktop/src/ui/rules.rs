//! Mock, map remote, rewrite, throttle and breakpoint rules, plus Map Local.
//! The core validates; this screen only collects fields and lists what exists.

use super::icons::Icon;
use super::widgets::{self, Kind};
use crate::app::App;
use crate::edit::{describe_action, describe_match, RuleForm, RuleKind};
use crate::paint::{self, cr};
use crate::theme::{radius, Role, Style};
use egui::{
    pos2, vec2, Align, Color32, Frame, Layout, Margin, Rect, RichText, ScrollArea, Stroke, Ui,
    UiBuilder,
};
use proto::pb;

fn text(ui: &mut Ui, s: &str, role: Role, color: Color32) {
    ui.add(egui::Label::new(RichText::new(s).font(role.font()).color(color)).selectable(false));
}

fn caption(ui: &mut Ui, st: &Style, s: &str) {
    ui.add_space(8.0);
    text(ui, s, Role::Caption, st.p.text2);
    ui.add_space(2.0);
}

fn single(ui: &mut Ui, value: &mut String, width: f32, hint: &str) {
    ui.add(
        egui::TextEdit::singleline(value)
            .font(Role::Mono.font())
            .hint_text(hint)
            .desired_width(width),
    );
}

fn multi(ui: &mut Ui, value: &mut String, rows: usize, hint: &str) {
    ui.add(
        egui::TextEdit::multiline(value)
            .font(Role::MonoSmall.font())
            .hint_text(hint)
            .desired_rows(rows)
            .desired_width(f32::INFINITY),
    );
}

fn kind_color(st: &Style, k: Option<RuleKind>) -> Color32 {
    match k {
        Some(RuleKind::Mock) => st.p.purple,
        Some(RuleKind::MapRemote) => st.p.teal,
        Some(RuleKind::Rewrite) => st.p.indigo,
        Some(RuleKind::Throttle) => st.p.orange,
        Some(RuleKind::Breakpoint) => st.p.red,
        None => st.p.green,
    }
}

impl App {
    pub fn rules_screen(&mut self, ui: &mut Ui) {
        let st = self.st.clone();
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, st.p.grouped);

        let col_w = (full.width() - 64.0).clamp(320.0, 780.0);
        let left = full.center().x - col_w / 2.0;
        let rect = Rect::from_min_max(pos2(left, full.top()), pos2(left + col_w, full.bottom()));
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
        child.set_clip_rect(full.intersect(ui.clip_rect()));

        ScrollArea::vertical()
            .id_salt("rules")
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                ui.set_width(col_w);
                ui.add_space(26.0);
                ui.horizontal(|ui| {
                    text(ui, "Rules", Role::LargeTitle, st.p.text);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.rule_form.is_none()
                            && widgets::button(
                                ui,
                                &st,
                                "New rule",
                                Kind::Primary,
                                Some(Icon::Plus),
                                true,
                            )
                            .clicked()
                        {
                            self.rule_form = Some(RuleForm::new(RuleKind::Mock));
                        }
                    });
                });
                ui.add_space(4.0);
                text(
                    ui,
                    "Rules change traffic as it passes through. They apply to new requests, in the order listed.",
                    Role::Callout,
                    st.p.text2,
                );
                ui.add_space(14.0);

                if self.rule_form.is_some() {
                    self.rule_editor(ui, &st);
                    ui.add_space(14.0);
                }
                self.rule_list(ui, &st);
                ui.add_space(40.0);
            });
    }

    fn rule_editor(&mut self, ui: &mut Ui, st: &Style) {
        let Some(mut form) = self.rule_form.take() else {
            return;
        };
        let mut action: Option<bool> = None; // Some(true) save, Some(false) cancel
        let mut error: Option<String> = None;
        Frame::new()
            .fill(st.p.card)
            .stroke(Stroke::new(1.0, st.p.accent))
            .corner_radius(cr(radius::CARD))
            .inner_margin(Margin::symmetric(18, 14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    text(
                        ui,
                        if form.id.is_empty() { "New rule" } else { "Edit rule" },
                        Role::Headline,
                        st.p.text,
                    );
                });
                ui.add_space(8.0);
                if form.id.is_empty() {
                    let labels: Vec<&str> = RuleKind::ALL.iter().map(|k| k.label()).collect();
                    let sel = RuleKind::ALL.iter().position(|k| *k == form.kind).unwrap_or(0);
                    if let Some(i) =
                        widgets::segmented(ui, st, self.id("rule-kind"), &labels, sel)
                    {
                        form.kind = RuleKind::ALL[i];
                    }
                }
                ui.add_space(4.0);
                text(ui, form.kind.hint(), Role::Callout, st.p.text2);

                caption(ui, st, "Name (optional)");
                single(ui, &mut form.name, f32::INFINITY, "Checkout is down");

                caption(ui, st, "Applies to requests where");
                ui.horizontal(|ui| {
                    single(ui, &mut form.method, 80.0, "GET");
                    single(ui, &mut form.host, 240.0, "host: api.example.com or *");
                    single(ui, &mut form.port, 70.0, "port");
                });
                ui.horizontal(|ui| {
                    single(ui, &mut form.path, 280.0, "path: /v1/* (empty = any)");
                    single(ui, &mut form.query, 160.0, "query (exact)");
                    single(ui, &mut form.scheme, 70.0, "https");
                });

                match form.kind {
                    RuleKind::Mock => {
                        caption(ui, st, "Respond with");
                        ui.horizontal(|ui| {
                            single(ui, &mut form.status, 70.0, "200");
                            single(ui, &mut form.delay_ms, 110.0, "delay ms");
                        });
                        caption(ui, st, "Headers, one per line");
                        multi(ui, &mut form.headers, 3, "content-type: application/json");
                        caption(ui, st, "Body");
                        multi(ui, &mut form.body, 6, "{\"ok\": true}");
                    }
                    RuleKind::MapRemote => {
                        caption(ui, st, "Send to (leave empty to keep)");
                        ui.horizontal(|ui| {
                            single(ui, &mut form.to_scheme, 80.0, "https");
                            single(ui, &mut form.to_host, 260.0, "staging.example.com");
                            single(ui, &mut form.to_port, 80.0, "port");
                        });
                    }
                    RuleKind::Rewrite => {
                        caption(ui, st, "Request headers: set, add or remove, one per line");
                        multi(ui, &mut form.request_edits, 3, "set X-Env: staging\nremove Cookie");
                        caption(ui, st, "Response headers");
                        multi(ui, &mut form.response_edits, 3, "set Cache-Control: no-store");
                    }
                    RuleKind::Throttle => {
                        caption(ui, st, "Delay each request by (ms)");
                        single(ui, &mut form.delay_ms, 110.0, "800");
                    }
                    RuleKind::Breakpoint => {
                        caption(ui, st, "Hold the");
                        ui.horizontal(|ui| {
                            if let Some(i) = widgets::segmented(
                                ui,
                                st,
                                self.id("rule-phase"),
                                &["Request and response", "Request", "Response"],
                                form.phase.min(2),
                            ) {
                                form.phase = i;
                            }
                            single(ui, &mut form.timeout_s, 150.0, "give up after s (60)");
                        });
                    }
                }

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if widgets::button(ui, st, "Save", Kind::Primary, Some(Icon::Check), true)
                        .clicked()
                    {
                        action = Some(true);
                    }
                    if widgets::button(ui, st, "Cancel", Kind::Plain, None, true).clicked() {
                        action = Some(false);
                    }
                });
            });

        match action {
            Some(true) => match form.to_rule() {
                Ok(rule) => {
                    self.put_rule(rule);
                    return;
                }
                Err(e) => error = Some(e),
            },
            Some(false) => return,
            None => {}
        }
        if let Some(e) = error {
            let ctx = ui.ctx().clone();
            self.toast(&ctx, e, false);
        }
        self.rule_form = Some(form);
    }

    fn rule_list(&mut self, ui: &mut Ui, st: &Style) {
        let v2 = self.rules.rules.clone();
        let local = self.rules.map_local.clone();
        if v2.is_empty() && local.is_empty() && self.rule_form.is_none() {
            Frame::new()
                .fill(st.p.card)
                .stroke(Stroke::new(1.0, st.p.separator))
                .corner_radius(cr(radius::CARD))
                .inner_margin(Margin::symmetric(18, 22))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    text(ui, "No rules yet", Role::Headline, st.p.text);
                    ui.add_space(4.0);
                    text(
                        ui,
                        "Mock a response, point a host somewhere else, add latency, rewrite headers, or hold a request at a breakpoint. You can also ask a connected AI client to do it (Settings > AI).",
                        Role::Callout,
                        st.p.text2,
                    );
                });
            return;
        }

        enum Act {
            Toggle(String, bool),
            Edit(pb::Rule),
            Remove(String),
        }
        let mut act: Option<Act> = None;
        let card = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
            Frame::new()
                .fill(st.p.card)
                .stroke(Stroke::new(1.0, st.p.separator))
                .corner_radius(cr(radius::CARD))
                .inner_margin(Margin::symmetric(16, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    add(ui);
                });
        };
        let mut first = true;
        card(ui, &mut |ui| {
            for r in &v2 {
                if !first {
                    ui.add_space(6.0);
                    let (l, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 1.0), egui::Sense::hover());
                    ui.painter().rect_filled(l, 0.0, st.p.separator);
                    ui.add_space(6.0);
                }
                first = false;
                let kind = RuleKind::of(r);
                ui.horizontal(|ui| {
                    let mut on = r.enabled;
                    if widgets::toggle(ui, st, &mut on).changed() {
                        act = Some(Act::Toggle(r.id.clone(), on));
                    }
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            let col = kind_color(st, kind);
                            widgets::badge(
                                ui,
                                kind.map(|k| k.label()).unwrap_or("Rule"),
                                col,
                                paint::with_alpha(col, 0.14),
                            );
                            let title = if r.name.is_empty() {
                                r.matcher.as_ref().map(describe_match).unwrap_or_default()
                            } else {
                                r.name.clone()
                            };
                            text(
                                ui,
                                &title,
                                Role::BodyMedium,
                                if r.enabled { st.p.text } else { st.p.text3 },
                            );
                        });
                        let detail = format!(
                            "{}{}",
                            describe_action(r),
                            if r.name.is_empty() {
                                String::new()
                            } else {
                                format!("  ·  {}", r.matcher.as_ref().map(describe_match).unwrap_or_default())
                            }
                        );
                        text(ui, &detail, Role::Callout, st.p.text2);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::icon_button(ui, st, Icon::Trash, "Delete rule", false, true)
                            .clicked()
                        {
                            act = Some(Act::Remove(r.id.clone()));
                        }
                        if widgets::button(ui, st, "Edit", Kind::Plain, None, true).clicked() {
                            act = Some(Act::Edit(r.clone()));
                        }
                    });
                });
            }
            for m in &local {
                if !first {
                    ui.add_space(6.0);
                    let (l, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 1.0), egui::Sense::hover());
                    ui.painter().rect_filled(l, 0.0, st.p.separator);
                    ui.add_space(6.0);
                }
                first = false;
                ui.horizontal(|ui| {
                    let mut on = m.enabled;
                    if widgets::toggle(ui, st, &mut on).changed() {
                        act = Some(Act::Toggle(m.id.clone(), on));
                    }
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            widgets::badge(
                                ui,
                                "Map Local",
                                st.p.green,
                                paint::with_alpha(st.p.green, 0.14),
                            );
                            text(
                                ui,
                                &format!("{}{}", m.host, m.path),
                                Role::BodyMedium,
                                if m.enabled { st.p.text } else { st.p.text3 },
                            );
                        });
                        let name = std::path::Path::new(&m.file)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        text(ui, &format!("Serve {name}"), Role::Callout, st.p.text2);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::icon_button(ui, st, Icon::Trash, "Delete rule", false, true)
                            .clicked()
                        {
                            act = Some(Act::Remove(m.id.clone()));
                        }
                    });
                });
            }
        });

        match act {
            Some(Act::Toggle(id, on)) => self.set_rule_enabled(&id, on),
            Some(Act::Edit(r)) => self.rule_form = Some(RuleForm::from_rule(&r)),
            Some(Act::Remove(id)) => self.remove_rule(&id),
            None => {}
        }
    }
}
