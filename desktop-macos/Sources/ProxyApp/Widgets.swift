import AppKit

/// A view that draws its own fill and border, so dynamic colors resolve at draw
/// time and appearance changes need no extra work.
class Surface: NSView {
    var fill: NSColor? { didSet { needsDisplay = true } }
    var border: NSColor? { didSet { needsDisplay = true } }
    var radius: CGFloat = 0 { didSet { needsDisplay = true } }
    override var isFlipped: Bool { true }

    override func draw(_ dirtyRect: NSRect) {
        let r = bounds.insetBy(dx: border == nil ? 0 : 0.5, dy: border == nil ? 0 : 0.5)
        let path = NSBezierPath(roundedRect: r, xRadius: radius, yRadius: radius)
        if let fill { fill.setFill(); path.fill() }
        if let border { border.setStroke(); path.lineWidth = 1; path.stroke() }
    }
}

func label(_ s: String, _ role: Theme.Role = .body, _ color: NSColor = Theme.text, lines: Int = 1) -> NSTextField {
    let t = NSTextField(labelWithAttributedString: NSAttributedString(string: s, attributes: Theme.attrs(role, color)))
    t.maximumNumberOfLines = lines
    t.lineBreakMode = lines == 1 ? .byTruncatingTail : .byWordWrapping
    t.translatesAutoresizingMaskIntoConstraints = false
    t.isSelectable = false
    return t
}

extension NSTextField {
    func set(_ s: String, _ role: Theme.Role, _ color: NSColor) {
        attributedStringValue = NSAttributedString(string: s, attributes: Theme.attrs(role, color))
    }
}

func symbol(_ name: String, size: CGFloat = 13, weight: NSFont.Weight = .regular) -> NSImage? {
    let cfg = NSImage.SymbolConfiguration(pointSize: size, weight: weight)
    return NSImage(systemSymbolName: name, accessibilityDescription: nil)?.withSymbolConfiguration(cfg)
}

/// A symbol tinted with a dynamic color, drawn at the current appearance.
func drawSymbol(_ name: String, in rect: NSRect, color: NSColor, size: CGFloat = 13, weight: NSFont.Weight = .regular) {
    guard let img = symbol(name, size: size, weight: weight) else { return }
    let s = img.size
    let r = NSRect(x: rect.midX - s.width / 2, y: rect.midY - s.height / 2, width: s.width, height: s.height)
    let tinted = NSImage(size: s, flipped: false) { b in
        img.draw(in: b)
        color.setFill()
        b.fill(using: .sourceAtop)
        return true
    }
    tinted.draw(in: r, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
}

// MARK: - Segmented control

/// The pill segmented control used for navigation, filters and tabs.
final class Segmented: NSView {
    var titles: [String] { didSet { invalidateIntrinsicContentSize(); needsDisplay = true } }
    var selected: Int { didSet { needsDisplay = true } }
    var onChange: ((Int) -> Void)?
    var equalWidths = false
    var role: Theme.Role = .bodyMedium
    var padding: CGFloat = 14
    private var hover: Int?
    private var tracking: NSTrackingArea?
    override var isFlipped: Bool { true }

    init(_ titles: [String], selected: Int = 0) {
        self.titles = titles
        self.selected = selected
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
    }
    required init?(coder: NSCoder) { fatalError() }

    private func widths() -> [CGFloat] {
        let natural = titles.map { ($0 as NSString).size(withAttributes: Theme.attrs(role)).width + padding * 2 }
        if equalWidths {
            let w = (bounds.width - 4) / CGFloat(max(titles.count, 1))
            return Array(repeating: w, count: titles.count)
        }
        return natural
    }

    override var intrinsicContentSize: NSSize {
        let w = titles.map { ($0 as NSString).size(withAttributes: Theme.attrs(role)).width + padding * 2 }.reduce(0, +) + 4
        return NSSize(width: equalWidths ? NSView.noIntrinsicMetric : w, height: 30)
    }

    override func draw(_ dirtyRect: NSRect) {
        let track = NSBezierPath(roundedRect: bounds, xRadius: 8, yRadius: 8)
        Theme.fill.setFill()
        track.fill()
        var x: CGFloat = 2
        for (i, w) in widths().enumerated() {
            let r = NSRect(x: x, y: 2, width: w, height: bounds.height - 4)
            if i == selected {
                let p = NSBezierPath(roundedRect: r, xRadius: 6, yRadius: 6)
                Theme.segSelected.setFill()
                p.fill()
            } else if i == hover {
                let p = NSBezierPath(roundedRect: r, xRadius: 6, yRadius: 6)
                Theme.fill.setFill()
                p.fill()
            }
            let color = i == selected ? Theme.text : Theme.text2
            let a = Theme.attrs(i == selected ? .bodyMedium : role, color)
            let s = titles[i] as NSString
            let size = s.size(withAttributes: a)
            s.draw(at: NSPoint(x: r.midX - size.width / 2, y: r.midY - size.height / 2), withAttributes: a)
            x += w
        }
    }

    private func index(at p: NSPoint) -> Int? {
        var x: CGFloat = 2
        for (i, w) in widths().enumerated() {
            if p.x >= x && p.x < x + w { return i }
            x += w
        }
        return nil
    }

    override func mouseDown(with event: NSEvent) {
        if let i = index(at: convert(event.locationInWindow, from: nil)), i != selected {
            selected = i
            onChange?(i)
        }
    }
    override func mouseMoved(with event: NSEvent) {
        let i = index(at: convert(event.locationInWindow, from: nil))
        if i != hover { hover = i; needsDisplay = true }
    }
    override func mouseExited(with event: NSEvent) { hover = nil; needsDisplay = true }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let t = tracking { removeTrackingArea(t) }
        let t = NSTrackingArea(rect: bounds, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self)
        addTrackingArea(t)
        tracking = t
    }
    override var acceptsFirstResponder: Bool { false }
}

