import AppKit
import NekoAPI

struct PaletteItem {
    var title: String
    var detail: String = ""
    var symbol: String?
    var run: () -> Void
}

/// ⌘⇧P: type to run a command or jump to a request. Esc or a click outside closes it.
@MainActor
final class CommandPalette: NSView, NSTextFieldDelegate {
    private let scrim = Surface()
    private let card = Surface()
    private let input = NSTextField()
    private let rowsStack = NSStackView()
    private var shown: [PaletteItem] = []
    private var selected = 0
    private let maxRows = 9
    /// Commands are fixed; flows are looked up as the person types.
    var commands: () -> [PaletteItem] = { [] }
    var flows: (String) -> [PaletteItem] = { _ in [] }
    var onClose: (() -> Void)?
    override var isFlipped: Bool { true }

    init() {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        scrim.fill = NSColor.black.withAlphaComponent(0.28)
        scrim.translatesAutoresizingMaskIntoConstraints = false
        addSubview(scrim)
        scrim.pin(to: self)

        card.fill = Theme.card; card.border = Theme.separator; card.radius = Theme.Radius.sheet
        card.translatesAutoresizingMaskIntoConstraints = false
        card.shadow = {
            let s = NSShadow(); s.shadowColor = NSColor.black.withAlphaComponent(0.3)
            s.shadowBlurRadius = 24; s.shadowOffset = NSSize(width: 0, height: -8); return s
        }()
        addSubview(card)

        input.isBordered = false; input.drawsBackground = false; input.focusRingType = .none
        input.font = NSFont.systemFont(ofSize: 17)
        input.textColor = Theme.text
        input.placeholderAttributedString = NSAttributedString(string: "Run a command or find a request…", attributes: [.font: NSFont.systemFont(ofSize: 17), .foregroundColor: Theme.text3])
        input.delegate = self
        input.translatesAutoresizingMaskIntoConstraints = false

        rowsStack.orientation = .vertical; rowsStack.spacing = 2; rowsStack.alignment = .leading
        rowsStack.translatesAutoresizingMaskIntoConstraints = false
        let divider = Divider(vertical: false)
        card.addSubview(input); card.addSubview(divider); card.addSubview(rowsStack)
        NSLayoutConstraint.activate([
            card.centerXAnchor.constraint(equalTo: centerXAnchor),
            card.topAnchor.constraint(equalTo: topAnchor, constant: 90),
            card.widthAnchor.constraint(equalToConstant: 580),
            input.topAnchor.constraint(equalTo: card.topAnchor, constant: 14),
            input.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: 18),
            input.trailingAnchor.constraint(equalTo: card.trailingAnchor, constant: -18),
            input.heightAnchor.constraint(equalToConstant: 26),
            divider.topAnchor.constraint(equalTo: input.bottomAnchor, constant: 12),
            divider.leadingAnchor.constraint(equalTo: card.leadingAnchor),
            divider.trailingAnchor.constraint(equalTo: card.trailingAnchor),
            rowsStack.topAnchor.constraint(equalTo: divider.bottomAnchor, constant: 6),
            rowsStack.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: 8),
            rowsStack.trailingAnchor.constraint(equalTo: card.trailingAnchor, constant: -8),
            rowsStack.bottomAnchor.constraint(equalTo: card.bottomAnchor, constant: -8),
        ])
    }
    required init?(coder: NSCoder) { fatalError() }

    func open(in host: NSView) {
        host.addSubview(self)
        pin(to: host)
        input.stringValue = ""
        selected = 0
        refresh()
        window?.makeFirstResponder(input)
    }

    func close() {
        removeFromSuperview()
        onClose?()
    }

    // Clicks outside the card dismiss.
    override func mouseDown(with event: NSEvent) {
        if !card.frame.contains(convert(event.locationInWindow, from: nil)) { close() }
    }

    func controlTextDidChange(_ obj: Notification) { selected = 0; refresh() }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy sel: Selector) -> Bool {
        switch sel {
        case #selector(NSResponder.moveDown(_:)): move(1); return true
        case #selector(NSResponder.moveUp(_:)): move(-1); return true
        case #selector(NSResponder.insertNewline(_:)): runSelected(); return true
        case #selector(NSResponder.cancelOperation(_:)): close(); return true
        default: return false
        }
    }

    private func move(_ d: Int) {
        guard !shown.isEmpty else { return }
        selected = (selected + d + shown.count) % shown.count
        paintRows()
    }

    private func runSelected() {
        guard shown.indices.contains(selected) else { return }
        let item = shown[selected]
        close()
        item.run()
    }

    private func refresh() {
        let q = input.stringValue.trimmingCharacters(in: .whitespaces)
        var scored: [(Int, PaletteItem)] = []
        for c in commands() {
            if let s = fuzzyScore(q, c.title) { scored.append((s, c)) }
        }
        scored.sort { $0.0 > $1.0 }
        var items = scored.map(\.1)
        if q.count >= 2 { items += flows(q) }
        shown = Array(items.prefix(maxRows))
        paintRows()
    }

    private func paintRows() {
        rowsStack.arrangedSubviews.forEach { $0.removeFromSuperview() }
        if shown.isEmpty {
            let l = label("Nothing matches", .callout, Theme.text3)
            rowsStack.addArrangedSubview(l)
            l.leadingAnchor.constraint(equalTo: rowsStack.leadingAnchor, constant: 10).isActive = true
            return
        }
        for (i, item) in shown.enumerated() {
            let row = Surface()
            row.radius = Theme.Radius.control
            row.fill = i == selected ? Theme.accent.withAlphaComponent(0.16) : nil
            row.translatesAutoresizingMaskIntoConstraints = false
            let title = label(item.title, .body, Theme.text)
            let detail = label(item.detail, .callout, Theme.text3)
            detail.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
            title.setContentCompressionResistancePriority(.defaultHigh, for: .horizontal)
            let st = NSStackView.horizontal([title, NSView(), detail], spacing: 10)
            row.addSubview(st)
            st.pin(to: row, insets: NSEdgeInsets(top: 7, left: 10, bottom: 7, right: 10))
            rowsStack.addArrangedSubview(row)
            row.widthAnchor.constraint(equalTo: rowsStack.widthAnchor).isActive = true
            let click = NSClickGestureRecognizer(target: self, action: #selector(rowClicked(_:)))
            row.addGestureRecognizer(click)
            row.identifier = NSUserInterfaceItemIdentifier(String(i))
        }
    }

    @objc private func rowClicked(_ g: NSClickGestureRecognizer) {
        guard let id = g.view?.identifier?.rawValue, let i = Int(id) else { return }
        selected = i
        runSelected()
    }
}
