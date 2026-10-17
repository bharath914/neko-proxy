//! Devices: who may send this app their traffic. The toggle that opens the
//! SDK listener, pairing requests waiting for a person, the paired devices
//! and their settings, and how to connect each kind of device.

use super::icons::Icon;
use super::widgets::{self, Kind};
use crate::app::{App, Section, Sheet};
use crate::edit::DeviceForm;
use crate::fmt;
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

fn cut(ui: &mut Ui, s: &str, role: Role, color: Color32) {
    ui.add(
        egui::Label::new(RichText::new(s).font(role.font()).color(color))
            .truncate()
            .selectable(false),
    );
}

fn connection_label(c: &str) -> &'static str {
    match c {
        "emulator" => "Emulator",
        "usb" => "USB",
        "wifi" => "Wi-Fi",
        "simulator" => "Simulator",
        _ => "—",
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn chip(ui: &mut Ui, st: &Style, label: &str, ok: Option<bool>) {
    let col = match ok {
        Some(true) => st.p.green,
        Some(false) => st.p.orange,
        None => st.p.text3,
    };
    widgets::badge(ui, label, col, paint::with_alpha(col, 0.14));
}

fn card<R>(ui: &mut Ui, st: &Style, accent: bool, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(st.p.card)
        .stroke(Stroke::new(
            1.0,
            if accent { st.p.accent } else { st.p.separator },
        ))
        .corner_radius(cr(radius::CARD))
        .inner_margin(Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

fn divider(ui: &mut Ui, st: &Style) {
    ui.add_space(6.0);
    let (l, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(l, 0.0, st.p.separator);
    ui.add_space(6.0);
}

/// A fixed-width cell, so headers and rows line up whatever they hold.
fn cell(ui: &mut Ui, w: f32, h: f32, add: impl FnOnce(&mut Ui)) {
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), egui::Sense::hover());
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    add(&mut child);
}

enum Act {
    Allow(u64),
    Deny(u64),
    Revoke(String, String),
    Forget(String),
    Settings(pb::Device),
}

impl App {
    pub fn devices_screen(&mut self, ui: &mut Ui) {
        let st = self.st.clone();
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, st.p.grouped);

        let col_w = (full.width() - 64.0).clamp(360.0, 980.0);
        let left = full.center().x - col_w / 2.0;
        let rect = Rect::from_min_max(pos2(left, full.top()), pos2(left + col_w, full.bottom()));
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
        child.set_clip_rect(full.intersect(ui.clip_rect()));

        // Expiry countdowns and "last seen" need a tick.
        if !self.devices.pending.is_empty() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(500));
        }

        let mut act: Option<Act> = None;
        ScrollArea::vertical()
            .id_salt("devices")
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                ui.set_width(col_w);
                ui.add_space(26.0);
                self.devices_header(ui, &st);
                ui.add_space(14.0);

                let available = self.sdk.as_ref().is_some_and(|s| s.available);
                if self.sdk.is_some() && !available {
                    card(ui, &st, false, |ui| {
                        text(
                            ui,
                            "This core runs without the SDK listener",
                            Role::Headline,
                            st.p.text,
                        );
                        ui.add_space(2.0);
                        text(
                            ui,
                            "Start proxyd without --no-sdk to accept connections from apps.",
                            Role::Callout,
                            st.p.text2,
                        );
                    });
                    return;
                }

                if !self.devices.pending.is_empty() {
                    for p in self.devices.pending.clone() {
                        self.pending_card(ui, &st, &p, &mut act);
                        ui.add_space(10.0);
                    }
                }

                if self.devices.devices.is_empty() {
                    self.devices_empty(ui, &st);
                } else {
                    self.device_table(ui, &st, &mut act);
                }
                ui.add_space(16.0);
                self.hints(ui, &st);
                ui.add_space(40.0);
            });

        match act {
            Some(Act::Allow(id)) => self.allow_prompt(id),
            Some(Act::Deny(id)) => self.deny_prompt(id),
            Some(Act::Revoke(d, a)) => self.revoke_device(&d, &a),
            Some(Act::Forget(d)) => self.forget_device(&d),
            Some(Act::Settings(d)) => {
                let open = self
                    .device_drawer
                    .as_ref()
                    .is_some_and(|f| f.device_id == d.device_id && f.app_id == d.app_id);
                self.device_drawer = if open {
                    None
                } else {
                    Some(DeviceForm::new(&d))
                };
            }
            None => {}
        }
    }

    fn devices_header(&mut self, ui: &mut Ui, st: &Style) {
        let enabled = self.sdk.as_ref().is_some_and(|s| s.enabled);
        ui.horizontal(|ui| {
            text(ui, "Devices", Role::LargeTitle, st.p.text);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut on = enabled;
                if widgets::toggle(ui, st, &mut on).changed() {
                    self.set_sdk_enabled(on);
                }
                ui.add_space(6.0);
                text(ui, "Accept SDK connections", Role::BodyMedium, st.p.text);
            });
        });
        ui.add_space(4.0);
        let Some(s) = self.sdk.clone() else {
            text(ui, "Connecting to the core…", Role::Callout, st.p.text2);
            return;
        };
        if s.enabled {
            // The two choices that decide who gets in without a prompt and who can reach this computer at all.
            ui.horizontal(|ui| {
                let (mut local, mut lan) = (s.auto_accept_local, s.lan);
                let mut changed = false;
                changed |= widgets::toggle(ui, st, &mut local).changed();
                text(
                    ui,
                    "Accept emulators, simulators and USB phones automatically",
                    Role::Callout,
                    st.p.text,
                );
                ui.add_space(14.0);
                changed |= widgets::toggle(ui, st, &mut lan).changed();
                text(ui, "Phones on Wi-Fi", Role::Callout, st.p.text);
                if changed {
                    self.set_sdk_settings(lan, local);
                }
            });
            ui.add_space(4.0);
        }
        ui.horizontal_wrapped(|ui| {
            if s.enabled {
                let addr = s
                    .listening
                    .iter()
                    .find(|l| !l.starts_with("127.") && !l.starts_with("172."))
                    .or(s.listening.first())
                    .cloned()
                    .unwrap_or_default();
                let fp: String = s.fingerprint.chars().take(12).collect();
                text(
                    ui,
                    &format!("Listening on {addr}  ·  certificate {fp}…"),
                    Role::Callout,
                    st.p.text2,
                );
            } else {
                text(
                    ui,
                    "Off. Apps with the SDK cannot connect. Turn it on to pair a device.",
                    Role::Callout,
                    st.p.text2,
                );
            }
            ui.add_space(6.0);
            let adb_ok = match s.adb_state.as_str() {
                "ready" => Some(true),
                "unavailable" | "error" => Some(false),
                _ => None,
            };
            chip(ui, st, &s.adb_label, if s.enabled { adb_ok } else { None });
            if s.enabled {
                chip(
                    ui,
                    st,
                    if s.discovery_state == "advertising" {
                        "Wi-Fi discovery on"
                    } else {
                        "Wi-Fi discovery unavailable"
                    },
                    Some(s.discovery_state == "advertising"),
                );
            }
        });
    }

    fn pending_card(
        &mut self,
        ui: &mut Ui,
        st: &Style,
        p: &pb::PendingPrompt,
        act: &mut Option<Act>,
    ) {
        card(ui, st, true, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        widgets::badge(
                            ui,
                            if p.repair {
                                "Pair again"
                            } else {
                                "Pairing request"
                            },
                            st.p.accent,
                            st.p.accent_soft,
                        );
                        text(
                            ui,
                            &format!("{}  ·  {}", p.device_name, p.device_model),
                            Role::Headline,
                            st.p.text,
                        );
                    });
                    ui.add_space(2.0);
                    text(
                        ui,
                        &format!(
                            "{}  ·  {} {}  ·  {}  ·  {}",
                            p.app_id,
                            p.sdk_name,
                            p.sdk_version,
                            connection_label(&p.connection),
                            p.os_version
                        ),
                        Role::Callout,
                        st.p.text2,
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(fmt::pair_code(&p.code))
                                .font(egui::FontId::monospace(30.0))
                                .color(st.p.text),
                        )
                        .selectable(true),
                    );
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if widgets::button(ui, st, "Allow", Kind::Primary, Some(Icon::Check), !p.code.is_empty())
                    .clicked()
                {
                    *act = Some(Act::Allow(p.id));
                }
                if widgets::button(ui, st, "Deny", Kind::Secondary, None, true).clicked() {
                    *act = Some(Act::Deny(p.id));
                }
                ui.add_space(6.0);
                let left = ((p.expires_ms - now_ms()) / 1000).clamp(0, 3600);
                text(
                    ui,
                    &format!(
                        "Confirm the same code appears in the app's log or banner.  Expires in {left} s."
                    ),
                    Role::Callout,
                    st.p.text2,
                );
            });
        });
    }

    fn device_table(&mut self, ui: &mut Ui, st: &Style, act: &mut Option<Act>) {
        text(ui, "Paired devices", Role::Headline, st.p.text);
        ui.add_space(8.0);
        let now = now_ms();
        let devices = self.devices.devices.clone();
        card(ui, st, false, |ui| {
            let w = ui.available_width();
            let widths = [0.22, 0.21, 0.10, 0.10, 0.12, 0.13].map(|f| (w - 112.0) * f);
            ui.horizontal(|ui| {
                for (label, cw) in ["Device", "App", "SDK", "Connection", "Status", "Last seen"]
                    .iter()
                    .zip(widths)
                {
                    cell(ui, cw, 16.0, |ui| {
                        text(ui, label, Role::Caption, st.p.text3)
                    });
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(112.0 - 16.0);
                    text(ui, "Flows", Role::Caption, st.p.text3);
                });
            });
            for d in &devices {
                divider(ui, st);
                ui.horizontal(|ui| {
                    cell(ui, widths[0], 36.0, |ui| {
                        ui.set_max_width(widths[0] - 8.0);
                        cut(ui, &d.device_name, Role::BodyMedium, st.p.text);
                        cut(
                            ui,
                            &format!("{} · {}", d.device_model, d.os_version),
                            Role::Caption,
                            st.p.text3,
                        );
                    });
                    cell(ui, widths[1], 36.0, |ui| {
                        ui.set_max_width(widths[1] - 8.0);
                        let name = if d.app_name.is_empty() {
                            &d.app_id
                        } else {
                            &d.app_name
                        };
                        cut(ui, name, Role::BodyMedium, st.p.text);
                        cut(
                            ui,
                            &format!("{} · {}", d.app_id, d.app_version),
                            Role::Caption,
                            st.p.text3,
                        );
                    });
                    cell(ui, widths[2], 36.0, |ui| {
                        text(ui, &d.sdk_name, Role::Body, st.p.text);
                        text(ui, &d.sdk_version, Role::Caption, st.p.text3);
                    });
                    cell(ui, widths[3], 36.0, |ui| {
                        text(ui, connection_label(&d.connection), Role::Body, st.p.text);
                    });
                    cell(ui, widths[4], 36.0, |ui| {
                        ui.horizontal(|ui| {
                            let (label, col) =
                                match pb::DeviceStatus::try_from(d.status).unwrap_or_default() {
                                    pb::DeviceStatus::DeviceConnected => ("Connected", st.p.green),
                                    pb::DeviceStatus::DeviceIdle => ("Idle", st.p.text3),
                                    pb::DeviceStatus::DeviceDenied => ("Denied", st.p.red),
                                    pb::DeviceStatus::DeviceRevoked => ("Revoked", st.p.orange),
                                };
                            let (r, _) =
                                ui.allocate_exact_size(vec2(8.0, 18.0), egui::Sense::hover());
                            ui.painter().circle_filled(r.center(), 4.0, col);
                            text(ui, label, Role::Body, st.p.text);
                        });
                    });
                    cell(ui, widths[5], 36.0, |ui| {
                        let connected = d.status == pb::DeviceStatus::DeviceConnected as i32;
                        text(
                            ui,
                            &if connected {
                                "now".to_string()
                            } else {
                                fmt::ago(d.last_seen_ms, now)
                            },
                            Role::Body,
                            st.p.text2,
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let gone = d.status == pb::DeviceStatus::DeviceDenied as i32
                            || d.status == pb::DeviceStatus::DeviceRevoked as i32;
                        if gone {
                            if widgets::icon_button(
                                ui,
                                st,
                                Icon::Trash,
                                "Forget this device",
                                false,
                                true,
                            )
                            .clicked()
                            {
                                *act = Some(Act::Forget(d.device_id.clone()));
                            }
                        } else {
                            if widgets::icon_button(
                                ui,
                                st,
                                Icon::Sliders,
                                "Settings for this device",
                                false,
                                true,
                            )
                            .clicked()
                            {
                                *act = Some(Act::Settings(d.clone()));
                            }
                            if widgets::button(ui, st, "Revoke", Kind::Destructive, None, true)
                                .clicked()
                            {
                                *act = Some(Act::Revoke(d.device_id.clone(), d.app_id.clone()));
                            }
                        }
                        ui.add_space(6.0);
                        text(ui, &fmt::count(d.flows as usize), Role::Mono, st.p.text);
                    });
                });
                let open = self
                    .device_drawer
                    .as_ref()
                    .is_some_and(|f| f.device_id == d.device_id && f.app_id == d.app_id);
                if open {
                    ui.add_space(8.0);
                    self.device_drawer_ui(ui, st);
                }
            }
        });
    }

    fn device_drawer_ui(&mut self, ui: &mut Ui, st: &Style) {
        let Some(mut f) = self.device_drawer.take() else {
            return;
        };
        let mut close = false;
        let mut save = false;
        Frame::new()
            .fill(st.p.grouped)
            .stroke(Stroke::new(1.0, st.p.separator))
            .corner_radius(cr(radius::CARD))
            .inner_margin(Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                text(ui, &format!("Settings for {}", f.title), Role::Headline, st.p.text);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    widgets::toggle(ui, st, &mut f.capture);
                    text(ui, "Capture this app's calls", Role::Body, st.p.text);
                });
                ui.add_space(8.0);
                ui.columns(2, |cols| {
                    for (i, (label, val)) in
                        [("Only these hosts (empty = all)", &mut f.allow), ("Never capture these hosts", &mut f.deny)]
                            .into_iter()
                            .enumerate()
                    {
                        text(&mut cols[i], label, Role::Caption, st.p.text2);
                        cols[i].add(
                            egui::TextEdit::multiline(val)
                                .font(Role::MonoSmall.font())
                                .hint_text("api.example.com\n*.cdn.example.com")
                                .desired_rows(3)
                                .desired_width(f32::INFINITY),
                        );
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    text(ui, "Body limit", Role::Body, st.p.text);
                    ui.add(egui::DragValue::new(&mut f.body_cap_kb).range(1..=8192).suffix(" KB"));
                    ui.add_space(14.0);
                    text(ui, "Send bodies up to", Role::Body, st.p.text);
                    ui.add(egui::DragValue::new(&mut f.eager_kb).range(0..=8192).suffix(" KB"));
                    text(ui, "right away; larger ones load when opened", Role::Callout, st.p.text2);
                });
                ui.add_space(10.0);
                text(ui, "Redaction", Role::Caption, st.p.text2);
                let sel = match f.redaction {
                    1 => 1,
                    2 => 2,
                    _ => 0,
                };
                if let Some(i) = widgets::segmented(ui, st, self.id("redaction"), &["Default", "Strict", "None"], sel) {
                    f.redaction = i as i32;
                }
                ui.add_space(4.0);
                let note = match f.redaction {
                    1 => "Strict: only a short list of headers is sent, and no bodies.",
                    2 => "None: nothing is hidden.",
                    _ => "Default: Authorization, cookies, API keys, and passwords or tokens in JSON are replaced on the device before they are sent.",
                };
                text(ui, note, Role::Callout, st.p.text2);
                if f.reveals_secrets() {
                    ui.add_space(6.0);
                    Frame::new()
                        .fill(paint::with_alpha(st.p.orange, 0.14))
                        .corner_radius(cr(radius::CONTROL))
                        .inner_margin(Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                super::icons::paint(ui.painter(), Icon::Warning, ui.cursor().min + vec2(8.0, 9.0), 15.0, st.p.orange);
                                ui.add_space(20.0);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new("Tokens, passwords and cookies from this app will appear in the inspector, in exports and in anything an AI client reads.")
                                            .font(Role::Callout.font())
                                            .color(st.p.text),
                                    )
                                    .wrap()
                                    .selectable(false),
                                );
                            });
                        });
                }
                ui.add_space(8.0);
                text(ui, "Also hide these keys (one per line)", Role::Caption, st.p.text2);
                ui.add(
                    egui::TextEdit::multiline(&mut f.extra_keys)
                        .font(Role::MonoSmall.font())
                        .hint_text("ssn\nnational_id")
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if widgets::button(ui, st, "Save", Kind::Primary, None, f.dirty()).clicked() {
                        save = true;
                    }
                    if widgets::button(ui, st, if f.dirty() { "Cancel" } else { "Close" }, Kind::Secondary, None, true)
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        if save {
            self.save_device_config(&f.device_id, &f.app_id, f.config());
            close = true;
        }
        if !close {
            self.device_drawer = Some(f);
        }
    }

    fn devices_empty(&mut self, ui: &mut Ui, st: &Style) {
        card(ui, st, false, |ui| {
            text(ui, "No devices yet", Role::Title, st.p.text);
            ui.add_space(4.0);
            text(
                ui,
                "Add the SDK to a debug build and run the app. Every call shows up in the Inspector with its screen and name. No proxy, no certificate.",
                Role::Body,
                st.p.text2,
            );
            ui.add_space(12.0);
            self.snippet(ui, st, "Android", "plugins { id(\"dev.proxy.inspector\") }");
            ui.add_space(6.0);
            self.snippet(ui, st, "iOS", "import ProxyInspector\n\n@main struct MyApp: App {\n    init() { Inspector.start() }\n}");
            ui.add_space(12.0);
            for (k, v) in [
                (
                    "Android emulator",
                    "Works as is. It reaches this computer at 10.0.2.2.",
                ),
                (
                    "Android phone on USB",
                    "Works as is when adb is installed: the port is forwarded for you.",
                ),
                (
                    "Phone on the same Wi-Fi",
                    "Found automatically. If the network blocks device to device traffic, use USB.",
                ),
                ("iOS Simulator", "Works as is."),
            ] {
                ui.horizontal(|ui| {
                    ui.allocate_ui(vec2(170.0, 18.0), |ui| {
                        text(ui, k, Role::BodyMedium, st.p.text)
                    });
                    text(ui, v, Role::Callout, st.p.text2);
                });
            }
            if !self.sdk.as_ref().is_some_and(|s| s.enabled) {
                ui.add_space(12.0);
                if widgets::button(ui, st, "Accept SDK connections", Kind::Primary, None, true)
                    .clicked()
                {
                    self.set_sdk_enabled(true);
                }
            }
        });
    }

    fn snippet(&mut self, ui: &mut Ui, st: &Style, label: &str, code: &str) {
        text(ui, label, Role::Caption, st.p.text2);
        Frame::new()
            .fill(st.p.code)
            .stroke(Stroke::new(1.0, st.p.separator))
            .corner_radius(cr(radius::CONTROL))
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(code)
                                .font(Role::MonoSmall.font())
                                .color(st.p.text),
                        )
                        .selectable(true),
                    );
                    ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                        if widgets::icon_button(ui, st, Icon::Copy, "Copy", false, true).clicked() {
                            ui.ctx().copy_text(code.to_string());
                        }
                    });
                });
            });
    }

    fn hints(&mut self, ui: &mut Ui, st: &Style) {
        let open = self.hints_open;
        let r = ui.horizontal(|ui| {
            let icon = if open {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            };
            let b = widgets::icon_button(ui, st, icon, "Connection hints", false, true);
            text(ui, "Connection hints", Role::Headline, st.p.text);
            b
        });
        if r.inner.clicked() {
            self.hints_open = !self.hints_open;
        }
        if !open {
            return;
        }
        let s = self.sdk.clone().unwrap_or_default();
        let manual: Vec<String> = s
            .lan_addresses
            .iter()
            .map(|ip| format!("{ip}:{}", s.port))
            .collect();
        card(ui, st, false, |ui| {
            for (k, v) in [
                ("Emulator", format!("Reaches this computer at 10.0.2.2:{}. Nothing to set up.", s.port)),
                ("USB", format!("With adb installed, port {} is forwarded to every attached phone. Status: {}.", s.port, s.adb_label)),
                ("Wi-Fi", "Phones find this computer by name (_proxyinspect._tcp). It must be on the same network, and the network must allow device to device traffic.".to_string()),
                ("Manual", if manual.is_empty() { "Set host:port in the SDK configuration.".to_string() } else { format!("Set host:port to {} in the SDK configuration.", manual.join(" or ")) }),
                ("Discovery fails", "Check that the phone and this computer are on the same Wi-Fi, that a guest or office network is not isolating devices, and that the firewall allows this app. USB always works for Android.".to_string()),
            ] {
                ui.horizontal_top(|ui| {
                    ui.allocate_ui(vec2(120.0, 18.0), |ui| text(ui, k, Role::BodyMedium, st.p.text));
                    ui.add(
                        egui::Label::new(RichText::new(v).font(Role::Callout.font()).color(st.p.text2))
                            .wrap()
                            .selectable(true),
                    );
                });
                ui.add_space(4.0);
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                text(ui, "Certificate", Role::BodyMedium, st.p.text);
                ui.add(
                    egui::Label::new(
                        RichText::new(&s.fingerprint)
                            .font(Role::MonoSmall.font())
                            .color(st.p.text2),
                    )
                    .selectable(true),
                );
            });
            ui.add_space(6.0);
            if widgets::button(
                ui,
                st,
                "Replace certificate…",
                Kind::Destructive,
                None,
                s.available,
            )
            .clicked()
            {
                self.open_sheet(Sheet::ConfirmRegenCert);
            }
            if let Some(stats) = &s.stats {
                if stats.dropped_events
                    + stats.dropped_chunks
                    + stats.dropped_calls
                    + stats.protocol_violations
                    > 0
                {
                    ui.add_space(6.0);
                    text(
                        ui,
                        &format!(
                            "Dropped under load: {} events, {} body chunks, {} calls. Refused: {} bad connections.",
                            stats.dropped_events, stats.dropped_chunks, stats.dropped_calls, stats.protocol_violations
                        ),
                        Role::Callout,
                        st.p.orange,
                    );
                }
            }
        });
        let _ = Section::Devices;
    }
}