// MARK: - Buttons

enum ChipStyle { case plain, accent, text, danger }

/// A rounded button with an optional symbol, used for Copy, Replay, Install and so on.
final class Chip: NSView {
    var title: String { didSet { invalidateIntrinsicContentSize(); needsDisplay = true } }
    var symbolName: String? { didSet { invalidateIntrinsicContentSize(); needsDisplay = true } }
    var style: ChipStyle { didSet { needsDisplay = true } }
    var isEnabled = true { didSet { needsDisplay = true } }
    var onClick: (() -> Void)?
    private var hover = false, pressed = false
    override var isFlipped: Bool { true }

    init(_ title: String, symbol: String? = nil, style: ChipStyle = .plain) {
        self.title = title
        self.symbolName = symbol
        self.style = style
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
    }
    required init?(coder: NSCoder) { fatalError() }

    override var intrinsicContentSize: NSSize {
        let tw = (title as NSString).size(withAttributes: Theme.attrs(.bodyMedium)).width
        return NSSize(width: tw + 24 + (symbolName == nil ? 0 : 20), height: 30)
    }

    override func draw(_ dirtyRect: NSRect) {
        var bg: NSColor? = nil
        var fg = Theme.text
        switch style {
        case .plain: bg = pressed ? Theme.fillPress : hover && isEnabled ? Theme.fillHover : Theme.fill
        case .accent: bg = Theme.accent; fg = Theme.onAccent
        case .text: fg = Theme.accent; if hover && isEnabled { bg = Theme.accentSoft }
        case .danger: bg = Theme.fill; fg = Theme.red
        }
        if !isEnabled { fg = Theme.text3; if style == .accent { bg = Theme.fill } }
        if let bg {
            let p = NSBezierPath(roundedRect: bounds, xRadius: Theme.Radius.control, yRadius: Theme.Radius.control)
            bg.setFill(); p.fill()
        }
        let a = Theme.attrs(.bodyMedium, fg)
        let size = (title as NSString).size(withAttributes: a)
        let iconW: CGFloat = symbolName == nil ? 0 : 20
        var x = bounds.midX - (size.width + iconW) / 2
        if let s = symbolName {
            drawSymbol(s, in: NSRect(x: x, y: 0, width: 16, height: bounds.height), color: fg, size: 12, weight: .medium)
            x += iconW
        }
        (title as NSString).draw(at: NSPoint(x: x, y: bounds.midY - size.height / 2), withAttributes: a)
    }

    override func mouseDown(with event: NSEvent) { if isEnabled { pressed = true; needsDisplay = true } }
    override func mouseUp(with event: NSEvent) {
        let inside = bounds.contains(convert(event.locationInWindow, from: nil))
        if pressed && inside { onClick?() }
        pressed = false
        needsDisplay = true
    }
    override func mouseEntered(with event: NSEvent) { hover = true; needsDisplay = true }
    override func mouseExited(with event: NSEvent) { hover = false; needsDisplay = true }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }
}

/// A square icon-only button.
final class IconButton: NSView {
    var symbolName: String { didSet { needsDisplay = true } }
    var isEnabled = true { didSet { needsDisplay = true } }
    var dot: NSColor? { didSet { needsDisplay = true } }
    var onClick: (() -> Void)?
    private var hover = false
    override var isFlipped: Bool { true }

