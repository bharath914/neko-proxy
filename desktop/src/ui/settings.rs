use super::icons::Icon;
use super::widgets::{self, Kind};
use crate::app::{App, Job, Sheet};
use crate::paint::{self, cr};
use crate::platform::{self, LaunchOptions};
use proto::pb;
use crate::theme::{radius, Role, Style};
use egui::text::{LayoutJob, TextFormat};
use egui::{pos2, vec2, Align, Color32, Frame, Layout, Margin, Rect, ScrollArea, Stroke, Ui};

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

fn group_title(ui: &mut Ui, st: &Style, title: &str, note: &str) {
    ui.add_space(22.0);
    ui.add(egui::Label::new(job(title, Role::Headline, st.p.text)).selectable(false));
    if !note.is_empty() {
        ui.add(
            egui::Label::new(job(note, Role::Callout, st.p.text2))
                .wrap()
                .selectable(false),
        );
    }
    ui.add_space(8.0);
}

fn group(ui: &mut Ui, st: &Style, add: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(st.p.card)
        .stroke(Stroke::new(1.0, st.p.separator))
        .corner_radius(cr(radius::CARD))
        .inner_margin(Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// A labeled row with trailing content, like System Settings.
fn row(ui: &mut Ui, st: &Style, title: &str, subtitle: &str, trailing: impl FnOnce(&mut Ui)) {
    row_w(ui, st, title, subtitle, 190.0, trailing)
}

/// A row with no trailing control: the text may use the full width.
fn row_plain(ui: &mut Ui, st: &Style, title: &str, subtitle: &str) {
    row_w(ui, st, title, subtitle, 0.0, |_| {})
}

fn row_w(
    ui: &mut Ui,
    st: &Style,
    title: &str,
    subtitle: &str,
    reserve: f32,
    trailing: impl FnOnce(&mut Ui),
) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width((ui.available_width() - reserve).max(160.0));
            ui.add(egui::Label::new(job(title, Role::Body, st.p.text)).selectable(false));
            if !subtitle.is_empty() {
                ui.add(
                    egui::Label::new(job(subtitle, Role::Callout, st.p.text2))
                        .wrap()
                        .selectable(false),
                );
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), trailing);
    });
}

fn code(ui: &mut Ui, st: &Style, text: &str) {
    Frame::new()
        .fill(st.p.code)
        .stroke(Stroke::new(1.0, st.p.separator))
        .corner_radius(cr(radius::CONTROL))
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                egui::Label::new(job(text, Role::MonoSmall, st.p.text))
                    .wrap()
                    .selectable(true),
            );
        });
}

fn divider(ui: &mut Ui, st: &Style) {
    ui.add_space(8.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 0.0, st.p.separator);
    ui.add_space(8.0);
}

impl App {
    pub fn settings(&mut self, ui: &mut Ui) {
        let st = self.st.clone();
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, st.p.grouped);
        self.refresh_trust(ui.ctx());

        let col_w = (full.width() - 64.0).clamp(320.0, 700.0);
        let left = full.center().x - col_w / 2.0;
        let rect = Rect::from_min_max(pos2(left, full.top()), pos2(left + col_w, full.bottom()));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        child.set_clip_rect(full.intersect(ui.clip_rect()));

