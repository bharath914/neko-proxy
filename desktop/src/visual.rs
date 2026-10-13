//! Headless renders of the real UI on fixture data. Set `PROXY_SHOT_DIR` to
//! write PNGs; without it these tests only check that every screen draws.

use crate::app::{App, Section, Tab};
use crate::backend::DemoBackend;
use crate::platform::{Prefs, SystemAppearance};
use egui_kittest::Harness;

fn harness(dark: bool, size: (f32, f32), flows: usize) -> Harness<'static, Option<App>> {
    Harness::builder()
        .with_size(egui::vec2(size.0, size.1))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            move |ui, state: &mut Option<App>| {
                let ctx = ui.ctx().clone();
                if state.is_none()
                    && !ctx.data(|d| d.get_temp::<bool>(egui::Id::new("fonts")).unwrap_or(false))
                {
                    // Fonts take effect on the next pass, so draw nothing yet.
                    crate::style::install_fonts(&ctx);
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("fonts"), true));
                    ctx.request_repaint();
                    return;
                }
                let app = state.get_or_insert_with(|| {
                    App::new(
                        &ctx,
                        Box::new(DemoBackend::new(flows, false)),
                        SystemAppearance::fixed(dark),
                        Prefs::default(),
                    )
                });
                app.frame(ui);
            },
            None,
        )
}

fn save(h: &mut Harness<'static, Option<App>>, name: &str) {
    let Ok(dir) = std::env::var("PROXY_SHOT_DIR") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let img = h.render().expect("render");
    img.save(format!("{dir}/{name}.png")).unwrap();
}

fn settle(h: &mut Harness<'static, Option<App>>) {
    for _ in 0..12 {
        h.step();
    }
    // Let springs and page fetches finish.
    h.run_steps(30);
}

fn app<'a>(h: &'a mut Harness<'static, Option<App>>) -> &'a mut App {
    h.state_mut().as_mut().unwrap()
}

#[test]
fn inspector_light_and_dark() {
    for (dark, name) in [(false, "inspector-light"), (true, "inspector-dark")] {
        let mut h = harness(dark, (1320.0, 820.0), 240);
        settle(&mut h);
        assert!(app(&mut h).table.total() >= 240);
        app(&mut h).select(Some(214));
        settle(&mut h);
        save(&mut h, name);
        assert!(app(&mut h).details.contains_key(&214));
    }
}

#[test]
fn detail_tabs() {
    let mut h = harness(false, (1320.0, 820.0), 60);
    settle(&mut h);
    app(&mut h).select(Some(1));
    for (tab, name) in [
        (Tab::Request, "tab-request"),
        (Tab::Response, "tab-response"),
        (Tab::Timing, "tab-timing"),
    ] {
        app(&mut h).tab = tab;
        settle(&mut h);
        save(&mut h, name);
    }
    // Pinned flow explains itself and links to settings.
    app(&mut h).tab = Tab::Overview;
    app(&mut h).select(Some(10));
    settle(&mut h);
    save(&mut h, "overview-pinned");
}

#[test]
fn body_viewers() {
    // Flow 3 is a long nested JSON list, flow 12 an HTML page.
    for (dark, tag) in [(false, "light"), (true, "dark")] {
        let mut h = harness(dark, (1320.0, 820.0), 60);
        settle(&mut h);
        app(&mut h).select(Some(3));
        app(&mut h).tab = Tab::Response;
        app(&mut h).headers_open = false;
        settle(&mut h);
        save(&mut h, &format!("json-{tag}"));
        assert!(app(&mut h).details.contains_key(&3));

        app(&mut h).headers_open = true;
        settle(&mut h);
        save(&mut h, &format!("json-headers-{tag}"));

        app(&mut h).select(Some(12));
        app(&mut h).headers_open = false;
        settle(&mut h);
        save(&mut h, &format!("html-preview-{tag}"));
        app(&mut h).html_source = true;
        settle(&mut h);
        save(&mut h, &format!("html-source-{tag}"));
        app(&mut h).html_source = false;
    }
}

#[test]
fn settings_screen() {
    let mut h = harness(false, (1100.0, 900.0), 20);
    settle(&mut h);
    app(&mut h).section = Section::Settings;
    settle(&mut h);
    save(&mut h, "settings-light");
    let mut h = harness(true, (1100.0, 900.0), 20);
    settle(&mut h);
    app(&mut h).section = Section::Settings;
    settle(&mut h);
    save(&mut h, "settings-dark");
}

#[test]
fn empty_and_filtered_states() {
    let mut h = harness(false, (1100.0, 700.0), 0);
    settle(&mut h);
    save(&mut h, "empty");
    let mut h = harness(false, (1100.0, 700.0), 80);
    settle(&mut h);
    app(&mut h).search = "nomatchhere".into();
    app(&mut h).filter_changed();
    settle(&mut h);
    save(&mut h, "no-match");
}
