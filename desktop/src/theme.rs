//! Design tokens: color, type and spacing. Light and dark follow the system;
//! high contrast hardens separators and secondary text.

use egui::{Color32, FontFamily, FontId};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scheme {
    Light,
    Dark,
}

#[derive(Clone, Debug)]
pub struct Palette {
    pub scheme: Scheme,
    pub content: Color32,
    pub sidebar: Color32,
    pub toolbar: Color32,
    pub panel: Color32,
    pub grouped: Color32,
    pub card: Color32,
    pub code: Color32,
    pub separator: Color32,
    pub text: Color32,
    pub text2: Color32,
    pub text3: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub accent_soft: Color32,
    pub fill: Color32,
    pub fill_hover: Color32,
    pub fill_press: Color32,
    pub row_alt: Color32,
    pub green: Color32,
    pub orange: Color32,
    pub red: Color32,
    pub purple: Color32,
    pub teal: Color32,
    pub pink: Color32,
    pub indigo: Color32,
    pub yellow: Color32,
    pub scrim: Color32,
    /// Code and markup colors, tuned for contrast on `code`.
    pub syn: Syntax,
}

/// Token colors for JSON, markup and plain code. Keys read first, then
/// strings, numbers and literals; punctuation stays quiet.
#[derive(Clone, Debug)]
pub struct Syntax {
    pub key: Color32,
    pub string: Color32,
    pub number: Color32,
    pub literal: Color32,
    pub punct: Color32,
    pub tag: Color32,
    pub attr: Color32,
    pub value: Color32,
    pub comment: Color32,
    pub entity: Color32,
    pub text: Color32,
    /// Line numbers.
    pub gutter: Color32,
    /// Indent guides.
    pub guide: Color32,
    pub hover: Color32,
}

fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

fn black(a: u8) -> Color32 {
    Color32::from_black_alpha(a)
}

fn white(a: u8) -> Color32 {
    Color32::from_white_alpha(a)
}

impl Palette {
    pub fn new(scheme: Scheme, high_contrast: bool) -> Palette {
        let mut p = match scheme {
            Scheme::Light => Palette {
                scheme,
                // Window, sidebar and toolbar follow macOS: white content, a
                // cool grey sidebar, and a grouped background that lets white
                // cards lift off the detail pane.
                content: rgb(0xFFFFFF),
                sidebar: rgb(0xF2F2F6),
                toolbar: rgb(0xFAFAFC),
                panel: rgb(0xF4F4F8),
                grouped: rgb(0xF2F2F7),
                card: rgb(0xFFFFFF),
                code: rgb(0xFFFFFF),
                separator: Color32::from_rgba_unmultiplied(60, 60, 67, 41),
                text: rgb(0x1C1C1E),
                text2: rgb(0x58585D),
                text3: rgb(0x8A8A90),
                accent: rgb(0x007AFF),
                on_accent: rgb(0xFFFFFF),
                accent_soft: Color32::from_rgba_unmultiplied(0, 122, 255, 30),
                fill: Color32::from_rgba_unmultiplied(120, 120, 128, 31),
                fill_hover: Color32::from_rgba_unmultiplied(120, 120, 128, 46),
                fill_press: Color32::from_rgba_unmultiplied(120, 120, 128, 66),
                row_alt: Color32::from_rgba_unmultiplied(60, 60, 67, 9),
                green: rgb(0x1F8A3C),
                orange: rgb(0xD25F00),
                red: rgb(0xDD2B1F),
                purple: rgb(0x9B3FD0),
                teal: rgb(0x007F9A),
                pink: rgb(0xD6336C),
                indigo: rgb(0x4B48D6),
                yellow: rgb(0xB07A00),
                scrim: black(70),
                syn: Syntax {
                    key: rgb(0x0A5FD4),
                    string: rgb(0xC9321C),
                    number: rgb(0x0B8A5A),
                    literal: rgb(0xB1228C),
                    punct: rgb(0x7A7A82),
                    tag: rgb(0xB1228C),
                    attr: rgb(0x0A5FD4),
                    value: rgb(0xC9321C),
                    comment: rgb(0x80868F),
                    entity: rgb(0x0B8A5A),
                    text: rgb(0x1C1C1E),
                    gutter: rgb(0xB4B4BC),
                    guide: black(22),
                    hover: Color32::from_rgba_unmultiplied(0, 122, 255, 20),
                },
            },
            Scheme::Dark => Palette {
                scheme,
                content: rgb(0x1C1C1E),
                sidebar: rgb(0x262629),
                toolbar: rgb(0x2A2A2D),
                panel: rgb(0x161618),
                grouped: rgb(0x121214),
                card: rgb(0x242427),
                code: rgb(0x0E0E10),
                separator: Color32::from_rgba_unmultiplied(84, 84, 88, 120),
                text: rgb(0xF5F5F7),
                text2: rgb(0xB0B0B8),
                text3: rgb(0x80808A),
                accent: rgb(0x0A84FF),
                on_accent: rgb(0xFFFFFF),
                accent_soft: Color32::from_rgba_unmultiplied(10, 132, 255, 64),
                fill: Color32::from_rgba_unmultiplied(120, 120, 128, 58),
                fill_hover: Color32::from_rgba_unmultiplied(120, 120, 128, 80),
                fill_press: Color32::from_rgba_unmultiplied(120, 120, 128, 110),
                row_alt: white(8),
                green: rgb(0x32D74B),
                orange: rgb(0xFF9F0A),
                red: rgb(0xFF6259),
                purple: rgb(0xBF5AF2),
                teal: rgb(0x5AD2F0),
                pink: rgb(0xFF5C8A),
                indigo: rgb(0x7D7AFF),
                yellow: rgb(0xFFD60A),
                scrim: black(130),
                syn: Syntax {
                    key: rgb(0x64D2FF),
                    string: rgb(0xFF8F70),
                    number: rgb(0x7BE0A0),
                    literal: rgb(0xFF7AB2),
                    punct: rgb(0x8A8A96),
                    tag: rgb(0xFF7AB2),
                    attr: rgb(0x64D2FF),
                    value: rgb(0xFF8F70),
                    comment: rgb(0x6E7A88),
                    entity: rgb(0x7BE0A0),
                    text: rgb(0xEDEDF2),
                    gutter: rgb(0x4A4A52),
                    guide: white(22),
                    hover: Color32::from_rgba_unmultiplied(10, 132, 255, 38),
                },
            },
        };
        if high_contrast {
            match scheme {
                Scheme::Light => {
                    p.separator = black(90);
                    p.text2 = rgb(0x3C3C40);
                    p.text3 = rgb(0x5C5C62);
                    p.fill = black(28);
                    p.syn.punct = rgb(0x4A4A50);
                    p.syn.gutter = rgb(0x6C6C74);
                    p.syn.comment = rgb(0x5A6068);
                }
                Scheme::Dark => {
                    p.separator = white(110);
                    p.text2 = rgb(0xD0D0D6);
                    p.text3 = rgb(0xA8A8B0);
                    p.fill = white(36);
                    p.syn.punct = rgb(0xC4C4CE);
                    p.syn.gutter = rgb(0x9A9AA6);
                    p.syn.comment = rgb(0xA6B0BC);
                }
            }
        }
        p
    }

