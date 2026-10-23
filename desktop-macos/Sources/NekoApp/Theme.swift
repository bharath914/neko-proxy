import AppKit

/// Design tokens, copied from `desktop/src/theme.rs` so both apps look the same.
/// Every color is dynamic: it resolves against the drawing appearance, so views
/// only need to draw in `draw(_:)` and light/dark switches need no bookkeeping.
enum Theme {
    private static func rgb(_ hex: UInt32, _ a: CGFloat = 1) -> NSColor {
        NSColor(srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255,
                blue: CGFloat(hex & 0xFF) / 255, alpha: a)
    }

    private static func rgba(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat, _ a: CGFloat) -> NSColor {
        NSColor(srgbRed: r / 255, green: g / 255, blue: b / 255, alpha: a / 255)
    }

    private static func dyn(_ light: NSColor, _ dark: NSColor) -> NSColor {
        NSColor(name: nil) { ap in
            ap.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? dark : light
        }
    }

    static let content = dyn(rgb(0xFFFFFF), rgb(0x1C1C1E))
    static let sidebar = dyn(rgb(0xF2F2F6), rgb(0x262629))
    static let toolbar = dyn(rgb(0xFAFAFC), rgb(0x2A2A2D))
    static let panel = dyn(rgb(0xF4F4F8), rgb(0x161618))
    static let grouped = dyn(rgb(0xF2F2F7), rgb(0x121214))
    static let card = dyn(rgb(0xFFFFFF), rgb(0x242427))
    static let code = dyn(rgb(0xFFFFFF), rgb(0x0E0E10))
    static let separator = dyn(rgba(60, 60, 67, 41), rgba(84, 84, 88, 120))
    static let text = dyn(rgb(0x1C1C1E), rgb(0xF5F5F7))
    static let text2 = dyn(rgb(0x58585D), rgb(0xB0B0B8))
    static let text3 = dyn(rgb(0x8A8A90), rgb(0x80808A))
    static let accent = dyn(rgb(0x007AFF), rgb(0x0A84FF))
    static let onAccent = rgb(0xFFFFFF)
    static let accentSoft = dyn(rgba(0, 122, 255, 30), rgba(10, 132, 255, 64))
    static let fill = dyn(rgba(120, 120, 128, 31), rgba(120, 120, 128, 58))
    static let fillHover = dyn(rgba(120, 120, 128, 46), rgba(120, 120, 128, 80))
    static let fillPress = dyn(rgba(120, 120, 128, 66), rgba(120, 120, 128, 110))
    static let rowAlt = dyn(rgba(60, 60, 67, 9), NSColor.white.withAlphaComponent(8.0 / 255))
    static let green = dyn(rgb(0x1F8A3C), rgb(0x32D74B))
    static let orange = dyn(rgb(0xD25F00), rgb(0xFF9F0A))
    static let red = dyn(rgb(0xDD2B1F), rgb(0xFF6259))
    static let purple = dyn(rgb(0x9B3FD0), rgb(0xBF5AF2))
    static let teal = dyn(rgb(0x007F9A), rgb(0x5AD2F0))
    static let pink = dyn(rgb(0xD6336C), rgb(0xFF5C8A))
    static let indigo = dyn(rgb(0x4B48D6), rgb(0x7D7AFF))
    static let yellow = dyn(rgb(0xB07A00), rgb(0xFFD60A))
    /// Selected segment of a segmented control.
    static let segSelected = dyn(rgb(0xFFFFFF), rgb(0x636366))

    /// Washes for badge and banner fills: the brighter system hue on light.
    static func tint(_ c: NSColor, _ alpha: CGFloat) -> NSColor {
        let brighter: [(NSColor, UInt32)] = [(orange, 0xFF9500), (green, 0x34C759), (red, 0xFF3B30),
                                             (purple, 0xAF52DE), (teal, 0x30B0C7), (pink, 0xFF2D55)]
        return NSColor(name: nil) { ap in
            let dark = ap.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            var base = c
            if !dark, let b = brighter.first(where: { $0.0 === c }) { base = rgb(b.1) }
            var out = base
            ap.performAsCurrentDrawingAppearance { out = base.usingColorSpace(.sRGB) ?? base }
            return out.withAlphaComponent(alpha)
        }
    }

