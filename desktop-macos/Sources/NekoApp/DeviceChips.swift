import AppKit
import NekoAPI

/// The row of "All traffic / Pixel 8 / …" chips above the table.
@MainActor
final class DeviceChips: NSView {
    var onPick: (([String]) -> Void)?
    private var items: [(id: String, name: String)] = []
    private var selected: Set<String> = []
    override var isFlipped: Bool { true }

    func update(devices: [Neko_Api_V1_Device], selected: [String]) {
        var seen = Set<String>()
        items = devices.compactMap { d in
            guard seen.insert(d.deviceID).inserted else { return nil }
            return (d.deviceID, d.deviceName.isEmpty ? d.deviceModel : d.deviceName)
        }
        self.selected = Set(selected)
        subviews.forEach { $0.removeFromSuperview() }
        needsLayout = true
        var chips: [(String, Bool, () -> Void)] = [("All traffic", self.selected.isEmpty, { [weak self] in self?.onPick?([]) })]
        for it in items {
            chips.append((it.name, self.selected.contains(it.id), { [weak self] in
                guard let self else { return }
                var s = self.selected
                if s.contains(it.id) { s.remove(it.id) } else { s.insert(it.id) }
                self.onPick?(Array(s))
            }))
        }
        var x: CGFloat = 12
        for (title, on, action) in chips {
            let c = DeviceChip(title, on: on)
            c.onClick = action
            let s = c.fittingSize
            c.frame = NSRect(x: x, y: 8, width: s.width, height: 26)
            addSubview(c)
            x += s.width + 6
        }
    }

    override func draw(_ dirtyRect: NSRect) { Theme.content.setFill(); bounds.fill() }
}

final class DeviceChip: NSView {
    private let title: String
    private let on: Bool
    var onClick: (() -> Void)?
    override var isFlipped: Bool { true }

    init(_ title: String, on: Bool) {
        self.title = title; self.on = on
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = true
    }
    required init?(coder: NSCoder) { fatalError() }

    override var fittingSize: NSSize {
        NSSize(width: (title as NSString).size(withAttributes: Theme.attrs(.callout)).width + 24, height: 26)
    }
    override func draw(_ dirtyRect: NSRect) {
        let p = NSBezierPath(roundedRect: bounds, xRadius: bounds.height / 2, yRadius: bounds.height / 2)
        (on ? Theme.accentSoft : Theme.fill).setFill(); p.fill()
        let a = Theme.attrs(.callout, on ? Theme.accent : Theme.text2)
        let sz = (title as NSString).size(withAttributes: a)
        (title as NSString).draw(at: NSPoint(x: bounds.midX - sz.width / 2, y: bounds.midY - sz.height / 2), withAttributes: a)
    }
    override func mouseUp(with event: NSEvent) { if bounds.contains(convert(event.locationInWindow, from: nil)) { onClick?() } }
}

/// Stand-in for screens that exist in the Linux app but not here yet.
final class ComingSoon: NSView {
    override var isFlipped: Bool { true }
    init(_ name: String) {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        let t = label("\(name) isn't in the macOS app yet", .title, Theme.text)
        let s = label("It works in the Linux app and through the CLI and MCP tools. Everything captured here is still available.", .body, Theme.text2, lines: 0)
        s.preferredMaxLayoutWidth = 380
        s.alignment = .center
        let st = NSStackView.vertical([t, s], spacing: 6)
        st.alignment = .centerX
        addSubview(st)
        NSLayoutConstraint.activate([
            st.centerXAnchor.constraint(equalTo: centerXAnchor),
            st.centerYAnchor.constraint(equalTo: centerYAnchor, constant: -30),
        ])
    }
    required init?(coder: NSCoder) { fatalError() }
    override func draw(_ dirtyRect: NSRect) { Theme.content.setFill(); bounds.fill() }
}