    /// Blend two palettes; the scheme flips to `b`'s at once so shadows and
    /// materials that branch on it settle immediately while colors ease.
    pub fn lerp(a: &Palette, b: &Palette, t: f32) -> Palette {
        let m = |x: Color32, y: Color32| {
            let f = |p: u8, q: u8| (p as f32 + (q as f32 - p as f32) * t).round() as u8;
            Color32::from_rgba_premultiplied(
                f(x.r(), y.r()),
                f(x.g(), y.g()),
                f(x.b(), y.b()),
                f(x.a(), y.a()),
            )
        };
        Palette {
            scheme: b.scheme,
            content: m(a.content, b.content),
            sidebar: m(a.sidebar, b.sidebar),
            toolbar: m(a.toolbar, b.toolbar),
            panel: m(a.panel, b.panel),
            grouped: m(a.grouped, b.grouped),
            card: m(a.card, b.card),
            code: m(a.code, b.code),
            separator: m(a.separator, b.separator),
            text: m(a.text, b.text),
            text2: m(a.text2, b.text2),
            text3: m(a.text3, b.text3),
            accent: m(a.accent, b.accent),
            on_accent: m(a.on_accent, b.on_accent),
            accent_soft: m(a.accent_soft, b.accent_soft),
            fill: m(a.fill, b.fill),
            fill_hover: m(a.fill_hover, b.fill_hover),
            fill_press: m(a.fill_press, b.fill_press),
            row_alt: m(a.row_alt, b.row_alt),
            green: m(a.green, b.green),
            orange: m(a.orange, b.orange),
            red: m(a.red, b.red),
            purple: m(a.purple, b.purple),
            teal: m(a.teal, b.teal),
            pink: m(a.pink, b.pink),
            indigo: m(a.indigo, b.indigo),
            yellow: m(a.yellow, b.yellow),
            scrim: m(a.scrim, b.scrim),
            syn: Syntax {
                key: m(a.syn.key, b.syn.key),
                string: m(a.syn.string, b.syn.string),
                number: m(a.syn.number, b.syn.number),
                literal: m(a.syn.literal, b.syn.literal),
                punct: m(a.syn.punct, b.syn.punct),
                tag: m(a.syn.tag, b.syn.tag),
                attr: m(a.syn.attr, b.syn.attr),
                value: m(a.syn.value, b.syn.value),
                comment: m(a.syn.comment, b.syn.comment),
                entity: m(a.syn.entity, b.syn.entity),
                text: m(a.syn.text, b.syn.text),
                gutter: m(a.syn.gutter, b.syn.gutter),
                guide: m(a.syn.guide, b.syn.guide),
                hover: m(a.syn.hover, b.syn.hover),
            },
        }
    }