    init(_ symbolName: String, tip: String) {
        self.symbolName = symbolName
        super.init(frame: .zero)
        toolTip = tip
        translatesAutoresizingMaskIntoConstraints = false
        widthAnchor.constraint(equalToConstant: 30).isActive = true
        heightAnchor.constraint(equalToConstant: 30).isActive = true
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        if hover && isEnabled {
            let p = NSBezierPath(roundedRect: bounds, xRadius: Theme.Radius.control, yRadius: Theme.Radius.control)
            Theme.fillHover.setFill(); p.fill()
        }
        drawSymbol(symbolName, in: bounds, color: isEnabled ? Theme.text2 : Theme.text3, size: 14)
        if let dot {
            dot.setFill()
            NSBezierPath(ovalIn: NSRect(x: bounds.maxX - 10, y: 4, width: 7, height: 7)).fill()
        }
    }
    override func mouseUp(with event: NSEvent) {
        if isEnabled && bounds.contains(convert(event.locationInWindow, from: nil)) { onClick?() }
    }
    override func mouseEntered(with event: NSEvent) { hover = true; needsDisplay = true }
    override func mouseExited(with event: NSEvent) { hover = false; needsDisplay = true }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }
}

// MARK: - Badge

/// A small tinted label: SOURCE column, method and status in the detail header.
final class Badge: NSView {
    private let text: String
    private let color: NSColor
    private let role: Theme.Role
    override var isFlipped: Bool { true }

    init(_ text: String, color: NSColor, role: Theme.Role = .caption) {
        self.text = text
        self.color = color
        self.role = role
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
    }
    required init?(coder: NSCoder) { fatalError() }

    override var intrinsicContentSize: NSSize {
        let w = (text as NSString).size(withAttributes: Theme.attrs(role)).width
        return NSSize(width: w + 14, height: 20)
    }

    override func draw(_ dirtyRect: NSRect) {
        let p = NSBezierPath(roundedRect: bounds, xRadius: 5, yRadius: 5)
        Theme.tint(color, 0.18).setFill()
        p.fill()
        let a = Theme.attrs(role, color)
        let s = (text as NSString).size(withAttributes: a)
        (text as NSString).draw(at: NSPoint(x: bounds.midX - s.width / 2, y: bounds.midY - s.height / 2), withAttributes: a)
    }
}

// MARK: - Layout helpers

final class FlippedView: NSView {
    override var isFlipped: Bool { true }
}

/// Lays its subviews out left to right and wraps to a new row when full.
final class WrapView: NSView {
    var spacing: CGFloat = 8
    private var heightConstraint: NSLayoutConstraint!
    override var isFlipped: Bool { true }

    override init(frame: NSRect) {
        super.init(frame: frame)
        translatesAutoresizingMaskIntoConstraints = false
        heightConstraint = heightAnchor.constraint(equalToConstant: 30)
        heightConstraint.isActive = true
    }
    required init?(coder: NSCoder) { fatalError() }

    override func layout() {
        super.layout()
        var x: CGFloat = 0, y: CGFloat = 0, rowH: CGFloat = 0
        for v in subviews {
            let s = v.fittingSize
            if x > 0 && x + s.width > bounds.width { x = 0; y += rowH + spacing; rowH = 0 }
            v.frame = NSRect(x: x, y: y, width: s.width, height: s.height)
            x += s.width + spacing
            rowH = max(rowH, s.height)
        }
        let h = y + rowH
        if abs(heightConstraint.constant - h) > 0.5 { heightConstraint.constant = h }
    }
}

/// A 1 px divider that follows the separator color.
final class Divider: NSView {
    var vertical = false
    override var isFlipped: Bool { true }
    init(vertical: Bool = false) {
        self.vertical = vertical
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        if vertical { widthAnchor.constraint(equalToConstant: 1).isActive = true }
        else { heightAnchor.constraint(equalToConstant: 1).isActive = true }
    }
    required init?(coder: NSCoder) { fatalError() }
    override func draw(_ dirtyRect: NSRect) { Theme.separator.setFill(); bounds.fill() }
}

extension NSStackView {
    static func vertical(_ views: [NSView], spacing: CGFloat = 0) -> NSStackView {
        let s = NSStackView(views: views)
        s.orientation = .vertical
        s.alignment = .leading
        s.spacing = spacing
        s.translatesAutoresizingMaskIntoConstraints = false
        return s
    }
    static func horizontal(_ views: [NSView], spacing: CGFloat = 8, alignment: NSLayoutConstraint.Attribute = .centerY) -> NSStackView {
        let s = NSStackView(views: views)
        s.orientation = .horizontal
        s.alignment = alignment
        s.spacing = spacing
        s.translatesAutoresizingMaskIntoConstraints = false
        return s
    }
}

extension NSView {
    func pin(to other: NSView, insets: NSEdgeInsets = NSEdgeInsets()) {
        translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            leadingAnchor.constraint(equalTo: other.leadingAnchor, constant: insets.left),
            trailingAnchor.constraint(equalTo: other.trailingAnchor, constant: -insets.right),
            topAnchor.constraint(equalTo: other.topAnchor, constant: insets.top),
            bottomAnchor.constraint(equalTo: other.bottomAnchor, constant: -insets.bottom),
        ])
    }
}
