use super::icons::{self, Icon};
use super::widgets;
use crate::app::{App, Section};
use crate::motion::{self, QUICK};
use crate::paint::{self, cr};
use crate::theme::Role;
use egui::{pos2, vec2, Align, Layout, Rect, Sense, Ui, UiBuilder};

pub const TOOLBAR_H: f32 = 56.0;

impl App {
    /// The window's top bar. Navigation lives here, with the capture controls
    /// and the filters for the Inspector, and the proxy settings entry.
    pub fn toolbar(&mut self, ui: &mut Ui, rect: Rect) {
        let st = self.st.clone();
        let p = ui.painter().clone();
        p.rect_filled(rect, 0.0, st.p.toolbar);
        paint::hairline(&p, &st, rect.left_bottom(), rect.right_bottom());

        let inner = rect.shrink2(vec2(14.0, 0.0));
        let inspector = self.section == Section::Inspector;
        let roomy = inner.width() >= 1100.0;

        // Left group: where am I, then capture controls.
        let mut left = ui.new_child(
            UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::left_to_right(Align::Center)),
        );
        left.spacing_mut().item_spacing.x = 8.0;
        let sel = match self.section {
            Section::Inspector => 0,
            Section::Breakpoints => 1,
            Section::Rules => 2,
            Section::Devices => 3,
            Section::Settings => 4,
        };
        // A count on Breakpoints says something is waiting for you.
        let held = if self.paused.is_empty() {
            "Breakpoints".to_string()
        } else {
            format!("Breakpoints ({})", self.paused.len())
        };
        // A count on Devices says a phone is asking to pair.
        let devices = if self.devices.pending.is_empty() {
            "Devices".to_string()
        } else {
            format!("Devices ({})", self.devices.pending.len())
        };
        if let Some(i) = widgets::segmented(
            &mut left,
            &st,
            self.id("nav"),
            &["Inspector", &held, "Rules", &devices, "Settings"],
            sel,
        ) {
            self.section = match i {
                0 => Section::Inspector,
                1 => Section::Breakpoints,
                2 => Section::Rules,
                3 => Section::Devices,
                _ => Section::Settings,
            };
        }
        if inspector {
            left.add_space(10.0);
            if self.record_toggle(&mut left) {
                self.toggle_recording();
            }
            if widgets::icon_button(
                &mut left,
                &st,
                Icon::Trash,
                "Clear all flows  (Ctrl+K)",
                false,
                self.table.total() > 0,
            )
            .clicked()
            {
                self.open_sheet(crate::app::Sheet::ConfirmClear);
            }
        }

        // Right group, from the edge inwards: proxy settings, export, search, filter.
        let mut right = ui.new_child(
            UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::right_to_left(Align::Center)),
        );
        right.spacing_mut().item_spacing.x = 10.0;
        if self.proxy_button(&mut right, roomy) {
            self.proxy_tab = 0;
            self.open_sheet(crate::app::Sheet::Proxy);
        }
        let link = self.link_button(&mut right);
        if link.clicked() {
            self.link_open = !self.link_open;
        }
        self.link_rect = Some(link.rect);
        if !inspector {
            return;
        }
        right.menu_button("Export", |ui| {
            for kind in [
                "har",
                "postman",
                "k6",
                "wiremock",
                "msw",
                "mockoon",
                "openapi",
                "dart",
                "kotlin",
                "swift",
                "typescript",
                "dio",
            ] {
                if ui.button(kind).clicked() {
                    self.export_kind(kind);
                    ui.close();
                }
            }
        });
        let avail = inner.width();
        let search_w = (avail * 0.2).clamp(150.0, 280.0);
        let sid = self.id("search");
        let want = std::mem::take(&mut self.focus_search);
        let mut text = std::mem::take(&mut self.search);
        let resp = widgets::search_field(
            &mut right,
            &st,
            sid,
            &mut text,
            "Filter flows",
            search_w,
            want,
        );
        self.search = text;
        if resp.changed() {
            self.filter_changed();
        }
        let labels = ["All", "Errors", "Slow", "Pinned"];
        let sel = self.table.kind.clamp(0, 3) as usize;
        if let Some(i) = widgets::segmented(&mut right, &st, self.id("filter"), &labels, sel) {
            self.table.kind = i as i32;
            self.filter_changed();
        }
    }

    /// Whether any proxy rule is changing what the person sees.
    pub fn proxy_rules_active(&self) -> bool {
        let ssl = self.rules.ssl.clone().unwrap_or_default();
        ssl.mode != 0 || !ssl.exclude.is_empty() || self.rules.map_local.iter().any(|r| r.enabled)
    }

    /// Opens proxy settings. A dot says rules are in effect.
    fn proxy_button(&mut self, ui: &mut Ui, roomy: bool) -> bool {
        let st = self.st.clone();
        let resp = if roomy {
            widgets::button(
                ui,
                &st,
                "Proxy",
                widgets::Kind::Secondary,
                Some(Icon::Shield),
                true,
            )
        } else {
            widgets::icon_button(
                ui,
                &st,
                Icon::Shield,
                "Proxy settings: SSL proxying and Map Local",
                false,
                true,
            )
        };
        if self.proxy_rules_active() {
            ui.painter().circle_filled(
                pos2(resp.rect.right() - 3.0, resp.rect.top() + 4.0),
                3.5,
                st.p.accent,
            );
        }
        let tip = if roomy {
            resp.clone()
                .on_hover_text("Proxy settings: SSL proxying and Map Local")
        } else {
            resp.clone()
        };
        let _ = tip;
        resp.clicked()
    }

    /// Pill that shows and flips capture state. Returns true on click.
    fn record_toggle(&mut self, ui: &mut Ui) -> bool {
        let st = self.st.clone();
        let label = if self.recording {
            "Recording"
        } else {
            "Paused"
        };
        let g = paint::galley(ui, label, Role::BodyMedium, st.p.text);
        let w = g.size().x + 40.0;
        let (rect, resp) = ui.allocate_exact_size(vec2(w, 30.0), Sense::click());
        let ctx = ui.ctx();
        let down = resp.is_pointer_button_down_on() as u8 as f32;
        let press = motion::animate(ctx, resp.id.with("p"), down, QUICK, st.reduced_motion);
        let hover = motion::animate(
            ctx,
            resp.id.with("h"),
            resp.hovered() as u8 as f32,
            QUICK,
            st.reduced_motion,
        );
        let on = motion::animate(
            ctx,
            resp.id.with("on"),
            self.recording as u8 as f32,
            QUICK,
            st.reduced_motion,
        );
        let r = paint::scale_rect(rect, 1.0 - 0.03 * press);
        let p = ui.painter();
        let base = paint::mix(st.p.fill, paint::with_alpha(st.p.red, 0.16), on);
        p.rect_filled(
            r,
            cr(255),
            paint::mix(
                base,
                st.p.fill_press,
                0.5 * press + 0.25 * hover * (1.0 - on),
            ),
        );
        let dot = pos2(r.left() + 16.0, r.center().y);
        if self.recording {
            p.circle_filled(dot, 5.0, st.p.red);
        } else {
            icons::paint(p, Icon::Pause, dot, 14.0, st.p.text2);
        }
        let color = paint::mix(st.p.text2, st.p.text, on);
        p.galley(
            pos2(r.left() + 30.0, r.center().y - g.size().y / 2.0),
            paint::galley(ui, label, Role::BodyMedium, color),
            color,
        );
        if resp.has_focus() {
            paint::ring(ui.painter(), &st, r, 15);
        }
        resp.clone()
            .on_hover_text("Pause or resume capture  (Ctrl+R)");
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
    }
}