    /// A translucent wash of a status color for badge and banner fills. On
    /// light backgrounds the text colors are deepened for contrast, so washes
    /// are taken from the brighter system hue instead of going muddy.
    pub fn tint(&self, c: Color32, alpha: f32) -> Color32 {
        let base = if self.is_dark() {
            c
        } else if c == self.orange {
            rgb(0xFF9500)
        } else if c == self.green {
            rgb(0x34C759)
        } else if c == self.red {
            rgb(0xFF3B30)
        } else if c == self.purple {
            rgb(0xAF52DE)
        } else if c == self.teal {
            rgb(0x30B0C7)
        } else if c == self.pink {
            rgb(0xFF2D55)
        } else {
            c
        };
        base.gamma_multiply(alpha.clamp(0.0, 1.0))
    }

    pub fn is_dark(&self) -> bool {
        self.scheme == Scheme::Dark
    }

    /// HTTP status to a semantic color.
    pub fn status(&self, status: u32, failed: bool) -> Color32 {
        match status {
            _ if failed || status == 0 => self.text3,
            200..=299 => self.green,
            300..=399 => self.text2,
            400..=499 => self.orange,
            _ => self.red,
        }
    }

    pub fn method(&self, m: &str) -> Color32 {
        match m {
            "GET" => self.green,
            "POST" => self.orange,
            "PUT" => self.accent,
            "PATCH" => self.purple,
            "DELETE" => self.red,
            "CONNECT" => self.teal,
            _ => self.text2,
        }
    }
}

/// Type roles. Tracking is size-specific: large text tightens, small text
/// opens up, body sits at zero. Leading follows size inversely.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    LargeTitle,
    Title,
    Headline,
    Body,
    BodyMedium,
    Callout,
    Caption,
    Overline,
    Mono,
    MonoSmall,
    /// Body viewers: a touch larger than table text, with room to breathe.
    Code,
    CodeMedium,
}

pub struct TypeSpec {
    pub size: f32,
    pub family: FontFamily,
    /// Extra letter spacing in points.
    pub tracking: f32,
    /// Row height for single lines.
    pub line: f32,
}

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn medium() -> FontFamily {
    FontFamily::Name("medium".into())
}

impl Role {
    pub fn spec(self) -> TypeSpec {
        let s = |size, family, tracking, line| TypeSpec {
            size,
            family,
            tracking,
            line,
        };
        match self {
            Role::LargeTitle => s(24.0, semibold(), -0.55, 30.0),
            Role::Title => s(17.0, semibold(), -0.34, 22.0),
            Role::Headline => s(13.0, semibold(), -0.08, 18.0),
            Role::Body => s(13.0, FontFamily::Proportional, 0.0, 18.0),
            Role::BodyMedium => s(13.0, medium(), 0.0, 18.0),
            Role::Callout => s(12.0, FontFamily::Proportional, 0.06, 16.0),
            Role::Caption => s(11.0, medium(), 0.16, 14.0),
            Role::Overline => s(10.5, semibold(), 0.7, 14.0),
            Role::Mono => s(12.0, FontFamily::Monospace, 0.0, 17.0),
            Role::MonoSmall => s(11.0, FontFamily::Monospace, 0.0, 15.0),
            Role::Code => s(12.0, FontFamily::Monospace, 0.0, 20.0),
            Role::CodeMedium => s(12.0, FontFamily::Name("mono-medium".into()), 0.0, 20.0),
        }
    }

    pub fn font(self) -> FontId {
        let s = self.spec();
        FontId::new(s.size, s.family)
    }
}

/// Corner radii. Larger surfaces get larger radii.
pub mod radius {
    pub const CONTROL: u8 = 7;
    pub const CARD: u8 = 10;
    pub const SHEET: u8 = 14;
    pub const PANEL: u8 = 12;
    pub const PILL: u8 = 255;
}

/// A bundle of what widgets need to draw and animate.
#[derive(Clone)]
pub struct Style {
    pub p: Palette,
    pub reduced_motion: bool,
    pub reduced_transparency: bool,
}
