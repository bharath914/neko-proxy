//! Screens. Each is an `impl App` block so state stays in one place.

mod breakpoints;
mod detail;
mod devices;
pub mod icons;
mod overlay;
mod rules;
mod settings;
mod sidebar;
mod table;
mod toolbar;
mod viewer;
pub mod widgets;

use crate::app::{App, Section};
use crate::theme::Palette;
use egui::{Context, Frame};

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }
}

impl App {
    pub fn frame(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        let ctx = &ctx;
        static FIRST_FRAME: std::sync::Once = std::sync::Once::new();
        FIRST_FRAME.call_once(|| {
            // Benchmark hook: wall-clock time of the first frame, in ns.
            if std::env::var_os("PROXY_BENCH").is_some() {
                let ns = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                eprintln!("first-frame-ns {ns}");
            }
        });
        self.pump(ctx);
        self.automation_tick(ctx);
        self.follow_system(ctx);
        self.step_theme(ctx);
        self.refresh_hosts(ctx);
        self.shortcuts(ctx);

        let p: Palette = self.st.p.clone();
        // Navigation and controls span the window; everything else sits below.
        egui::Panel::top("topbar")
            .exact_size(toolbar::TOOLBAR_H)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(p.toolbar))
            .show(root, |ui| {
                let r = ui.max_rect();
                self.toolbar(ui, r);
            });

        // Below about 1040 px the host list becomes an icon rail.
        let side_w = if root.available_width() < 1040.0 { 64.0 } else { 236.0 };
        egui::Panel::left("sidebar")
            .exact_size(side_w)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(p.sidebar))
            .show(root, |ui| self.sidebar(ui));

        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.content))
            .show(root, |ui| match self.section {
                Section::Inspector => self.inspector(ui),
                Section::Breakpoints => self.breakpoints(ui),
                Section::Rules => self.rules_screen(ui),
                Section::Devices => self.devices_screen(ui),
                Section::Settings => self.settings(ui),
            });

        self.overlays(ctx);
    }

    /// Re-resolve theme and motion when the system or preferences change.
    fn follow_system(&mut self, ctx: &Context) {
        let scheme = match self.prefs.theme.as_str() {
            "light" => crate::theme::Scheme::Light,
            "dark" => crate::theme::Scheme::Dark,
            _ => self.sys.scheme(),
        };
        let reduced = self
            .prefs
            .reduce_motion
            .unwrap_or_else(|| self.sys.reduce_motion());
        let hc = self.sys.high_contrast();
        if scheme != self.st.p.scheme
            || reduced != self.st.reduced_motion
            || hc != self.st.reduced_transparency
        {
            self.sync_style(ctx);
        }
    }

    fn shortcuts(&mut self, ctx: &Context) {
        use egui::{Key, Modifiers};
        let typing = ctx.egui_wants_keyboard_input();
        let (slash, find, esc, clear, rec, settings, down, up, home, end, pgdn, pgup, copy_curl) =
            ctx.input(|i| {
                (
                    i.key_pressed(Key::Slash) && !i.modifiers.any(),
                    i.key_pressed(Key::F) && i.modifiers.command,
                    i.key_pressed(Key::Escape),
                    i.key_pressed(Key::K) && i.modifiers.command,
                    i.key_pressed(Key::R) && i.modifiers.command,
                    i.key_pressed(Key::Comma) && i.modifiers.command,
                    i.key_pressed(Key::ArrowDown),
                    i.key_pressed(Key::ArrowUp),
                    i.key_pressed(Key::Home),
                    i.key_pressed(Key::End),
                    i.key_pressed(Key::PageDown),
                    i.key_pressed(Key::PageUp),
                    i.modifiers
                        .matches_logically(Modifiers::COMMAND | Modifiers::SHIFT)
                        && i.key_pressed(Key::C),
                )
            });
        if find || (slash && !typing) {
            self.section = Section::Inspector;
            self.focus_search = true;
            // The keystroke focuses the field; it must not also type into it.
            ctx.input_mut(|i| {
                i.events
                    .retain(|e| !matches!(e, egui::Event::Text(t) if t == "/"));
            });
        }
        if esc {
            if self.sheet.is_some() {
                self.close_sheet();
            } else if !self.search.is_empty() && self.section == Section::Inspector {
                self.search.clear();
                self.filter_changed();
            }
        }
        if clear {
            self.open_sheet(crate::app::Sheet::ConfirmClear);
        }
        if rec {
            self.toggle_recording();
        }
        if settings {
            self.section = Section::Settings;
        }
        if copy_curl {
            if let Some(e) = self.selected.and_then(|id| self.details.get(&id)) {
                ctx.copy_text(e.detail.curl.clone());
                self.toast(ctx, "Copied as cURL", true);
            }
        }
        if self.section == Section::Inspector && !typing && self.sheet.is_none() {
            let page = ((self.view_height / 30.0) as isize - 1).max(1);
            let step: Option<isize> = if down {
                Some(1)
            } else if up {
                Some(-1)
            } else if pgdn {
                Some(page)
            } else if pgup {
                Some(-page)
            } else {
                None
            };
            let total = self.table.total() as isize;
            if total > 0 {
                let cur = self
                    .selected
                    .and_then(|id| self.table.row_of(id))
                    .map(|r| r as isize);
                let target = if home {
                    Some(0)
                } else if end {
                    Some(total - 1)
                } else {
                    step.map(|s| {
                        (cur.unwrap_or(if s > 0 { -1 } else { total }) + s).clamp(0, total - 1)
                    })
                };
                if let Some(row) = target {
                    if let Some(id) = self.table.get(row as usize).map(|s| s.id) {
                        self.select(Some(id));
                        self.reveal_row(row as usize);
                    }
                }
            }
        }
    }
}
