use super::icons::{self, Icon};
use super::widgets;
use crate::app::{App, Section};
use crate::backend::ConnState;
use crate::paint::{self, cr};
use crate::theme::Role;
use egui::{pos2, vec2, Align2, Rect, Sense, Ui};

impl App {
    pub fn sidebar(&mut self, ui: &mut Ui) {
        let st = self.st.clone();
        let rect = ui.max_rect();
        let p = ui.painter().clone();
        let compact = rect.width() < 120.0;
        paint::hairline(&p, &st, rect.right_top(), rect.right_bottom());

        // Hosts: every host with traffic. Focusing one narrows the list to it
        // and folds the rest away.
        self.host_list(ui, &st, rect, compact);

        // Footer: honest status, always visible.
        let fx = |r: Rect| if compact { r.center().x } else { r.left() + 14.0 };
        let mut fy = rect.bottom() - 18.0;
        paint::hairline(
            &p,
            &st,
            pos2(rect.left() + 14.0, fy - 88.0),
            pos2(rect.right() - 14.0, fy - 88.0),
        );

        // Appearance: always one click away. Auto follows the desktop.
        let sel = match self.prefs.theme.as_str() {
            "light" => 1,
            "dark" => 2,
            _ => 0,
        };
        let mut pick: Option<usize> = None;
        if compact {
            let r = Rect::from_center_size(
                pos2(rect.center().x, fy - 88.0 - 26.0),
                vec2(30.0, 28.0),
            );
            let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r));
            let icon = [Icon::Auto, Icon::Sun, Icon::Moon][sel];
            let tip = ["Appearance: Auto", "Appearance: Light", "Appearance: Dark"][sel];
            if widgets::icon_button(&mut c, &st, icon, tip, false, true).clicked() {
                pick = Some((sel + 1) % 3);
            }
        } else {
            let r = Rect::from_min_size(
                pos2(rect.left() + 10.0, fy - 88.0 - 42.0),
                vec2(rect.width() - 20.0, 28.0),
            );
            let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r));
            if let Some(i) = widgets::segmented_w(
                &mut c,
                &st,
                self.id("side-theme"),
                &["Auto", "Light", "Dark"],
                sel,
                Some(r.width()),
            ) {
                pick = Some(i);
            }
        }
        if let Some(i) = pick {
            self.prefs.theme = ["system", "light", "dark"][i].into();
            self.prefs.save();
            self.sync_style(ui.ctx());
        }

        let (cert_txt, cert_col, cert_icon) = match self.is_trusted() {
            Some(true) => ("Certificate trusted", st.p.green, Icon::Check),
            Some(false) => ("Certificate not trusted", st.p.orange, Icon::Warning),
            None => ("Certificate", st.p.text3, Icon::Shield),
        };
        let r = Rect::from_min_size(
            pos2(rect.left() + 10.0, fy - 28.0),
            vec2(rect.width() - 20.0, 28.0),
        );
        let resp = ui.interact(r, self.id("side-cert"), Sense::click());
        if resp.hovered() {
            p.rect_filled(r, cr(7), st.p.fill_hover);
        }
        icons::paint(
            &p,
            cert_icon,
            pos2(fx(r), r.center().y),
            14.0,
            cert_col,
        );
        paint::text(
            &p,
            ui,
            pos2(r.left() + 30.0, r.center().y),
            Align2::LEFT_CENTER,
            if compact { "" } else { cert_txt },
            Role::Callout,
            st.p.text2,
        );
        if resp.clicked() {
            self.section = Section::Settings;
        }
        resp.on_hover_text("Open certificate settings")
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        fy -= 30.0;

        let addr = self.proxy_addr();
        let r = Rect::from_min_size(
            pos2(rect.left() + 10.0, fy - 28.0),
            vec2(rect.width() - 20.0, 28.0),
        );
        let resp = ui.interact(r, self.id("side-addr"), Sense::click());
        if resp.hovered() {
            p.rect_filled(r, cr(7), st.p.fill_hover);
        }
        icons::paint(
            &p,
            Icon::Globe,
            pos2(fx(r), r.center().y),
            14.0,
            st.p.text2,
        );
        paint::text(
            &p,
            ui,
            pos2(r.left() + 30.0, r.center().y),
            Align2::LEFT_CENTER,
            if compact { "" } else { &addr },
            Role::MonoSmall,
            st.p.text2,
        );
        if resp.clicked() {
            ui.ctx().copy_text(addr.clone());
            self.toast(ui.ctx(), "Proxy address copied", true);
        }
        resp.on_hover_text("Copy proxy address")
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        fy -= 30.0;

        let (txt, col) = match &self.conn {
            ConnState::Connected if self.recording => ("Recording", st.p.green),
            ConnState::Connected => ("Paused", st.p.text3),
            ConnState::Starting => ("Starting core…", st.p.orange),
            ConnState::Reconnecting { .. } => ("Reconnecting…", st.p.orange),
            ConnState::Failed(_) => ("Core unavailable", st.p.red),
        };
        let r = Rect::from_min_size(
            pos2(rect.left() + 10.0, fy - 28.0),
            vec2(rect.width() - 20.0, 28.0),
        );
        p.circle_filled(
            pos2(fx(r), r.center().y),
            6.0,
            paint::with_alpha(col, 0.22),
        );
        p.circle_filled(pos2(fx(r), r.center().y), 3.5, col);
        paint::text(
            &p,
            ui,
            pos2(r.left() + 30.0, r.center().y),
            Align2::LEFT_CENTER,
            if compact { "" } else { txt },
            Role::Callout,
            st.p.text,
        );
    }

    /// The host list between the header and the footer.
    fn host_list(&mut self, ui: &mut Ui, st: &crate::theme::Style, rect: Rect, compact: bool) {
        use egui::{ScrollArea, UiBuilder};
        let p = ui.painter().clone();
        let top = rect.top();
        let bottom = rect.bottom() - 18.0 - 88.0 - 52.0;

        if !compact {
            paint::text(
                &p,
                ui,
                pos2(rect.left() + 20.0, top + 24.0),
                Align2::LEFT_CENTER,
                "HOSTS",
                Role::Overline,
                st.p.text3,
            );
            // Right side: a way out of focus, or a count of what is here.
            if !self.focus.is_empty() {
                let r = Rect::from_center_size(
                    pos2(rect.right() - 44.0, top + 24.0),
                    vec2(76.0, 22.0),
                );
                let resp = ui.interact(r, self.id("clear-focus"), Sense::click());
                if resp.hovered() {
                    p.rect_filled(r, cr(6), st.p.fill_hover);
                }
                paint::text(
                    &p,
                    ui,
                    r.center(),
                    Align2::CENTER_CENTER,
                    "Show all",
                    Role::Callout,
                    st.p.accent,
                );
                if resp.clicked() {
                    self.set_focus(Vec::new());
                }
                resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            } else if !self.hosts.is_empty() {
                paint::text(
                    &p,
                    ui,
                    pos2(rect.right() - 16.0, top + 24.0),
                    Align2::RIGHT_CENTER,
                    &self.hosts.len().to_string(),
                    Role::Caption,
                    st.p.text3,
                );
            }
        }

        enum Row {
            All,
            Host(usize),
            Others(usize),
        }
        let mut rows: Vec<Row> = vec![Row::All];
        if self.focus.is_empty() {
            rows.extend((0..self.hosts.len()).map(Row::Host));
        } else {
            // Focused hosts first, in the order they were picked; the rest
            // collapse behind one row.
            for f in &self.focus {
                if let Some(i) = self.hosts.iter().position(|h| &h.host == f) {
                    rows.push(Row::Host(i));
                }
            }
            let others: Vec<usize> = (0..self.hosts.len())
                .filter(|i| !self.focus.iter().any(|f| f == &self.hosts[*i].host))
                .collect();
            if !others.is_empty() {
                rows.push(Row::Others(others.len()));
                if self.others_open {
                    rows.extend(others.into_iter().map(Row::Host));
                }
            }
        }

        let area = Rect::from_min_max(
            pos2(rect.left(), top + 44.0),
            pos2(rect.right(), bottom.max(top + 80.0)),
        );
        let mut child = ui.new_child(UiBuilder::new().max_rect(area));
        child.set_clip_rect(area.intersect(ui.clip_rect()));
        child.spacing_mut().item_spacing.y = 0.0;

        #[derive(Clone)]
        enum Act {
            Click(String, bool),
            All,
            ToggleOthers,
            Ssl(String, bool),
            Copy(String),
        }
        let mut acts: Vec<Act> = Vec::new();
        let multi = ui.input(|i| i.modifiers.command || i.modifiers.shift);
        let total: u64 = self.hosts.iter().map(|h| h.flows).sum();
        let row_h = 34.0;

        if self.hosts.is_empty() && !compact {
            paint::text(
                &p,
                ui,
                pos2(rect.left() + 20.0, area.top() + 40.0),
                Align2::LEFT_CENTER,
                "Hosts appear as traffic arrives.",
                Role::Callout,
                st.p.text3,
            );
        }

        ScrollArea::vertical()
            .id_salt("host-list")
            .auto_shrink([false, false])
            .show_rows(&mut child, row_h, rows.len(), |ui, range| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for ri in range {
                    let (r, resp) = ui.allocate_exact_size(
                        vec2(ui.available_width(), row_h),
                        Sense::click(),
                    );
                    let cell = r.shrink2(vec2(8.0, 1.0));
                    let pa = ui.painter();
                    let (label, count, focused, dim, tip, icon, icon_col, errors, host_name) =
                        match rows[ri] {
                            Row::All => {
                                let all = self.focus.is_empty();
                                (
                                    "All traffic".to_string(),
                                    total,
                                    all,
                                    false,
                                    "Every host".to_string(),
                                    Icon::List,
                                    if all { st.p.accent } else { st.p.text2 },
                                    0,
                                    None,
                                )
                            }
                            Row::Others(n) => (
                                format!("Other hosts · {n}"),
                                0,
                                false,
                                true,
                                "Hosts outside the focus".to_string(),
                                if self.others_open {
                                    Icon::ChevronDown
                                } else {
                                    Icon::ChevronRight
                                },
                                st.p.text3,
                                0,
                                None,
                            ),
                            Row::Host(i) => {
                                let h = &self.hosts[i];
                                let focused = self.focus.iter().any(|f| f == &h.host);
                                let (icon, col, state) = if h.pinned {
                                    (Icon::Warning, st.p.orange, "Pinned: tunneled, not decrypted")
                                } else if h.tunnel_only || !h.decrypted {
                                    (Icon::Unlock, st.p.text3, "Not decrypted")
                                } else {
                                    (Icon::Lock, st.p.green, "Decrypted")
                                };
                                let tip = format!(
                                    "{}\n{} requests{} · {}",
                                    h.host,
                                    h.flows,
                                    if h.errors > 0 {
                                        format!(" · {} errors", h.errors)
                                    } else {
                                        String::new()
                                    },
                                    state
                                );
                                (
                                    h.host.clone(),
                                    h.flows,
                                    focused,
                                    !self.focus.is_empty() && !focused,
                                    tip,
                                    icon,
                                    col,
                                    h.errors,
                                    Some(h.host.clone()),
                                )
                            }
                        };

                    // Row feedback on the press frame, like every other control.
                    let pressed = resp.is_pointer_button_down_on();
                    if focused {
                        pa.rect_filled(cell, cr(8), st.p.accent_soft);
                    } else if pressed {
                        pa.rect_filled(cell, cr(8), st.p.fill_press);
                    } else if resp.hovered() {
                        pa.rect_filled(cell, cr(8), st.p.fill_hover);
                    }
                    let text_col = if dim { st.p.text2 } else { st.p.text };
                    if compact {
                        let c = cell.center();
                        let initial = host_name
                            .as_deref()
                            .and_then(|h| h.chars().find(|c| c.is_alphanumeric()))
                            .map(|c| c.to_ascii_uppercase().to_string())
                            .unwrap_or_else(|| "·".into());
                        pa.circle_filled(c, 12.0, if focused { st.p.accent } else { st.p.fill });
                        paint::text(
                            pa,
                            ui,
                            c,
                            Align2::CENTER_CENTER,
                            &initial,
                            Role::BodyMedium,
                            if focused { st.p.on_accent } else { st.p.text },
                        );
                    } else {
                        if focused {
                            pa.rect_filled(
                                Rect::from_min_size(
                                    pos2(cell.left() + 1.0, cell.center().y - 8.0),
                                    vec2(3.0, 16.0),
                                ),
                                cr(2),
                                st.p.accent,
                            );
                        }
                        icons::paint(
                            pa,
                            icon,
                            pos2(cell.left() + 20.0, cell.center().y),
                            14.0,
                            icon_col,
                        );
                        let right_w = if count > 0 { 48.0 } else { 12.0 };
                        paint::text_fit(
                            pa,
                            ui,
                            pos2(cell.left() + 38.0, cell.center().y),
                            cell.width() - 38.0 - right_w,
                            &label,
                            if focused { Role::BodyMedium } else { Role::Body },
                            text_col,
                        );
                        if count > 0 {
                            paint::text(
                                pa,
                                ui,
                                pos2(cell.right() - 10.0, cell.center().y),
                                Align2::RIGHT_CENTER,
                                &crate::fmt::count(count as usize),
                                Role::Caption,
                                st.p.text3,
                            );
                        }
                        if errors > 0 {
                            pa.circle_filled(
                                pos2(cell.right() - 10.0 - if count > 0 { 34.0 } else { 0.0 }, cell.center().y),
                                3.0,
                                st.p.red,
                            );
                        }
                    }

                    if resp.clicked() {
                        match (&rows[ri], &host_name) {
                            (Row::All, _) => acts.push(Act::All),
                            (Row::Others(_), _) => acts.push(Act::ToggleOthers),
                            (_, Some(h)) => acts.push(Act::Click(h.clone(), multi)),
                            _ => {}
                        }
                    }
                    let resp = resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand);
                    if let Some(h) = host_name {
                        let info = self.hosts.iter().find(|x| x.host == h).cloned();
                        let focused_now = focused;
                        resp.context_menu(|ui| {
                            ui.set_min_width(248.0);
                            widgets::menu_title(ui, st, &h);
                            let label = if focused_now { "Remove focus" } else { "Focus on this host" };
                            if widgets::menu_item(ui, st, Some(Icon::Focus), label, "", true).clicked() {
                                acts.push(Act::Click(h.clone(), false));
                                ui.close();
                            }
                            widgets::menu_separator(ui, st);
                            let on = info.as_ref().map(|i| i.decrypted && !i.pinned).unwrap_or(true);
                            let (ssl_label, ssl_icon) = if on {
                                ("Disable SSL proxying", Icon::Unlock)
                            } else {
                                ("Enable SSL proxying", Icon::Lock)
                            };
                            if widgets::menu_item(ui, st, Some(ssl_icon), ssl_label, "", true).clicked() {
                                acts.push(Act::Ssl(h.clone(), !on));
                                ui.close();
                            }
                            widgets::menu_separator(ui, st);
                            if widgets::menu_item(ui, st, Some(Icon::Copy), "Copy host", "", true).clicked() {
                                acts.push(Act::Copy(h.clone()));
                                ui.close();
                            }
                        });
                    }
                }
            });

        for a in acts {
            match a {
                Act::Click(h, m) => self.click_host(&h, m),
                Act::All => self.set_focus(Vec::new()),
                Act::ToggleOthers => self.others_open = !self.others_open,
                Act::Ssl(h, enable) => {
                    let ctx = ui.ctx().clone();
                    self.ssl_host(&ctx, &h, enable);
                }
                Act::Copy(h) => {
                    ui.ctx().copy_text(h);
                    let ctx = ui.ctx().clone();
                    self.toast(&ctx, "Host copied", true);
                }
            }
        }
    }
}
