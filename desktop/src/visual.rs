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

#[test]
fn breakpoints_rules_and_ai_screens() {
    for (dark, tag) in [(false, "light"), (true, "dark")] {
        let mut h = harness(dark, (1320.0, 820.0), 40);
        settle(&mut h);
        assert_eq!(app(&mut h).paused.len(), 1, "demo core holds one message");
        assert_eq!(app(&mut h).approvals.len(), 1);
        assert_eq!(app(&mut h).rules.rules.len(), 3);

        app(&mut h).section = Section::Breakpoints;
        settle(&mut h);
        save(&mut h, &format!("breakpoints-{tag}"));
        assert!(app(&mut h).bp_edit.is_some(), "the held message is open in the editor");

        app(&mut h).section = Section::Rules;
        settle(&mut h);
        save(&mut h, &format!("rules-{tag}"));

        app(&mut h).rule_form = Some(crate::edit::RuleForm::new(crate::edit::RuleKind::Mock));
        settle(&mut h);
        save(&mut h, &format!("rules-editor-{tag}"));

    }
    // The AI group sits below the fold of a normal window; render it tall.
    let mut h = harness(false, (1100.0, 2100.0), 10);
    settle(&mut h);
    app(&mut h).section = Section::Settings;
    settle(&mut h);
    save(&mut h, "settings-ai-tall");
}

#[test]
fn resuming_and_deciding_clear_the_queues() {
    let mut h = harness(false, (1320.0, 820.0), 10);
    settle(&mut h);
    let id = app(&mut h).paused[0].flow_id;
    app(&mut h).select_held(Some(id));
    app(&mut h).bp_edit.as_mut().unwrap().path = "/v1/changed".into();
    app(&mut h).resume_held(id);
    let aid = app(&mut h).approvals[0].id.clone();
    app(&mut h).decide_approval(&aid, true);
    settle(&mut h);
    assert!(app(&mut h).paused.is_empty());
    assert!(app(&mut h).approvals.is_empty());
    app(&mut h).section = Section::Breakpoints;
    settle(&mut h);
    save(&mut h, "breakpoints-empty");
}

#[test]
fn devices_screen() {
    for (dark, tag) in [(false, "light"), (true, "dark")] {
        let mut h = harness(dark, (1320.0, 900.0), 40);
        settle(&mut h);
        assert_eq!(app(&mut h).devices.devices.len(), 4);
        assert_eq!(app(&mut h).devices.pending.len(), 1);
        app(&mut h).section = Section::Devices;
        settle(&mut h);
        save(&mut h, &format!("devices-{tag}"));

        // Settings drawer and the hints panel.
        let d = app(&mut h).devices.devices[0].clone();
        app(&mut h).device_drawer = Some(crate::edit::DeviceForm::new(&d));
        app(&mut h).hints_open = true;
        app(&mut h).device_drawer.as_mut().unwrap().redaction = 2;
        settle(&mut h);
        save(&mut h, &format!("devices-drawer-{tag}"));
    }
}

#[test]
fn devices_empty_state_and_actions() {
    let mut h = harness(false, (1100.0, 800.0), 5);
    settle(&mut h);
    let id = app(&mut h).devices.pending[0].id;
    app(&mut h).allow_prompt(id);
    app(&mut h).revoke_device("emu-1", "com.demo.shop");
    settle(&mut h);
    assert!(app(&mut h).devices.pending.is_empty());
    assert_eq!(app(&mut h).devices.devices.len(), 5);
    let revoked = app(&mut h)
        .devices
        .devices
        .iter()
        .find(|d| d.device_id == "emu-1")
        .unwrap()
        .status;
    assert_eq!(revoked, proto::pb::DeviceStatus::DeviceRevoked as i32);
    for dev in ["pixel-8", "emu-1", "iphone-15", "galaxy-s23", "tab-a9"] {
        app(&mut h).forget_device(dev);
    }
    app(&mut h).section = Section::Devices;
    settle(&mut h);
    assert!(app(&mut h).devices.devices.is_empty());
    save(&mut h, "devices-empty");
}

