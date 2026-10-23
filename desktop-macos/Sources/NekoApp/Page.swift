import AppKit

/// A scrolling page with one centered column, as used by Rules, Devices and Settings.
@MainActor
class Page: NSView {
    let scroll = NSScrollView()
    let doc = FlippedView()
    let column = NSStackView()
    override var isFlipped: Bool { true }

    init(maxWidth: CGFloat) {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        doc.translatesAutoresizingMaskIntoConstraints = false
        scroll.documentView = doc
        column.orientation = .vertical
        column.alignment = .leading
        column.spacing = 14
        column.translatesAutoresizingMaskIntoConstraints = false
        doc.addSubview(column)
        addSubview(scroll)
        scroll.pin(to: self)
        let clip = scroll.contentView
        let w = column.widthAnchor.constraint(equalToConstant: maxWidth)
        w.priority = .defaultHigh
        NSLayoutConstraint.activate([
            doc.topAnchor.constraint(equalTo: clip.topAnchor),
            doc.leadingAnchor.constraint(equalTo: clip.leadingAnchor),
            doc.trailingAnchor.constraint(equalTo: clip.trailingAnchor),
            column.topAnchor.constraint(equalTo: doc.topAnchor, constant: 28),
            column.bottomAnchor.constraint(equalTo: doc.bottomAnchor, constant: -40),
            column.centerXAnchor.constraint(equalTo: doc.centerXAnchor),
            column.leadingAnchor.constraint(greaterThanOrEqualTo: doc.leadingAnchor, constant: 24),
            column.trailingAnchor.constraint(lessThanOrEqualTo: doc.trailingAnchor, constant: -24),
            w,
        ])
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) { Theme.grouped.setFill(); bounds.fill() }

    /// Replaces the page content, keeping the scroll position.
    func setContent(_ views: [NSView]) {
        let y = scroll.contentView.bounds.origin.y
        column.arrangedSubviews.forEach { column.removeArrangedSubview($0); $0.removeFromSuperview() }
        for v in views {
            column.addArrangedSubview(v)
            v.widthAnchor.constraint(equalTo: column.widthAnchor).isActive = true
        }
        layoutSubtreeIfNeeded()
        scroll.contentView.scroll(to: NSPoint(x: 0, y: y))
        scroll.reflectScrolledClipView(scroll.contentView)
    }
}

/// Rows in a rounded card with hairlines between them.
func groupCard(_ rows: [NSView], border: NSColor? = nil, padding: NSEdgeInsets = NSEdgeInsets(top: 14, left: 16, bottom: 14, right: 16)) -> NSView {
    let c = Surface()
    c.fill = Theme.card
    c.border = border ?? Theme.separator
    c.radius = Theme.Radius.card
    c.translatesAutoresizingMaskIntoConstraints = false
    var parts: [NSView] = []
    for (i, r) in rows.enumerated() {
        let box = FlippedView()
        box.translatesAutoresizingMaskIntoConstraints = false
        box.addSubview(r)
        r.pin(to: box, insets: padding)
        parts.append(box)
        if i < rows.count - 1 { parts.append(Divider()) }
    }
    let st = NSStackView.vertical(parts, spacing: 0)
    c.addSubview(st)
    st.pin(to: c, insets: NSEdgeInsets(top: 1, left: 1, bottom: 1, right: 1))
    for v in parts { v.leadingAnchor.constraint(equalTo: st.leadingAnchor).isActive = true; v.trailingAnchor.constraint(equalTo: st.trailingAnchor).isActive = true }
    return c
}
