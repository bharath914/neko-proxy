//! Messages held at breakpoints: a list on the left, an editor on the right.
//! Resuming sends only the fields that changed; the core applies them.

use super::icons::Icon;
use super::widgets::{self, Kind};
use crate::app::{App, Section};
use crate::edit::RuleKind;
use crate::paint::{self, cr};
use crate::theme::{radius, Role, Style};
use egui::{
    pos2, vec2, Align2, Color32, Frame, Margin, Rect, RichText, ScrollArea, Sense, Stroke, Ui,
    UiBuilder,
};
use proto::pb;

fn text(ui: &mut Ui, s: &str, role: Role, color: Color32) {
    ui.add(egui::Label::new(RichText::new(s).font(role.font()).color(color)).selectable(false));
}

fn field_label(ui: &mut Ui, st: &Style, s: &str) {
    ui.add_space(10.0);
    text(ui, s, Role::Caption, st.p.text2);
    ui.add_space(3.0);
}

impl App {
    pub fn breakpoints(&mut self, ui: &mut Ui) {
        let st = self.st.clone();
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, st.p.grouped);
        if self.paused.is_empty() {
            self.breakpoints_empty(ui, &st, full);
            return;
        }
        let list_w = (full.width() * 0.34).clamp(240.0, 340.0);
        let list = Rect::from_min_max(full.min, pos2(full.left() + list_w, full.bottom()));
        let editor = Rect::from_min_max(pos2(full.left() + list_w, full.top()), full.max);
        ui.painter().rect_filled(list, 0.0, st.p.content);
        paint::hairline(ui.painter(), &st, list.right_top(), list.right_bottom());

        let mut child = ui.new_child(UiBuilder::new().max_rect(list.shrink2(vec2(10.0, 12.0))));
        child.set_clip_rect(list.intersect(ui.clip_rect()));
        self.held_list(&mut child, &st);

