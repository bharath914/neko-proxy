import AppKit

enum Section: Int, CaseIterable { case inspector, breakpoints, rules, devices, settings }

/// A pill that shows capture state.
final class RecordPill: NSView {
    var recording = true { didSet { needsDisplay = true } }
    var onClick: (() -> Void)?
    private var hover = false
    override var isFlipped: Bool { true }
    override var mouseDownCanMoveWindow: Bool { false }

    init() {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        heightAnchor.constraint(equalToConstant: 30).isActive = true
        widthAnchor.constraint(equalToConstant: 104).isActive = true
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        let color = recording ? Theme.red : Theme.text3
        let p = NSBezierPath(roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), xRadius: bounds.height / 2, yRadius: bounds.height / 2)
        Theme.tint(color, hover ? 0.28 : 0.2).setFill(); p.fill()
        Theme.tint(color, 0.45).setStroke(); p.lineWidth = 1; p.stroke()
        color.setFill()
        NSBezierPath(ovalIn: NSRect(x: 14, y: bounds.midY - 4, width: 8, height: 8)).fill()
        let a = Theme.attrs(.bodyMedium, Theme.text)
        let s = (recording ? "Recording" : "Paused") as NSString
        let sz = s.size(withAttributes: a)
        s.draw(at: NSPoint(x: 28, y: bounds.midY - sz.height / 2), withAttributes: a)
    }
    override func mouseUp(with event: NSEvent) { if bounds.contains(convert(event.locationInWindow, from: nil)) { onClick?() } }
    override func mouseEntered(with event: NSEvent) { hover = true; needsDisplay = true }
    override func mouseExited(with event: NSEvent) { hover = false; needsDisplay = true }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }
}

/// Rounded search field with a magnifier and a "/" key hint.
final class SearchBox: Surface, NSTextFieldDelegate {
    let field = NSTextField()
    var onChange: ((String) -> Void)?

    override init(frame: NSRect) {
        super.init(frame: frame)
        fill = Theme.fill
        radius = 8
        translatesAutoresizingMaskIntoConstraints = false
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = Theme.font(.body)
        field.textColor = Theme.text
        field.placeholderAttributedString = NSAttributedString(string: "Filter flows", attributes: Theme.attrs(.body, Theme.text2))
        field.delegate = self
        field.cell?.isScrollable = true
        field.cell?.usesSingleLineMode = true
        field.translatesAutoresizingMaskIntoConstraints = false
        addSubview(field)
        NSLayoutConstraint.activate([
            field.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 32),
            field.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -34),
            field.centerYAnchor.constraint(equalTo: centerYAnchor),
            heightAnchor.constraint(equalToConstant: 30),
        ])
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        drawSymbol("magnifyingglass", in: NSRect(x: 8, y: 0, width: 18, height: bounds.height), color: Theme.text2, size: 12, weight: .medium)
        if field.stringValue.isEmpty {
            let r = NSRect(x: bounds.maxX - 26, y: bounds.midY - 9, width: 18, height: 18)
            let p = NSBezierPath(roundedRect: r, xRadius: 4, yRadius: 4)
            Theme.fill.setFill(); p.fill()
            let a = Theme.attrs(.monoSmall, Theme.text3)
            let sz = ("/" as NSString).size(withAttributes: a)
            ("/" as NSString).draw(at: NSPoint(x: r.midX - sz.width / 2, y: r.midY - sz.height / 2), withAttributes: a)
        }
    }

    func controlTextDidChange(_ obj: Notification) { needsDisplay = true; onChange?(field.stringValue) }
}

/// The window's top bar: navigation, capture controls, filters, search and the Proxy entry.
@MainActor
final class TopBar: NSView {
    let nav = Segmented(["Inspector", "Breakpoints", "Rules", "Devices", "Settings"])
    let record = RecordPill()
    let clear = IconButton("trash", tip: "Clear all flows (⌘K)")
    let filter = Segmented(["All", "Errors", "Slow", "Pinned"])
    let search = SearchBox()
    let export = IconButton("square.and.arrow.down", tip: "Export as HAR")
    let phone = IconButton("iphone", tip: "Connect a phone")
    let proxy = Chip("Proxy", symbol: "shield", style: .plain)
    private var inspectorOnly: [NSView] = []
    override var isFlipped: Bool { true }
    override var mouseDownCanMoveWindow: Bool { true }

    init() {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        heightAnchor.constraint(equalToConstant: 56).isActive = true

        let left = NSStackView.horizontal([nav, record, clear, filter], spacing: 8)
        left.setCustomSpacing(12, after: nav)
        let right = NSStackView.horizontal([export, phone, proxy], spacing: 8)
        for v in [left, search, right] { addSubview(v) }
        inspectorOnly = [record, clear, filter, search, export]
        NSLayoutConstraint.activate([
            left.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 82),
            left.centerYAnchor.constraint(equalTo: centerYAnchor),
            right.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -14),
            right.centerYAnchor.constraint(equalTo: centerYAnchor),
            search.centerYAnchor.constraint(equalTo: centerYAnchor),
            search.trailingAnchor.constraint(equalTo: right.leadingAnchor, constant: -10),
            search.leadingAnchor.constraint(greaterThanOrEqualTo: left.trailingAnchor, constant: 10),
            search.widthAnchor.constraint(lessThanOrEqualToConstant: 280),
            search.widthAnchor.constraint(greaterThanOrEqualToConstant: 150),
        ])
        let flex = search.widthAnchor.constraint(equalToConstant: 280)
        flex.priority = .defaultHigh
        flex.isActive = true
        left.setContentCompressionResistancePriority(.required, for: .horizontal)
    }
    required init?(coder: NSCoder) { fatalError() }

    func setSection(_ s: Section) {
        nav.selected = s.rawValue
        for v in inspectorOnly { v.isHidden = s != .inspector }
    }

    override func draw(_ dirtyRect: NSRect) {
        Theme.toolbar.setFill(); bounds.fill()
        Theme.separator.setFill()
        NSRect(x: 0, y: bounds.height - 1, width: bounds.width, height: 1).fill()
    }
}