#[test]
fn sdk_flows_in_the_inspector() {
    for (dark, tag) in [(false, "light"), (true, "dark")] {
        let mut h = harness(dark, (1480.0, 860.0), 60);
        settle(&mut h);
        app(&mut h).col_screen = true;
        // Flow 4 was seen by the SDK and the proxy; 2 is SDK only, with a lazy body.
        app(&mut h).select(Some(4));
        settle(&mut h);
        save(&mut h, &format!("sdk-merged-app-{tag}"));
        assert!(app(&mut h).details.get(&4).unwrap().wire.is_some());
        app(&mut h).tab = Tab::Request;
        settle(&mut h);
        save(&mut h, &format!("sdk-request-redacted-{tag}"));
        app(&mut h).tab = Tab::Timing;
        settle(&mut h);
        save(&mut h, &format!("sdk-timing-both-{tag}"));
        app(&mut h).tab = Tab::Overview;
        app(&mut h).wire_view = true;
        settle(&mut h);
        save(&mut h, &format!("sdk-merged-wire-{tag}"));
        app(&mut h).wire_view = false;
    }
}

#[test]
fn lazy_bodies_load_on_demand() {
    let mut h = harness(false, (1480.0, 860.0), 60);
    settle(&mut h);
    app(&mut h).select(Some(2));
    app(&mut h).tab = Tab::Response;
    app(&mut h).headers_open = false;
    settle(&mut h);
    // Opening the tab asked the device for the body; the fixture answers.
    let c = app(&mut h).details.get(&2).unwrap().detail.sdk.clone().unwrap();
    assert_eq!(c.response_body_state, proto::pb::BodyState::BodyPresent as i32);
    assert!(app(&mut h).fetching.is_empty());
    save(&mut h, "sdk-lazy-loaded");
}

#[test]
fn device_chips_narrow_the_list() {
    let mut h = harness(false, (1480.0, 860.0), 60);
    settle(&mut h);
    let before = app(&mut h).table.total();
    app(&mut h).set_device_filter(vec!["pixel-8".into()], vec![]);
    settle(&mut h);
    let after = app(&mut h).table.total();
    assert!(after > 0 && after < before, "{after} of {before}");
    save(&mut h, "device-chip-filter");
    app(&mut h).set_device_filter(vec![], vec![]);
    settle(&mut h);
    assert_eq!(app(&mut h).table.total(), before);
}

#[test]
fn ten_thousand_flows_scroll_and_filter_smoothly() {
    let mut h = harness(false, (1480.0, 860.0), 10_000);
    settle(&mut h);
    assert_eq!(app(&mut h).table.total(), 10_000);
    // Scroll through the list: only visible rows are laid out, so a frame
    // costs the same at row 10 and row 9,000.
    let mut worst = std::time::Duration::ZERO;
    let t0 = std::time::Instant::now();
    for i in 0..120 {
        app(&mut h).scroll_to = Some(i as f32 * 80.0 * 30.0 / 30.0);
        app(&mut h).follow = false;
        let t = std::time::Instant::now();
        h.step();
        worst = worst.max(t.elapsed());
    }
    let avg = t0.elapsed() / 120;
    eprintln!("10,000 rows: avg frame {avg:?}, worst {worst:?}");
    assert!(avg < std::time::Duration::from_millis(16), "avg frame {avg:?}");

    // Narrowing by device is one request to the core; the table is ready
    // within a few frames.
    let t = std::time::Instant::now();
    app(&mut h).set_device_filter(vec!["pixel-8".into()], vec![]);
    for _ in 0..6 {
        h.step();
    }
    assert!(app(&mut h).table.total() < 10_000);
    eprintln!("device filter applied in {:?}", t.elapsed());
    assert!(t.elapsed() < std::time::Duration::from_millis(100));
}