        ScrollArea::vertical()
            .id_salt("settings")
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                ui.set_width(col_w);
                ui.add_space(26.0);
                ui.add(
                    egui::Label::new(job("Settings", Role::LargeTitle, st.p.text))
                        .selectable(false),
                );
                self.settings_certificate(ui, &st);
                self.settings_launch(ui, &st);
                self.settings_terminal(ui, &st);
                self.settings_ai(ui, &st);
                self.settings_appearance(ui, &st);
                self.settings_data(ui, &st);
                self.settings_about(ui, &st);
                ui.add_space(40.0);
            });
    }

    fn settings_certificate(&mut self, ui: &mut Ui, st: &Style) {
        group_title(
            ui,
            st,
            "Certificate",
            "HTTPS traffic is decrypted with a certificate made for this computer. Trust it once so apps accept it.",
        );
        let Some(ca) = self.ca.clone() else {
            group(ui, st, |ui| {
                ui.horizontal(|ui| {
                    widgets::spinner(ui, st, 16.0);
                    ui.add(
                        egui::Label::new(job("Waiting for the core…", Role::Body, st.p.text2))
                            .selectable(false),
                    );
                });
            });
            return;
        };
        let trusted = self.trusted_system;
        let path = ca.cert_path.clone();
        group(ui, st, |ui| {
            row(
                ui,
                st,
                &ca.subject,
                &format!("Expires {}", ca.not_after.split('T').next().unwrap_or("")),
                |ui| match trusted {
                    Some(true) => {
                        widgets::badge(
                            ui,
                            "Trusted by the system",
                            st.p.green,
                            paint::with_alpha(st.p.green, 0.14),
                        );
                    }
                    Some(false) => {
                        widgets::badge(
                            ui,
                            "Not installed",
                            st.p.orange,
                            paint::with_alpha(st.p.orange, 0.14),
                        );
                    }
                    None => {}
                },
            );
            divider(ui, st);
            let short = if ca.sha256.len() >= 32 {
                format!("{}…", &ca.sha256[..32])
            } else {
                ca.sha256.clone()
            };
            row(ui, st, "Fingerprint (SHA-256)", &short, |ui| {
                if widgets::button(ui, st, "Copy", Kind::Plain, Some(Icon::Copy), true).clicked() {
                    ui.ctx().copy_text(ca.sha256.clone());
                    self.toast(ui.ctx(), "Fingerprint copied", true);
                }
            });
            divider(ui, st);
            let storage = if ca.key_storage == "keyring" {
                "The private key is in your desktop keyring and never leaves this computer."
            } else {
                "The private key is a file in the data folder, readable only by you."
            };
            row_plain(ui, st, "Private key", storage);
            divider(ui, st);
            row(ui, st, "Certificate file", &path, |ui| {
                if widgets::button(ui, st, "Show", Kind::Plain, Some(Icon::Folder), true).clicked()
                {
                    if let Some(dir) = std::path::Path::new(&path).parent() {
                        platform::open_path(&dir.display().to_string());
                    }
                }
                if widgets::button(ui, st, "Copy path", Kind::Plain, Some(Icon::Copy), true)
                    .clicked()
                {
                    ui.ctx().copy_text(path.clone());
                    self.toast(ui.ctx(), "Path copied", true);
                }
            });
        });

        // Install helpers: show the command, then run it only on confirmation.
        ui.add_space(12.0);
        let plan = platform::system_plan(self.family, &path);
        let nss = platform::nss_plan(&path);
        let installed_nss = self.trusted_nss;
        let has_certutil = platform::which("certutil").is_some();
        let family_label = self.family.label();
        group(ui, st, |ui| {
            row_plain(ui, st, &format!("Install for {family_label}"), plan.detail);
            ui.add_space(8.0);
            code(ui, st, &plan.display);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let can_run = plan.run.is_some() && self.busy.is_none();
                if widgets::button(
                    ui,
                    st,
                    "Install…",
                    Kind::Primary,
                    Some(Icon::Shield),
                    can_run,
                )
                .clicked()
                {
                    self.open_sheet(Sheet::ConfirmTrust(plan.clone()));
                }
                if widgets::button(
                    ui,
                    st,
                    "Copy command",
                    Kind::Secondary,
                    Some(Icon::Copy),
                    true,
                )
                .clicked()
                {
                    ui.ctx().copy_text(plan.display.clone());
                    self.toast(ui.ctx(), "Command copied", true);
                }
                if self.busy.is_some() {
                    widgets::spinner(ui, st, 16.0);
                }
            });
            divider(ui, st);
            row(ui, st, nss.title, nss.detail, |ui| match installed_nss {
                Some(true) => {
                    widgets::badge(
                        ui,
                        "Installed",
                        st.p.green,
                        paint::with_alpha(st.p.green, 0.14),
                    );
                }
                Some(false) => {
                    widgets::badge(
                        ui,
                        "Not installed",
                        st.p.orange,
                        paint::with_alpha(st.p.orange, 0.14),
                    );
                }
                None => {}
            });
            ui.add_space(8.0);
            code(ui, st, &nss.display);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if has_certutil && self.busy.is_none() {
                    if widgets::button(ui, st, "Install for browsers", Kind::Secondary, Some(Icon::Shield), true).clicked() {
                        let tx = self.job_tx.clone();
                        let ctx = ui.ctx().clone();
                        let cmd = format!(
                            "mkdir -p \"$HOME/.pki/nssdb\" && ([ -f \"$HOME/.pki/nssdb/cert9.db\" ] || certutil -d \"sql:$HOME/.pki/nssdb\" -N --empty-password) && certutil -d \"sql:$HOME/.pki/nssdb\" -A -t \"C,,\" -n neko-ca -i '{}'",
                            path.replace('\'', "'\\''")
                        );
                        self.busy = Some("nss");
                        std::thread::spawn(move || {
                            let r = platform::run_plain(&cmd);
                            let _ = tx.send(Job::Trust(r));
                            ctx.request_repaint();
                        });
                    }
                } else if !has_certutil {
                    ui.add(egui::Label::new(job(
                        "certutil is not installed. Install nss-tools (Fedora) or libnss3-tools (Debian).",
                        Role::Callout,
                        st.p.text3,
                    )).wrap().selectable(false));
                }
                if widgets::button(ui, st, "Copy command", Kind::Secondary, Some(Icon::Copy), true).clicked() {
                    ui.ctx().copy_text(nss.display.clone());
                    self.toast(ui.ctx(), "Command copied", true);
                }
            });
            ui.add_space(6.0);
            ui.add(egui::Label::new(job(
                "Firefox: the launcher below uses a fresh profile that already trusts this certificate. \
                 For your own profile, enable security.enterprise_roots.enabled.",
                Role::Callout,
                st.p.text2,
            )).wrap().selectable(false));
        });
    }

    fn settings_launch(&mut self, ui: &mut Ui, st: &Style) {
        group_title(
            ui,
            st,
            "Open a browser through the proxy",
            "Starts a separate window with its own profile. Your normal browser and system proxy settings are untouched.",
        );
        let proxy = self.proxy_addr();
        let ca_path = self.ca_path();
        let spki = self
            .ca
            .as_ref()
            .map(|c| c.spki_sha256.clone())
            .filter(|s| !s.is_empty());
        let browsers = self.browsers.clone();
        let use_spki = self.spki_launch;
        let mut toggle = use_spki;
        group(ui, st, |ui| {
            if browsers.is_empty() {
                ui.add(egui::Label::new(job(
                    "No supported browser was found. Chromium, Chrome, Edge, Brave, Vivaldi and Firefox are detected on your PATH.",
                    Role::Body,
                    st.p.text2,
                )).wrap().selectable(false));
            }
            for (i, b) in browsers.iter().enumerate() {
                if i > 0 {
                    divider(ui, st);
                }
                row(ui, st, b.name, &b.exe.display().to_string(), |ui| {
                    let ready = ca_path.is_some();
                    if widgets::button(ui, st, "Open", Kind::Primary, Some(Icon::Globe), ready)
                        .clicked()
                    {
                        if let Some(ca) = &ca_path {
                            let opts = LaunchOptions {
                                proxy: &proxy,
                                ca_path: ca,
                                spki: if use_spki { spki.as_deref() } else { None },
                            };
                            let r = platform::launch(b, &opts);
                            let _ = self.job_tx.send(Job::Browser(r));
                        }
                    }
                });
            }
            if browsers.iter().any(|b| !b.firefox) {
                divider(ui, st);
                row(
                    ui,
                    st,
                    "Trust the certificate in these windows only",
                    "Chromium family. Skips installing the certificate, for quick checks. Experimental.",
                    |ui| {
                        widgets::toggle(ui, st, &mut toggle);
                    },
                );
            }
        });
        self.spki_launch = toggle;
    }

    fn settings_terminal(&mut self, ui: &mut Ui, st: &Style) {
        group_title(
            ui,
            st,
            "Terminal and tools",
            "Paste into a shell to send curl, Node, Python and others through the proxy.",
        );
        let snippet = platform::env_snippet(
            &self.proxy_addr(),
            &self.ca_path().unwrap_or_else(|| "ca.pem".into()),
        );
        group(ui, st, |ui| {
            code(ui, st, &snippet);
            ui.add_space(10.0);
            if widgets::button(
                ui,
                st,
                "Copy snippet",
                Kind::Secondary,
                Some(Icon::Copy),
                true,
            )
            .clicked()
            {
                ui.ctx().copy_text(snippet.clone());
                self.toast(ui.ctx(), "Environment snippet copied", true);
            }
        });
    }

    fn settings_appearance(&mut self, ui: &mut Ui, st: &Style) {
        group_title(ui, st, "Appearance", "");
        let theme_sel = match self.prefs.theme.as_str() {
            "light" => 1,
            "dark" => 2,
            _ => 0,
        };
        let motion_sel = match self.prefs.reduce_motion {
            None => 0,
            Some(true) => 1,
            Some(false) => 2,
        };
        let mut new_theme = None;
        let mut new_motion = None;
        group(ui, st, |ui| {
            row(ui, st, "Theme", "Follows your desktop by default.", |ui| {
                if let Some(i) = widgets::segmented(
                    ui,
                    st,
                    self.id("theme"),
                    &["System", "Light", "Dark"],
                    theme_sel,
                ) {
                    new_theme = Some(i);
                }
            });
            divider(ui, st);
            row(
                ui,
                st,
                "Motion",
                "Reduced motion swaps springs for quick fades.",
                |ui| {
                    if let Some(i) = widgets::segmented(
                        ui,
                        st,
                        self.id("motion"),
                        &["System", "Reduced", "Full"],
                        motion_sel,
                    ) {
                        new_motion = Some(i);
                    }
                },
            );
        });
        if let Some(i) = new_theme {
            self.prefs.theme = ["system", "light", "dark"][i].into();
            self.prefs.save();
            self.sync_style(ui.ctx());
        }
        if let Some(i) = new_motion {
            self.prefs.reduce_motion = [None, Some(true), Some(false)][i];
            self.prefs.save();
            self.sync_style(ui.ctx());
        }
    }

    fn settings_data(&mut self, ui: &mut Ui, st: &Style) {
        group_title(ui, st, "Data", "Captures are stored on this computer only.");
        let dir = self
            .status
            .as_ref()
            .map(|s| s.data_dir.clone())
            .unwrap_or_default();
        let count = self.status.as_ref().map(|s| s.flows).unwrap_or(0);

        group(ui, st, |ui| {
            row(
                ui,
                st,
                "Stored flows",
                &format!(
                    "{} in {}",
                    crate::fmt::count(count as usize),
                    if dir.is_empty() {
                        "the data folder"
                    } else {
                        &dir
                    }
                ),
                |ui| {
                    if !dir.is_empty()
                        && widgets::button(ui, st, "Show", Kind::Plain, Some(Icon::Folder), true)
                            .clicked()
                    {
                        platform::open_path(&dir);
                    }
                },
            );
            divider(ui, st);
            row(
                ui,
                st,
                "Secrets are hidden in exports",
                "Authorization, cookies and token-shaped values become [redacted].",
                |ui| {
                    ui.label("Always on");
                },
            );
            divider(ui, st);
            row(
                ui,
                st,
                "Export as HAR",
                "The current filter applies. Opens in Chrome DevTools.",
                |ui| {
                    if widgets::button(
                        ui,
                        st,
                        "Export…",
                        Kind::Secondary,
                        Some(Icon::Download),
                        count > 0,
                    )
                    .clicked()
                    {
                        self.export_har();
                    }
                },
            );
            divider(ui, st);
            row(
                ui,
                st,
                "Clear all flows",
                "Removes every capture and its stored bodies. This cannot be undone.",
                |ui| {
                    if widgets::button(
                        ui,
                        st,
                        "Clear…",
                        Kind::Destructive,
                        Some(Icon::Trash),
                        count > 0,
                    )
                    .clicked()
                    {
                        self.open_sheet(Sheet::ConfirmClear);
                    }
                },
            );
        });
        self.redact_export = true;
    }

    fn settings_ai(&mut self, ui: &mut Ui, st: &Style) {
        group_title(
            ui,
            st,
            "AI clients",
            "Let Claude Code (or another MCP client on this computer) read your traffic and set up mocks, rewrites and breakpoints. Secrets are redacted before anything leaves the core.",
        );
        let Some(m) = self.mcp.clone() else {
            group(ui, st, |ui| {
                ui.horizontal(|ui| {
                    widgets::spinner(ui, st, 16.0);
                    ui.add(
                        egui::Label::new(job("Waiting for the core…", Role::Body, st.p.text2))
                            .selectable(false),
                    );
                });
            });
            return;
        };
        if !m.enabled {
            group(ui, st, |ui| {
                row_plain(
                    ui,
                    st,
                    "The AI server is not running",
                    "Its port may be taken. Start the core with --mcp-listen to choose another address.",
                );
            });
            return;
        }
        group(ui, st, |ui| {
            row(
                ui,
                st,
                "Changes from AI clients",
                match m.writes {
                    1 => "Applied immediately. Only choose this for clients you trust.",
                    2 => "Read only: clients can look but not change anything.",
                    _ => "Each change waits for you to allow it here.",
                },
                |ui| {
                    let sel = match m.writes {
                        1 => 1,
                        2 => 2,
                        _ => 0,
                    };
                    if let Some(i) = widgets::segmented(
                        ui,
                        st,
                        self.id("mcp-writes"),
                        &["Ask me", "Allow", "Read only"],
                        sel,
                    ) {
                        self.set_mcp_writes(match i {
                            1 => pb::McpWrites::McpAuto,
                            2 => pb::McpWrites::McpOff,
                            _ => pb::McpWrites::McpAsk,
                        });
                    }
                },
            );
            divider(ui, st);
            row(ui, st, "Endpoint", &m.url, |ui| {
                if widgets::button(ui, st, "Copy", Kind::Plain, Some(Icon::Copy), true).clicked() {
                    ui.ctx().copy_text(m.url.clone());
                    self.toast(ui.ctx(), "Endpoint copied", true);
                }
            });
            divider(ui, st);
            row(
                ui,
                st,
                "Connect Claude Code",
                "Run this once in a terminal. It includes the access token.",
                |ui| {
                    if widgets::button(ui, st, "Copy command", Kind::Primary, Some(Icon::Terminal), true)
                        .clicked()
                    {
                        ui.ctx().copy_text(m.command.clone());
                        self.toast(ui.ctx(), "Command copied", true);
                    }
                },
            );
            ui.add_space(6.0);
            let shown = if self.show_token {
                m.command.clone()
            } else {
                m.command.replace(&m.token, "••••••••••••••••")
            };
            code(ui, st, &shown);
            ui.add_space(6.0);
            let label = if self.show_token { "Hide token" } else { "Show token" };
            if widgets::button(ui, st, label, Kind::Plain, None, true).clicked() {
                self.show_token = !self.show_token;
            }
        });
    }

    fn settings_about(&mut self, ui: &mut Ui, st: &Style) {
        group_title(ui, st, "Shortcuts", "");
        group(ui, st, |ui| {
            let items = [
                ("Focus the filter", "/  or  Ctrl+F"),
                ("Pause or resume capture", "Ctrl+R"),
                ("Clear all flows", "Ctrl+K"),
                ("Copy selected flow as cURL", "Ctrl+Shift+C"),
                ("Move through flows", "↑  ↓  PgUp  PgDn  Home  End"),
                ("Open settings", "Ctrl+,"),
            ];
            for (i, (a, b)) in items.iter().enumerate() {
                if i > 0 {
                    ui.add_space(6.0);
                }
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(job(a, Role::Body, st.p.text)).selectable(false));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(job(b, Role::MonoSmall, st.p.text2)).selectable(false),
                        );
                    });
                });
            }
        });
        group_title(ui, st, "About", "");
        group(ui, st, |ui| {
            let (version, pid, up) = match &self.status {
                Some(s) => (s.version.clone(), s.core_pid, s.uptime_ms / 1000),
                None => (String::from("—"), 0, 0),
            };
            row_plain(
                ui,
                st,
                "Core",
                &format!("nekod {version}  ·  pid {pid}  ·  up {up} s"),
            );
            divider(ui, st);
            row(ui, st, "Proxy address", &self.proxy_addr(), |ui| {
                if widgets::button(ui, st, "Copy", Kind::Plain, Some(Icon::Copy), true).clicked() {
                    ui.ctx().copy_text(self.proxy_addr());
                    self.toast(ui.ctx(), "Proxy address copied", true);
                }
            });
        });
        let _ = vec2(0.0, 0.0);
    }
}
