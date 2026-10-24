import AppKit
import NekoAPI

/// The "AI client" card: a change an AI client wants to make, waiting for a person.
@MainActor
final class ApprovalCard: NSView {
    var onDecide: ((String, Bool) -> Void)?
    private var expanded = false
    private var current: Approval?
    override var isFlipped: Bool { true }

    override init(frame: NSRect) {
        super.init(frame: frame)
        translatesAutoresizingMaskIntoConstraints = false
        isHidden = true
        wantsLayer = true
        shadow = NSShadow()
        layer?.shadowOpacity = 0.35
        layer?.shadowRadius = 14
        layer?.shadowOffset = CGSize(width: 0, height: -4)
        layer?.shadowColor = NSColor.black.cgColor
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        let p = NSBezierPath(roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), xRadius: 12, yRadius: 12)
        Theme.card.setFill(); p.fill()
        Theme.accent.setStroke(); p.lineWidth = 1; p.stroke()
    }

    func update(_ list: [Approval]) {
        guard let a = list.first else {
            isHidden = true
            current = nil
            return
        }
        if a.id != current?.id { expanded = false }
        current = a
        isHidden = false
        subviews.forEach { $0.removeFromSuperview() }

        let badge = Badge("AI client", color: Theme.accent)
        var top: [NSView] = [badge, NSView()]
        if list.count > 1 { top.append(label("1 of \(list.count)", .callout, Theme.text3)) }
        let title = label(a.title, .headline, Theme.text, lines: 3)
        title.preferredMaxLayoutWidth = 390
        let toggle = DisclosureLine("Exact request", open: expanded)
        toggle.onToggle = { [weak self] in
            guard let self else { return }
            self.expanded.toggle()
            self.update(list)
        }
        var rows: [NSView] = [NSStackView.horizontal(top, spacing: 8), title, toggle]
        if expanded {
            let detail = label(a.detail, .monoSmall, Theme.text2, lines: 0)
            detail.lineBreakMode = .byCharWrapping
            detail.isSelectable = true
            detail.preferredMaxLayoutWidth = 390
            let box = Surface()
            box.fill = Theme.code; box.radius = 6
            box.translatesAutoresizingMaskIntoConstraints = false
            box.addSubview(detail)
            detail.pin(to: box, insets: NSEdgeInsets(top: 8, left: 10, bottom: 8, right: 10))
            rows.append(box)
        }
        let allow = Chip("Allow", symbol: "checkmark", style: .accent)
        allow.onClick = { [weak self] in self?.onDecide?(a.id, true) }
        let deny = Chip("Deny")
        deny.onClick = { [weak self] in self?.onDecide?(a.id, false) }
        rows.append(NSStackView.horizontal([allow, deny, NSView()], spacing: 8))

        let st = NSStackView.vertical(rows, spacing: 10)
        st.alignment = .leading
        addSubview(st)
        st.pin(to: self, insets: NSEdgeInsets(top: 16, left: 16, bottom: 16, right: 16))
        for v in rows { v.widthAnchor.constraint(equalTo: st.widthAnchor).isActive = true }
        needsDisplay = true
    }
}

/// "▸ Exact request": a small clickable disclosure line.
final class DisclosureLine: NSView {
    private let title: String
    private var open: Bool
    var onToggle: (() -> Void)?
    override var isFlipped: Bool { true }

    init(_ title: String, open: Bool) {
        self.title = title; self.open = open
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        heightAnchor.constraint(equalToConstant: 18).isActive = true
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        drawSymbol(open ? "chevron.down" : "chevron.right", in: NSRect(x: 0, y: 0, width: 12, height: bounds.height), color: Theme.text2, size: 8, weight: .bold)
        let a = Theme.attrs(.callout, Theme.text2)
        let sz = (title as NSString).size(withAttributes: a)
        (title as NSString).draw(at: NSPoint(x: 16, y: bounds.midY - sz.height / 2), withAttributes: a)
    }
    override func mouseUp(with event: NSEvent) { onToggle?() }
}
