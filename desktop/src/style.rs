//! egui global style from the design tokens, and font setup.

use crate::theme::{Palette, Style};
use egui::{
    epaint::Shadow, Color32, Context, FontData, FontDefinitions, FontFamily, Stroke, Visuals,
};
use std::sync::Arc;

/// Apple's San Francisco, when the person has installed it. It cannot ship
/// with the app (Apple's license limits it to Apple platforms), so it is
/// picked up from the usual font folders, or from `PROXY_FONT_DIR`.
struct SystemFonts {
    text: [Vec<u8>; 3],
    mono: Option<[Vec<u8>; 2]>,
}

fn font_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Some(d) = std::env::var_os("PROXY_FONT_DIR") {
        dirs.push(d.into());
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::PathBuf::from(home);
        dirs.push(home.join(".local/share/fonts"));
        dirs.push(home.join(".fonts"));
    }
    dirs.push("/usr/local/share/fonts".into());
    dirs.push("/usr/share/fonts".into());
    dirs
}

fn collect_fonts(dir: &std::path::Path, depth: u32, out: &mut Vec<(String, std::path::PathBuf)>) {
    if depth > 4 || out.len() > 20_000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let path = e.path();
        if path.is_dir() {
            collect_fonts(&path, depth + 1, out);
            continue;
        }
        let ext = path
            .extension()
            .and_then(|x| x.to_str())
            .map(str::to_ascii_lowercase);
        if !matches!(ext.as_deref(), Some("otf") | Some("ttf")) {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|x| x.to_str()) {
            let norm: String = stem
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase();
            if norm.starts_with("sf") {
                out.push((norm, path));
            }
        }
    }
}

fn system_fonts() -> Option<SystemFonts> {
    let mut files = Vec::new();
    for d in font_dirs() {
        collect_fonts(&d, 0, &mut files);
    }
    let read = |names: &[&str]| -> Option<Vec<u8>> {
        names.iter().find_map(|n| {
            files
                .iter()
                .find(|(norm, _)| norm == n)
                .and_then(|(_, p)| std::fs::read(p).ok())
        })
    };
    let text = [
        read(&["sfprotextregular", "sfproregular", "sfprodisplayregular"])?,
        read(&["sfprotextmedium", "sfpromedium", "sfprodisplaymedium"])?,
        read(&["sfprotextsemibold", "sfprosemibold", "sfprodisplaysemibold"])?,
    ];
    let mono = read(&["sfmonoregular"]).and_then(|r| {
        read(&["sfmonomedium"]).map(|m| [r, m])
    });
    Some(SystemFonts { text, mono })
}

pub fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::empty();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add(
        "Inter-Regular",
        include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    );
    add(
        "Inter-Medium",
        include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    );
    add(
        "Inter-SemiBold",
        include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    );
    add(
        "JB-Regular",
        include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    );
    add(
        "JB-Medium",
        include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf"),
    );

    // Prefer San Francisco when installed; Inter and JetBrains Mono cover
    // everything else, including glyphs the system face lacks.
    let (mut reg, mut med, mut semi) = (vec![], vec![], vec![]);
    let (mut mono, mut mono_med) = (vec![], vec![]);
    if let Some(sf) = system_fonts() {
        let [r, m, sb] = sf.text;
        fonts
            .font_data
            .insert("SF-Regular".into(), Arc::new(FontData::from_owned(r)));
        fonts
            .font_data
            .insert("SF-Medium".into(), Arc::new(FontData::from_owned(m)));
        fonts
            .font_data
            .insert("SF-Semibold".into(), Arc::new(FontData::from_owned(sb)));
        reg.push("SF-Regular".to_string());
        med.push("SF-Medium".to_string());
        semi.push("SF-Semibold".to_string());
        if let Some([r, m]) = sf.mono {
            fonts
                .font_data
                .insert("SFMono-Regular".into(), Arc::new(FontData::from_owned(r)));
            fonts
                .font_data
                .insert("SFMono-Medium".into(), Arc::new(FontData::from_owned(m)));
            mono.push("SFMono-Regular".to_string());
            mono_med.push("SFMono-Medium".to_string());
        }
    }
    let tail = |v: Vec<String>, rest: &[&str]| -> Vec<String> {
        v.into_iter()
            .chain(rest.iter().map(|s| s.to_string()))
            .collect()
    };
    fonts.families.insert(
        FontFamily::Proportional,
        tail(reg, &["Inter-Regular", "JB-Regular"]),
    );
    fonts.families.insert(
        FontFamily::Monospace,
        tail(mono, &["JB-Regular", "Inter-Regular"]),
    );
    fonts.families.insert(
        FontFamily::Name("medium".into()),
        tail(med, &["Inter-Medium", "Inter-Regular"]),
    );
    fonts.families.insert(
        FontFamily::Name("semibold".into()),
        tail(semi, &["Inter-SemiBold", "Inter-Regular"]),
    );
    fonts.families.insert(
        FontFamily::Name("mono-medium".into()),
        tail(mono_med, &["JB-Medium", "JB-Regular"]),
    );
    ctx.set_fonts(fonts);
}

/// Apply palette-driven visuals. Widgets are custom-painted, so this mostly
/// governs text editing, scroll bars and tooltips.
pub fn apply(ctx: &Context, st: &Style) {
    let p: &Palette = &st.p;
    let mut v = if p.is_dark() {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.override_text_color = Some(p.text);
    v.window_fill = p.card;
    v.panel_fill = p.content;
    v.extreme_bg_color = p.code;
    v.faint_bg_color = p.row_alt;
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.hyperlink_color = p.accent;
    v.window_stroke = Stroke::new(1.0, p.separator);
    v.window_shadow = Shadow {
        offset: [0, 10],
        blur: 30,
        spread: 0,
        color: Color32::from_black_alpha(80),
    };
    v.popup_shadow = Shadow {
        offset: [0, 6],
        blur: 20,
        spread: 0,
        color: Color32::from_black_alpha(60),
    };
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.separator);
    v.widgets.inactive.bg_fill = Color32::TRANSPARENT;
    v.widgets.hovered.bg_fill = Color32::TRANSPARENT;
    v.widgets.active.bg_fill = Color32::TRANSPARENT;
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.hovered.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.active.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.active.bg_stroke = Stroke::NONE;
    ctx.set_visuals(v);

    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.scroll.bar_width = 8.0;
        s.spacing.scroll.floating = true;
        s.spacing.scroll.floating_width = 4.0;
        s.spacing.scroll.floating_allocated_width = 0.0;
        s.spacing.scroll.foreground_color = false;
        s.interaction.tooltip_delay = 0.35;
        s.animation_time = 0.0;
        s.text_styles
            .insert(egui::TextStyle::Body, crate::theme::Role::Body.font());
        s.text_styles
            .insert(egui::TextStyle::Monospace, crate::theme::Role::Mono.font());
        s.text_styles.insert(
            egui::TextStyle::Button,
            crate::theme::Role::BodyMedium.font(),
        );
    });
}