    static func status(_ status: UInt32, failed: Bool) -> NSColor {
        if failed || status == 0 { return text3 }
        switch status {
        case 200..<300: return green
        case 300..<400: return text2
        case 400..<500: return orange
        default: return red
        }
    }

    static func method(_ m: String) -> NSColor {
        switch m {
        case "GET": return green
        case "POST": return orange
        case "PUT": return accent
        case "PATCH": return purple
        case "DELETE": return red
        case "CONNECT": return teal
        default: return text2
        }
    }

    // MARK: syntax

    enum Syn {
        static let key = dyn(rgb(0x0A5FD4), rgb(0x64D2FF))
        static let string = dyn(rgb(0xC9321C), rgb(0xFF8F70))
        static let number = dyn(rgb(0x0B8A5A), rgb(0x7BE0A0))
        static let literal = dyn(rgb(0xB1228C), rgb(0xFF7AB2))
        static let punct = dyn(rgb(0x7A7A82), rgb(0x8A8A96))
        static let tag = literal
        static let attr = key
        static let value = string
        static let comment = dyn(rgb(0x80868F), rgb(0x6E7A88))
        static let entity = number
        static let text = dyn(rgb(0x1C1C1E), rgb(0xEDEDF2))
        static let gutter = dyn(rgb(0xB4B4BC), rgb(0x4A4A52))
    }

    // MARK: type

    enum Role {
        case largeTitle, title, headline, body, bodyMedium, callout, caption, overline, mono, monoSmall, code

        var size: CGFloat {
            switch self {
            case .largeTitle: return 24
            case .title: return 17
            case .headline, .body, .bodyMedium: return 13
            case .callout, .mono, .code: return 12
            case .caption: return 11
            case .overline: return 10.5
            case .monoSmall: return 11
            }
        }

        var tracking: CGFloat {
            switch self {
            case .largeTitle: return -0.55
            case .title: return -0.34
            case .headline: return -0.08
            case .callout: return 0.06
            case .caption: return 0.16
            case .overline: return 0.7
            default: return 0
            }
        }
    }

    static func font(_ role: Role) -> NSFont {
        let s = role.size
        switch role {
        case .largeTitle, .title, .headline, .overline: return named("Inter-SemiBold", s, .semibold)
        case .bodyMedium, .caption: return named("Inter-Medium", s, .medium)
        case .mono, .monoSmall, .code: return named("JetBrainsMono-Regular", s, .regular, mono: true)
        default: return named("Inter-Regular", s, .regular)
        }
    }

    static func monoMedium(_ size: CGFloat) -> NSFont { named("JetBrainsMono-Medium", size, .medium, mono: true) }

    private static func named(_ ps: String, _ size: CGFloat, _ weight: NSFont.Weight, mono: Bool = false) -> NSFont {
        NSFont(name: ps, size: size)
            ?? (mono ? NSFont.monospacedSystemFont(ofSize: size, weight: weight) : NSFont.systemFont(ofSize: size, weight: weight))
    }

    static func attrs(_ role: Role, _ color: NSColor = text, extra: [NSAttributedString.Key: Any] = [:]) -> [NSAttributedString.Key: Any] {
        var a: [NSAttributedString.Key: Any] = [.font: font(role), .foregroundColor: color]
        if role.tracking != 0 { a[.kern] = role.tracking }
        for (k, v) in extra { a[k] = v }
        return a
    }

    /// Registers the bundled Inter and JetBrains Mono. Call once at launch.
    static func registerFonts() {
        // Inside Neko.app the fonts sit in Contents/Resources; under `swift run`
        // they are in the SwiftPM resource bundle.
        guard let dir = Bundle.main.url(forResource: "Fonts", withExtension: nil) ?? Bundle.module.url(forResource: "Fonts", withExtension: nil),
              let files = try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)
        else { return }
        for f in files where f.pathExtension == "ttf" {
            CTFontManagerRegisterFontsForURL(f as CFURL, .process, nil)
        }
    }

    enum Radius {
        static let control: CGFloat = 7
        static let card: CGFloat = 10
        static let sheet: CGFloat = 14
    }
}
