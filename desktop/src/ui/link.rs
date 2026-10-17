//! Can an app with the SDK reach this computer? A small icon in the toolbar says so at a glance, and a popover
//! lists every check with how to fix the ones that fail. The checks themselves run in the core.

use super::icons::{self, Icon};
use super::widgets::{self, Kind};
use crate::app::{App, Section};
use crate::paint::{self, cr};
use crate::theme::{radius, Role};
use egui::{
    pos2, vec2, Align2, Area, Color32, Context, Frame, Id, Margin, Order, Response, Stroke, Ui,
};
use proto::pb;

/// How the icon and the popover describe the link.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// The core runs without the SDK listener, or has not answered yet: show nothing alarming.
    Unknown,
    Off,
    Ok,
    Attention,
    Problem,
}

fn link_of(s: Option<&pb::SdkStatus>) -> Link {
    let Some(s) = s else { return Link::Unknown };
    if !s.available {
        return Link::Unknown;
    }
    match s.health.as_str() {
        "ok" => Link::Ok,
        "attention" => Link::Attention,
        "problem" => Link::Problem,
        "off" => Link::Off,
        // An older core that does not send checks: fall back to the switch.
        _ if s.enabled => Link::Ok,
        _ => Link::Off,
    }
}

impl App {
    fn link_color(&self, link: Link) -> Color32 {
        let p = &self.st.p;
        match link {
            Link::Ok => p.green,
            Link::Attention => p.orange,
            Link::Problem => p.red,
            Link::Off | Link::Unknown => p.text3,
        }
    }

    fn link_summary(&self, link: Link) -> String {
        let connected = self
            .devices
            .devices
            .iter()
            .filter(|d| d.status == pb::DeviceStatus::DeviceConnected as i32)
            .count();
        match link {
            Link::Unknown => "SDK connections".into(),
            Link::Off => "SDK connections are off".into(),
            Link::Problem => "Apps cannot connect. Click to see why".into(),
            Link::Attention => "Something needs attention. Click to see what".into(),
            Link::Ok if connected == 0 => "Ready: run an app with the SDK and it appears here".into(),
            Link::Ok => format!("{connected} app(s) connected"),
        }
    }

    /// The toolbar icon. A dot in the corner carries the verdict; a number says how many apps are connected.
    pub fn link_button(&mut self, ui: &mut Ui) -> Response {
        let st = self.st.clone();
        let link = link_of(self.sdk.as_ref());
        if link == Link::Unknown {
            // Nothing to say, but keep the layout stable.
            let (_, r) = ui.allocate_exact_size(vec2(0.0, 0.0), egui::Sense::hover());
            return r;
        }
        let tip = self.link_summary(link);
        let resp = widgets::icon_button(ui, &st, Icon::Phone, &tip, self.link_open, true);
        let col = self.link_color(link);
        let at = pos2(resp.rect.right() - 4.0, resp.rect.top() + 5.0);
        let p = ui.painter();
        // A ring in the toolbar colour keeps the dot readable on top of the icon.
        p.circle_filled(at, 5.5, st.p.toolbar);
        if link == Link::Off {
            p.circle_stroke(at, 3.5, Stroke::new(1.5, col));
        } else {
            p.circle_filled(at, 4.0, col);
        }
        resp
    }

    /// The popover under the icon. Called every frame; draws only while open.
    pub fn link_popover(&mut self, ctx: &Context) {
        if !self.link_open {
            return;
        }
        let Some(anchor) = self.link_rect else { return };
        let Some(s) = self.sdk.clone() else { return };
        let st = self.st.clone();
        let link = link_of(Some(&s));
        let screen = ctx.content_rect();
        let w = 400.0_f32.min(screen.width() - 24.0);
        let mut open_devices = false;

        let area = Area::new(Id::new("link-popover"))
            .order(Order::Foreground)
            .pivot(Align2::RIGHT_TOP)
            .fixed_pos(pos2(
                (anchor.right() + 8.0).min(screen.right() - 12.0),
                anchor.bottom() + 8.0,
            ))
            .show(ctx, |ui| {
                let mut frame = Frame::new()
                    .fill(st.p.card)
                    .stroke(Stroke::new(1.0, st.p.separator))
                    .corner_radius(cr(radius::SHEET))
                    .inner_margin(Margin::same(16));
                frame.shadow = egui::epaint::Shadow {
                    offset: [0, 12],
                    blur: 36,
                    spread: 0,
                    color: Color32::from_black_alpha(if st.p.is_dark() { 160 } else { 60 }),
                };
                frame.show(ui, |ui| {
                    ui.set_width(w - 32.0);
                    ui.horizontal(|ui| {
                        let col = self.link_color(link);
                        let (label, bg) = match link {
                            Link::Ok => ("Ready", paint::with_alpha(col, 0.14)),
                            Link::Attention => ("Needs attention", paint::with_alpha(col, 0.14)),
                            Link::Problem => ("Not working", paint::with_alpha(col, 0.14)),
                            _ => ("Off", paint::with_alpha(col, 0.14)),
                        };
                        widgets::badge(ui, label, col, bg);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("SDK connections")
                                    .font(Role::Headline.font())
                                    .color(st.p.text),
                            )
                            .selectable(false),
                        );
                    });
                    ui.add_space(8.0);
                    if s.checks.is_empty() {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(self.link_summary(link))
                                    .font(Role::Callout.font())
                                    .color(st.p.text2),
                            )
                            .wrap()
                            .selectable(false),
                        );
                    }
                    for c in &s.checks {
                        self.check_row(ui, c);
                        ui.add_space(6.0);
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if widgets::button(ui, &st, "Open Devices", Kind::Primary, None, true)
                            .clicked()
                        {
                            open_devices = true;
                        }
                    });
                });
            });

        if open_devices {
            self.section = Section::Devices;
            self.link_open = false;
            return;
        }
        // Close on Escape, or on a click anywhere outside the popover and its icon.
        let rect = area.response.rect;
        let (esc, clicked_at) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Escape),
                i.pointer
                    .any_pressed()
                    .then(|| i.pointer.interact_pos())
                    .flatten(),
            )
        });
        let outside = clicked_at.is_some_and(|p| !rect.contains(p) && !anchor.contains(p));
        if esc || outside {
            self.link_open = false;
        }
    }

    fn check_row(&self, ui: &mut Ui, c: &pb::SdkCheck) {
        let st = &self.st;
        let (icon, col) = match c.level.as_str() {
            "ok" => (Icon::Check, st.p.green),
            "error" => (Icon::Warning, st.p.red),
            "warn" => (Icon::Warning, st.p.orange),
            _ => (Icon::List, st.p.text3),
        };
        ui.horizontal_top(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(18.0, 20.0), egui::Sense::hover());
            icons::paint(ui.painter(), icon, r.center(), 14.0, col);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&c.title)
                            .font(Role::BodyMedium.font())
                            .color(st.p.text),
                    )
                    .wrap()
                    .selectable(false),
                );
                if !c.detail.is_empty() {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&c.detail)
                                .font(Role::Callout.font())
                                .color(st.p.text2),
                        )
                        .wrap()
                        .selectable(false),
                    );
                }
                if !c.fix.is_empty() && c.level != "ok" {
                    // Selectable, so a command can be copied.
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&c.fix)
                                .font(Role::Callout.font())
                                .color(st.p.accent),
                        )
                        .wrap()
                        .selectable(true),
                    );
                }
            });
        });
    }
}
