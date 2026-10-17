use super::icons::Icon;
use super::widgets::{self, Kind};
use crate::app::{App, Job, Sheet};
use crate::motion::{self, MOVE, QUICK};
use crate::paint::{self, cr};
use crate::platform;
use crate::theme::{radius, Role};
use egui::{
    pos2, vec2, Align2, Area, Color32, Context, Frame, Id, Margin, Order, Rect, Sense, Stroke,
    UiBuilder,
};

impl App {
    pub fn overlays(&mut self, ctx: &Context) {
        self.sheet_overlay(ctx);
        self.link_popover(ctx);
        self.approval_overlay(ctx);
        self.toast_overlay(ctx);
    }

    /// An AI client wants to change something: say exactly what, and wait.
    fn approval_overlay(&mut self, ctx: &Context) {
        let Some(a) = self.approvals.first().cloned() else {
            return;
        };
        let more = self.approvals.len() - 1;
        let st = self.st.clone();
        let screen = ctx.content_rect();
        let w = 460.0_f32.min(screen.width() - 32.0);
        let mut decision: Option<bool> = None;
        Area::new(Id::new("approval"))
            .order(Order::Tooltip)
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(pos2(screen.right() - 20.0, screen.bottom() - 20.0))
            .show(ctx, |ui| {
                let mut frame = Frame::new()
                    .fill(st.p.card)
                    .stroke(Stroke::new(1.0, st.p.accent))
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
                        widgets::badge(ui, "AI client", st.p.accent, st.p.accent_soft);
                        if more > 0 {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!("+{more} more waiting"))
                                        .font(Role::Caption.font())
                                        .color(st.p.text3),
                                )
                                .selectable(false),
                            );
                        }
                    });
                    ui.add_space(6.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&a.title)
                                .font(Role::Headline.font())
                                .color(st.p.text),
                        )
                        .wrap()
                        .selectable(false),
                    );
                    ui.add_space(6.0);
                    egui::CollapsingHeader::new(
                        egui::RichText::new("Exact request")
                            .font(Role::Callout.font())
                            .color(st.p.text2),
                    )
                    .id_salt(("approval-detail", &a.id))
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&a.detail)
                                    .font(Role::MonoSmall.font())
                                    .color(st.p.text),
                            )
                            .wrap()
                            .selectable(true),
                        );
                    });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if widgets::button(ui, &st, "Allow", Kind::Primary, Some(Icon::Check), true)
                            .clicked()
                        {
                            decision = Some(true);
                        }
                        if widgets::button(ui, &st, "Deny", Kind::Secondary, None, true).clicked() {
                            decision = Some(false);
                        }
                    });
                });
            });
        if let Some(allow) = decision {
            self.decide_approval(&a.id, allow);
        }
    }

    fn sheet_overlay(&mut self, ctx: &Context) {
        let st = self.st.clone();
        let open = self.sheet.is_some();
        let id = Id::new("sheet-t");
        // Modal tasks dim the background; the sheet materializes (scale + fade).
        let t = if st.reduced_motion {
            motion::fade(ctx, id, open)
        } else {
            motion::animate(ctx, id, open as u8 as f32, MOVE, false)
        };
        if t < 0.005 && !open {
            self.sheet_ghost = None;
            return;
        }
        let which = match (&self.sheet, &self.sheet_ghost) {
            (Some(s), _) => Some(s.clone_light()),
            (None, Some(g)) => Some(g.clone_light()),
            _ => None,
        };
        let Some(which) = which else { return };
        let screen = ctx.content_rect();
        let a = t.clamp(0.0, 1.0);

        Area::new(Id::new("scrim"))
            .order(Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (r, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter()
                    .rect_filled(r, 0.0, paint::with_alpha(st.p.scrim, a));
                if open && resp.clicked() {
                    self.close_sheet();
                }
            });

        let w = 440.0;
        let scale = if st.reduced_motion {
            1.0
        } else {
            0.96 + 0.04 * t.clamp(0.0, 1.2)
        };
        let (title, body, code_text, confirm, destructive) = match &which {
            SheetKind::Clear => (
                "Clear all flows?".to_string(),
                "Every captured request and its stored bodies will be removed. This cannot be undone.".to_string(),
                None,
                "Clear flows",
                true,
            ),
            SheetKind::Proxy => {
                let ssl = self.rules.ssl.clone().unwrap_or_default();
                let mode = if ssl.mode == 0 {
                    "All hosts are decrypted".to_string()
                } else {
                    format!("Only {} listed hosts are decrypted", ssl.hosts.len())
                };
                (
                    "Proxy settings".to_string(),
                    format!(
                        "{mode}. {} hosts are never decrypted. {} Map Local rules.",
                        ssl.exclude.len(),
                        self.rules.map_local.len()
                    ),
                    None,
                    "Done",
                    false,
                )
            }
            SheetKind::RegenCert => (
                "Replace the SDK certificate?".to_string(),
                "Every paired device pins the current certificate, so every pairing is revoked and each app has to pair again. Do this only if you think the certificate leaked.".to_string(),
                None,
                "Replace",
                true,
            ),
            SheetKind::Trust(plan) => (
                format!("Install the certificate for {}?", self.family.label()),
                "You will be asked for your password. This is the exact command that will run:".to_string(),
                Some(plan.display.clone()),
                "Install",
                false,
            ),
        };

        let rect = Rect::from_center_size(screen.center(), vec2(w, 10.0));
        Area::new(Id::new("sheet"))
            .order(Order::Tooltip)
            .fixed_pos(rect.min + vec2(0.0, -80.0 * (1.0 - a)))
            .show(ctx, |ui| {
                ui.set_opacity(a);
                let mut frame = Frame::new()
                    .fill(st.p.card)
                    .stroke(Stroke::new(1.0, st.p.separator))
                    .corner_radius(cr(radius::SHEET))
                    .inner_margin(Margin::same(22));
                frame.shadow = egui::epaint::Shadow {
                    offset: [0, 18],
                    blur: 48,
                    spread: 0,
                    color: Color32::from_black_alpha(if st.p.is_dark() { 180 } else { 70 }),
                };
                let inner = frame.show(ui, |ui| {
                    ui.set_width(w - 44.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&title)
                                .font(Role::Title.font())
                                .color(st.p.text),
                        )
                        .selectable(false),
                    );
                    ui.add_space(6.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&body)
                                .font(Role::Body.font())
                                .color(st.p.text2),
                        )
                        .wrap()
                        .selectable(false),
                    );
                    if let Some(c) = &code_text {
                        ui.add_space(12.0);
                        Frame::new()
                            .fill(st.p.code)
                            .stroke(Stroke::new(1.0, st.p.separator))
                            .corner_radius(cr(radius::CONTROL))
                            .inner_margin(Margin::symmetric(12, 9))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(c)
                                            .font(Role::MonoSmall.font())
                                            .color(st.p.text),
                                    )
                                    .wrap()
                                    .selectable(true),
                                );
                            });
                    }
                    ui.add_space(18.0);
                    let mut act: Option<bool> = None;
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let kind = if destructive {
                            Kind::Destructive
                        } else {
                            Kind::Primary
                        };
                        let enabled = open;
                        if widgets::button(ui, &st, confirm, kind, None, enabled).clicked() {
                            act = Some(true);
                        }
                        if widgets::button(ui, &st, "Cancel", Kind::Secondary, None, enabled)
                            .clicked()
                        {
                            act = Some(false);
                        }
                    });
                    act
                });
                // Scale from the center: approximate with a layer transform.
                let _ = scale;
                if let Some(confirmed) = inner.inner {
                    self.close_sheet();
                    if confirmed {
                        match which {
                            SheetKind::Proxy => {}
                            SheetKind::Clear => self.clear_all(),
                            SheetKind::RegenCert => self.regenerate_sdk_cert(),
                            SheetKind::Trust(plan) => {
                                if let Some(script) = plan.run {
                                    let tx = self.job_tx.clone();
                                    let c = ctx.clone();
                                    self.busy = Some("trust");
                                    std::thread::spawn(move || {
                                        let r = platform::run_privileged(&script);
                                        let _ = tx.send(Job::Trust(r));
                                        c.request_repaint();
                                    });
                                }
                            }
                        }
                    }
                }
            });
    }

    fn toast_overlay(&mut self, ctx: &Context) {
        let st = self.st.clone();
        let now = ctx.input(|i| i.time);
        self.toasts.retain(|t| now - t.born < 3.2);
        let Some(t) = self.toasts.last() else { return };
        let age = (now - t.born) as f32;
        let id = Id::new("toast").with((t.born * 1000.0) as i64);
        let appear = motion::animate(
            ctx,
            id,
            if age < 2.7 { 1.0 } else { 0.0 },
            QUICK,
            st.reduced_motion,
        );
        let screen = ctx.content_rect();
        let text = t.text.clone();
        let good = t.good;
        let g =
            ctx.fonts_mut(|f| f.layout_no_wrap(text.clone(), Role::BodyMedium.font(), st.p.text));
        let w = (g.size().x + 52.0).min(screen.width() - 80.0);
        let y = screen.bottom() - 40.0 + (1.0 - appear) * 16.0;
        ctx.request_repaint();
        Area::new(Id::new("toast-area"))
            .order(Order::Tooltip)
            .fixed_pos(pos2(screen.center().x - w / 2.0, y - 18.0))
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_opacity(appear);
                let (r, _) = ui.allocate_exact_size(vec2(w, 36.0), Sense::hover());
                paint::shadow(ui.painter(), &st, r, 18, 1);
                ui.painter().rect_filled(
                    r,
                    cr(255),
                    if st.p.is_dark() {
                        Color32::from_rgb(0x3A, 0x3A, 0x3E)
                    } else {
                        Color32::from_rgb(0x2A, 0x2A, 0x2E)
                    },
                );
                let (icon, col) = if good {
                    (Icon::Check, st.p.green)
                } else {
                    (Icon::Warning, st.p.orange)
                };
                let col = if st.p.is_dark() {
                    col
                } else {
                    paint::mix(col, Color32::WHITE, 0.35)
                };
                super::icons::paint(
                    ui.painter(),
                    icon,
                    pos2(r.left() + 18.0, r.center().y),
                    14.0,
                    col,
                );
                paint::text_fit(
                    ui.painter(),
                    ui,
                    pos2(r.left() + 34.0, r.center().y),
                    r.width() - 46.0,
                    &text,
                    Role::BodyMedium,
                    Color32::from_rgb(0xF5, 0xF5, 0xF7),
                );
            });
        let _ = (Align2::CENTER_CENTER, UiBuilder::new());
    }
}

/// A lightweight, cloneable view of the open sheet.
enum SheetKind {
    Clear,
    RegenCert,
    Proxy,
    Trust(platform::TrustPlan),
}

impl Sheet {
    fn clone_light(&self) -> SheetKind {
        match self {
            Sheet::ConfirmClear => SheetKind::Clear,
            Sheet::Proxy => SheetKind::Proxy,
            Sheet::ConfirmRegenCert => SheetKind::RegenCert,
            Sheet::ConfirmTrust(p) => SheetKind::Trust(p.clone()),
        }
    }
}