        let mut child = ui.new_child(UiBuilder::new().max_rect(editor.shrink2(vec2(24.0, 0.0))));
        child.set_clip_rect(editor.intersect(ui.clip_rect()));
        self.held_editor(&mut child, &st);
    }

    fn breakpoints_empty(&mut self, ui: &mut Ui, st: &Style, full: Rect) {
        let c = full.center();
        let p = ui.painter().clone();
        paint::text(
            &p,
            ui,
            pos2(c.x, c.y - 44.0),
            Align2::CENTER_CENTER,
            "Nothing is held",
            Role::Title,
            st.p.text,
        );
        paint::text(
            &p,
            ui,
            pos2(c.x, c.y - 14.0),
            Align2::CENTER_CENTER,
            "A breakpoint holds a request or response here until you resume it.",
            Role::Body,
            st.p.text2,
        );
        let active = self
            .rules
            .rules
            .iter()
            .filter(|r| r.enabled && matches!(r.action, Some(pb::rule::Action::Breakpoint(_))))
            .count();
        paint::text(
            &p,
            ui,
            pos2(c.x, c.y + 8.0),
            Align2::CENTER_CENTER,
            &if active == 0 {
                "No breakpoint rules yet.".to_string()
            } else {
                format!("{active} breakpoint rule{} watching.", if active == 1 { "" } else { "s" })
            },
            Role::Callout,
            st.p.text3,
        );
        let r = Rect::from_center_size(pos2(c.x, c.y + 50.0), vec2(180.0, 28.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(r));
        if widgets::button(&mut child, st, "New breakpoint", Kind::Primary, Some(Icon::Plus), true)
            .clicked()
        {
            self.rule_form = Some(crate::edit::RuleForm::new(RuleKind::Breakpoint));
            self.section = Section::Rules;
        }
    }

    fn held_list(&mut self, ui: &mut Ui, st: &Style) {
        text(ui, "Held", Role::Headline, st.p.text);
        ui.add_space(8.0);
        let items = self.paused.clone();
        ScrollArea::vertical()
            .id_salt("held-list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for p in &items {
                    let selected = self.bp_selected == Some(p.flow_id);
                    let (rect, resp) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
                    let pt = ui.painter().clone();
                    if selected {
                        pt.rect_filled(rect, cr(radius::CONTROL), st.p.accent_soft);
                    } else if resp.hovered() {
                        pt.rect_filled(rect, cr(radius::CONTROL), st.p.fill_hover);
                    }
                    let request = p.phase == pb::MessagePhase::PhaseRequest as i32;
                    let (tag, col) = if request {
                        ("Request", st.p.orange)
                    } else {
                        ("Response", st.p.purple)
                    };
                    let host = p
                        .summary
                        .as_ref()
                        .map(|s| s.host.clone())
                        .unwrap_or_default();
                    paint::text(
                        &pt,
                        ui,
                        pos2(rect.left() + 12.0, rect.top() + 18.0),
                        Align2::LEFT_CENTER,
                        &format!("{}  {}", p.method, host),
                        Role::BodyMedium,
                        st.p.text,
                    );
                    paint::text_fit(
                        &pt,
                        ui,
                        pos2(rect.left() + 12.0, rect.top() + 38.0),
                        rect.width() - 24.0,
                        &if request {
                            p.path.clone()
                        } else {
                            p.summary.as_ref().map(|s| s.path.clone()).unwrap_or_default()
                        },
                        Role::MonoSmall,
                        st.p.text2,
                    );
                    paint::text(
                        &pt,
                        ui,
                        pos2(rect.right() - 12.0, rect.top() + 18.0),
                        Align2::RIGHT_CENTER,
                        tag,
                        Role::Caption,
                        col,
                    );
                    if resp.is_pointer_button_down_on() || resp.clicked() {
                        self.select_held(Some(p.flow_id));
                    }
                    resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                    ui.add_space(2.0);
                }
            });
    }

    fn held_editor(&mut self, ui: &mut Ui, st: &Style) {
        let Some(id) = self.bp_selected else {
            return;
        };
        let Some(orig) = self.paused.iter().find(|p| p.flow_id == id).cloned() else {
            return;
        };
        if self.bp_edit.as_ref().map(|e| e.flow_id) != Some(id) {
            self.bp_edit = Some(crate::edit::BpEdit::new(&orig));
        }
        let request = orig.phase == pb::MessagePhase::PhaseRequest as i32;
        let summary = orig.summary.clone().unwrap_or_default();
        let changed = self
            .bp_edit
            .as_ref()
            .map(|e| e.changed(&orig))
            .unwrap_or(false);

        ScrollArea::vertical()
            .id_salt("held-editor")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    text(
                        ui,
                        if request { "Request is held" } else { "Response is held" },
                        Role::Title,
                        st.p.text,
                    );
                    widgets::badge(
                        ui,
                        &format!("rule {}", orig.rule_id),
                        st.p.text2,
                        st.p.fill,
                    );
                });
                text(
                    ui,
                    &format!(
                        "{}://{}{}  ·  gives up after {} s and lets it through",
                        summary.scheme,
                        summary.host,
                        if request { &orig.path } else { &summary.path },
                        orig.timeout_s
                    ),
                    Role::Callout,
                    st.p.text2,
                );

                // Buttons first, so they never scroll out of reach.
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let label = if changed { "Resume with edits" } else { "Resume" };
                    if widgets::button(ui, st, label, Kind::Primary, Some(Icon::Check), true)
                        .clicked()
                    {
                        self.resume_held(id);
                    }
                    if widgets::button(ui, st, "Abort", Kind::Destructive, Some(Icon::Close), true)
                        .clicked()
                    {
                        self.abort_held(id);
                    }
                    if changed
                        && widgets::button(ui, st, "Reset edits", Kind::Plain, None, true).clicked()
                    {
                        self.select_held(Some(id));
                    }
                });

                let Some(edit) = self.bp_edit.as_mut() else {
                    return;
                };
                Frame::new()
                    .fill(st.p.card)
                    .stroke(Stroke::new(1.0, st.p.separator))
                    .corner_radius(cr(radius::CARD))
                    .inner_margin(Margin::symmetric(16, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if request {
                            field_label(ui, st, "Method");
                            ui.add(
                                egui::TextEdit::singleline(&mut edit.method)
                                    .font(Role::Mono.font())
                                    .desired_width(120.0),
                            );
                            field_label(ui, st, "Path and query");
                            ui.add(
                                egui::TextEdit::singleline(&mut edit.path)
                                    .font(Role::Mono.font())
                                    .desired_width(f32::INFINITY),
                            );
                        } else {
                            field_label(ui, st, "Status");
                            ui.add(
                                egui::TextEdit::singleline(&mut edit.status)
                                    .font(Role::Mono.font())
                                    .desired_width(120.0),
                            );
                        }
                        field_label(ui, st, "Headers, one per line");
                        ui.add(
                            egui::TextEdit::multiline(&mut edit.headers)
                                .font(Role::MonoSmall.font())
                                .desired_rows(5)
                                .desired_width(f32::INFINITY),
                        );
                        field_label(
                            ui,
                            st,
                            if edit.binary {
                                "Body (binary: passed through unchanged)"
                            } else {
                                "Body"
                            },
                        );
                        ui.add_enabled(
                            !edit.binary,
                            egui::TextEdit::multiline(&mut edit.body)
                                .font(Role::MonoSmall.font())
                                .desired_rows(10)
                                .desired_width(f32::INFINITY),
                        );
                        ui.add_space(8.0);
                    });
                ui.add_space(24.0);
            });
    }
}
