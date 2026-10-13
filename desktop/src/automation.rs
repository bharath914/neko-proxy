//! Opt-in scripting for demos and visual checks. Enabled only when
//! `PROXY_AUTOMATION=<dir>` is set. The app watches `<dir>/cmd`; each line is
//! one command, and `shot <name>` writes `<dir>/<name>.png` of the window.
//!
//!     select last | select <id> | tab overview|request|response|timing
//!     section inspector|settings | search <text> | filter all|errors|slow|pinned
//!     shot <name>

use crate::app::{App, Section, Tab};
use egui::{Context, Event, ViewportCommand};
use std::path::PathBuf;
use std::time::Duration;

pub struct Automation {
    dir: PathBuf,
    /// A screenshot waits a few frames so the last command has been drawn.
    shot: Option<(String, u8)>,
}

impl Automation {
    pub fn from_env() -> Option<Automation> {
        let dir = PathBuf::from(std::env::var_os("PROXY_AUTOMATION")?);
        std::fs::create_dir_all(&dir).ok()?;
        Some(Automation { dir, shot: None })
    }
}

impl App {
    pub fn automation_tick(&mut self, ctx: &Context) {
        let Some(mut auto) = self.automation.take() else { return };
        ctx.request_repaint_after(Duration::from_millis(200));

        if let Ok(text) = std::fs::read_to_string(auto.dir.join("cmd")) {
            let _ = std::fs::remove_file(auto.dir.join("cmd"));
            for line in text.lines() {
                let words: Vec<&str> = line.split_whitespace().collect();
                match words.as_slice() {
                    ["select", "last"] => {
                        let total = self.table.total();
                        if total > 0 {
                            if let Some(id) = self.table.get(total - 1).map(|s| s.id) {
                                self.select(Some(id));
                                self.reveal_row(total - 1);
                            }
                        }
                    }
                    ["select", id] => {
                        if let Ok(id) = id.parse() {
                            self.select(Some(id));
                            if let Some(r) = self.table.row_of(id) {
                                self.reveal_row(r);
                            }
                        }
                    }
                    ["tab", t] => {
                        self.tab = match *t {
                            "request" => Tab::Request,
                            "response" => Tab::Response,
                            "timing" => Tab::Timing,
                            _ => Tab::Overview,
                        }
                    }
                    ["section", s] => {
                        self.section = if *s == "settings" { Section::Settings } else { Section::Inspector }
                    }
                    ["filter", f] => {
                        self.table.kind = match *f {
                            "errors" => 1,
                            "slow" => 2,
                            "pinned" => 3,
                            _ => 0,
                        };
                        self.filter_changed();
                    }
                    ["search", rest @ ..] => {
                        self.search = rest.join(" ");
                        self.filter_changed();
                    }
                    ["shot", name] => auto.shot = Some((name.to_string(), 8)),
                    _ => {}
                }
            }
        }

        if let Some((name, frames)) = auto.shot.take() {
            if frames > 0 {
                auto.shot = Some((name, frames - 1));
                ctx.request_repaint();
            } else {
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::new(name)));
            }
        }

        let events = ctx.input(|i| i.events.clone());
        for e in events {
            if let Event::Screenshot { image, user_data, .. } = e {
                let name = user_data
                    .data
                    .as_ref()
                    .and_then(|d| d.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "shot".into());
                let [w, h] = image.size;
                let mut raw = Vec::with_capacity(w * h * 4);
                for p in &image.pixels {
                    raw.extend_from_slice(&[p.r(), p.g(), p.b(), p.a()]);
                }
                if let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, raw) {
                    let _ = img.save(auto.dir.join(format!("{name}.png")));
                }
            }
        }
        self.automation = Some(auto);
    }
}
