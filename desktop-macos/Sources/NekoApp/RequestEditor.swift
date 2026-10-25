import AppKit
import NekoAPI

// MARK: - Parts

/// The unsaved-changes dot after the request name.
private final class DirtyDot: NSView {
    override var isFlipped: Bool { true }
    init() {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        widthAnchor.constraint(equalToConstant: 8).isActive = true
        heightAnchor.constraint(equalToConstant: 8).isActive = true
        toolTip = "Unsaved changes"
    }
    required init?(coder: NSCoder) { fatalError() }
    override func draw(_ dirtyRect: NSRect) {
        Theme.orange.setFill()
        NSBezierPath(ovalIn: bounds).fill()
    }
}

/// The request name, edited in place: looks like text until hovered or focused.
@MainActor
private final class InlineTitle: Surface, NSTextFieldDelegate {
    let input = CellField()
    var onChange: ((String) -> Void)?
    private var hover = false { didSet { restyle() } }
    private var focused = false { didSet { restyle() } }
    private var widthConstraint: NSLayoutConstraint!

    var text: String {
        get { input.stringValue }
        set { input.stringValue = newValue; fit() }
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        radius = 6
        translatesAutoresizingMaskIntoConstraints = false
        input.isBordered = false
        input.drawsBackground = false
        input.focusRingType = .none
        input.font = Theme.font(.headline)
        input.textColor = Theme.text
        input.placeholderAttributedString = NSAttributedString(string: "Untitled request", attributes: Theme.attrs(.headline, Theme.text3))
        input.cell?.usesSingleLineMode = true
        input.cell?.isScrollable = true
        input.cell?.lineBreakMode = .byTruncatingTail
        input.delegate = self
        input.translatesAutoresizingMaskIntoConstraints = false
        input.onFocus = { [weak self] in self?.focused = $0 }
        addSubview(input)
        widthConstraint = widthAnchor.constraint(equalToConstant: 140)
        widthConstraint.priority = .defaultHigh
        NSLayoutConstraint.activate([
            input.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 7),
            input.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -7),
            input.centerYAnchor.constraint(equalTo: centerYAnchor),
            heightAnchor.constraint(equalToConstant: 28),
            widthConstraint,
            widthAnchor.constraint(lessThanOrEqualToConstant: 420),
        ])
        setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
    }
    required init?(coder: NSCoder) { fatalError() }

    private func fit() {
        let s = input.stringValue.isEmpty ? "Untitled request" : input.stringValue
        let w = (s as NSString).size(withAttributes: Theme.attrs(.headline)).width
        widthConstraint.constant = max(60, ceil(w) + 22)
    }

    private func restyle() {
        fill = focused ? Theme.code : hover ? Theme.fill : nil
        border = focused ? Theme.accent.withAlphaComponent(0.7) : nil
    }

    func controlTextDidChange(_ obj: Notification) {
        fit()
        onChange?(input.stringValue)
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy sel: Selector) -> Bool {
        if sel == #selector(NSResponder.insertNewline(_:)) || sel == #selector(NSResponder.cancelOperation(_:)) {
            window?.makeFirstResponder(nil)
            return true
        }
        return false
    }

    override func mouseEntered(with event: NSEvent) { hover = true }
    override func mouseExited(with event: NSEvent) { hover = false }
    override func mouseDown(with event: NSEvent) { window?.makeFirstResponder(input) }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }
}

/// The address bar's text field. Tints `{{variables}}` and `:path` tokens as it is typed in.
@MainActor
private final class URLField: Surface, NSTextFieldDelegate {
    let input = CellField()
    var known: Set<String> = []
    var onEdit: ((String) -> Void)?
    var onCurl: ((String) -> Void)?
    var onSubmit: (() -> Void)?
    private static let font: NSFont = {
        let f = Theme.font(.mono)
        return NSFont(descriptor: f.fontDescriptor, size: 13) ?? f
    }()

    var isEditing: Bool { input.currentEditor() != nil }
    var text: String { input.stringValue }

    override init(frame: NSRect) {
        super.init(frame: frame)
        fill = Theme.code
        border = Theme.separator
        radius = Theme.Radius.control
        translatesAutoresizingMaskIntoConstraints = false
        input.isBordered = false
        input.drawsBackground = false
        input.focusRingType = .none
        input.font = Self.font
        input.placeholderAttributedString = NSAttributedString(
            string: "Enter URL or paste cURL", attributes: [.font: Self.font, .foregroundColor: Theme.text3])
        input.cell?.usesSingleLineMode = true
        input.cell?.isScrollable = true
        input.cell?.lineBreakMode = .byClipping
        input.delegate = self
        input.translatesAutoresizingMaskIntoConstraints = false
        input.onFocus = { [weak self] on in
            self?.border = on ? Theme.accent.withAlphaComponent(0.7) : Theme.separator
        }
        addSubview(input)
        NSLayoutConstraint.activate([
            input.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            input.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            input.centerYAnchor.constraint(equalTo: centerYAnchor),
            heightAnchor.constraint(equalToConstant: 36),
        ])
    }
    required init?(coder: NSCoder) { fatalError() }

    private var base: [NSAttributedString.Key: Any] { [.font: Self.font, .foregroundColor: Theme.text] }

    private func styled(_ s: String) -> NSAttributedString {
        let a = NSMutableAttributedString(string: s, attributes: base)
        tint(a)
        return a
    }

    private func tint(_ a: NSMutableAttributedString) {
        for t in ComposerEdit.tokens(in: a.string) {
            let color: NSColor
            switch t.kind {
            case .variable: color = ComposerEdit.resolves(t.name, known: known) ? Theme.accent : Theme.orange
            case .pathVariable: color = Theme.teal
            }
            a.addAttributes([.foregroundColor: color, .backgroundColor: Theme.tint(color, 0.14)], range: t.range)
        }
    }

    /// Shows text from the model. While the person types, their text wins unless `force`.
    func set(_ s: String, force: Bool = false) {
        if isEditing {
            guard force, let tv = input.currentEditor() as? NSTextView, tv.string != s else { return }
            tv.string = s
            retint()
        } else {
            input.attributedStringValue = styled(s)
        }
    }

    func retint() {
        if let tv = input.currentEditor() as? NSTextView, let ts = tv.textStorage {
            ts.beginEditing()
            ts.setAttributes(base, range: NSRange(location: 0, length: ts.length))
            let m = NSMutableAttributedString(attributedString: ts)
            tint(m)
            m.enumerateAttributes(in: NSRange(location: 0, length: m.length)) { attrs, r, _ in
                if attrs[.backgroundColor] != nil { ts.addAttributes(attrs, range: r) }
            }
            ts.endEditing()
            tv.typingAttributes = base
        } else {
            input.attributedStringValue = styled(input.stringValue)
        }
    }

    func controlTextDidChange(_ obj: Notification) {
        let t = input.stringValue
        let head = t.trimmingCharacters(in: .whitespacesAndNewlines)
        if head.lowercased().hasPrefix("curl ") {
            onCurl?(head)
            return
        }
        retint()
        onEdit?(t)
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy sel: Selector) -> Bool {
        if sel == #selector(NSResponder.insertNewline(_:)) {
            onSubmit?()
            return true
        }
        return false
    }
}

/// Send with a menu: accent fill, "Sending…" and dimmed while a request is in flight.
@MainActor
private final class SendButton: NSView {
    var busy = false { didSet { if busy != oldValue { needsDisplay = true } } }
    var onSend: (() -> Void)?
    var menuProvider: (() -> NSMenu)?
    private var hover = false, pressed = false
    private let arrow: CGFloat = 30
    override var isFlipped: Bool { true }

    init() {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        widthAnchor.constraint(equalToConstant: 112).isActive = true
        heightAnchor.constraint(equalToConstant: 36).isActive = true
        toolTip = "Send (⌘↩)"
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        let p = NSBezierPath(roundedRect: bounds, xRadius: Theme.Radius.control, yRadius: Theme.Radius.control)
        var bg = Theme.accent
        var fg = Theme.onAccent
        if busy { bg = Theme.fill; fg = Theme.text3 }
        else if pressed { bg = Theme.accent.blended(withFraction: 0.2, of: .black) ?? bg }
        else if hover { bg = Theme.accent.blended(withFraction: 0.1, of: .black) ?? bg }
        bg.setFill(); p.fill()
        let a = Theme.attrs(.bodyMedium, fg)
        let title = (busy ? "Sending…" : "Send") as NSString
        let sz = title.size(withAttributes: a)
        let w = bounds.width - arrow
        title.draw(at: NSPoint(x: w / 2 - sz.width / 2, y: bounds.midY - sz.height / 2), withAttributes: a)
        fg.withAlphaComponent(busy ? 0.3 : 0.35).setFill()
        NSRect(x: w, y: 9, width: 1, height: bounds.height - 18).fill()
        drawSymbol("chevron.down", in: NSRect(x: w, y: 0, width: arrow, height: bounds.height), color: fg, size: 9, weight: .bold)
    }

    override func mouseDown(with event: NSEvent) {
        let x = convert(event.locationInWindow, from: nil).x
        if x > bounds.width - arrow, let m = menuProvider?() {
            m.popUp(positioning: nil, at: NSPoint(x: bounds.width - 170, y: bounds.height + 4), in: self)
            return
        }
        if !busy { pressed = true; needsDisplay = true }
    }
    override func mouseUp(with event: NSEvent) {
        let inside = bounds.contains(convert(event.locationInWindow, from: nil))
        if pressed && inside { onSend?() }
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

/// Postman-style text tabs with a 2 pt accent underline, count badges and a body dot.
private final class EditorTabs: NSView {
    struct Item { var title: String; var count: Int?; var dot = false }
    var items: [Item] { didSet { needsDisplay = true } }
    var selected: Int { didSet { needsDisplay = true } }
    var onChange: ((Int) -> Void)?
    private var hover: Int?
    private let gap: CGFloat = 24, inset: CGFloat = 16
    override var isFlipped: Bool { true }

    init(_ items: [Item], selected: Int) {
        self.items = items
        self.selected = selected
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        heightAnchor.constraint(equalToConstant: 34).isActive = true
    }
    required init?(coder: NSCoder) { fatalError() }

    private func titleAttrs(_ i: Int) -> [NSAttributedString.Key: Any] {
        Theme.attrs(.bodyMedium, i == selected ? Theme.text : i == hover ? Theme.text : Theme.text2)
    }
    private func badgeWidth(_ it: Item) -> CGFloat {
        if let c = it.count, c > 0 { return 5 + ("\(c)" as NSString).size(withAttributes: Theme.attrs(.caption)).width }
        return it.dot ? 12 : 0
    }
    private func spans() -> [(x: CGFloat, w: CGFloat)] {
        var x = inset
        return items.indices.map { i in
            let w = (items[i].title as NSString).size(withAttributes: titleAttrs(i)).width + badgeWidth(items[i])
            defer { x += w + gap }
            return (x, w)
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        Theme.separator.setFill()
        NSRect(x: 0, y: bounds.height - 1, width: bounds.width, height: 1).fill()
        for (i, sp) in spans().enumerated() {
            let it = items[i]
            let a = titleAttrs(i)
            let sz = (it.title as NSString).size(withAttributes: a)
            let ty = bounds.midY - sz.height / 2 - 1
            (it.title as NSString).draw(at: NSPoint(x: sp.x, y: ty), withAttributes: a)
            let tw = sz.width
            if let c = it.count, c > 0 {
                let ca = Theme.attrs(.caption, Theme.green)
                let cs = ("\(c)" as NSString).size(withAttributes: ca)
                ("\(c)" as NSString).draw(at: NSPoint(x: sp.x + tw + 5, y: ty + (sz.height - cs.height) / 2 - 1), withAttributes: ca)
            } else if it.dot {
                Theme.green.setFill()
                NSBezierPath(ovalIn: NSRect(x: sp.x + tw + 6, y: bounds.midY - 4, width: 6, height: 6)).fill()
            }
            if i == selected {
                Theme.accent.setFill()
                NSBezierPath(roundedRect: NSRect(x: sp.x - 2, y: bounds.height - 2, width: sp.w + 4, height: 2), xRadius: 1, yRadius: 1).fill()
            }
        }
    }

    private func index(at p: NSPoint) -> Int? {
        for (i, sp) in spans().enumerated() where p.x >= sp.x - gap / 2 && p.x <= sp.x + sp.w + gap / 2 { return i }
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
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }
}

/// "Auto-generated headers (n)": collapsed by default, read-only rows when open.
@MainActor
private final class AutoHeaders: NSStackView {
    private let toggle = NSView()
    private let title = label("", .bodyMedium, Theme.text2)
    private let chevron = NSImageView()
    private var cardHolder = NSStackView()
    private var headers: [Neko_Api_V1_Header] = []
    var expanded = false { didSet { rebuild() } }

    override init(frame: NSRect) {
        super.init(frame: frame)
        orientation = .vertical
        alignment = .leading
        spacing = 8
        translatesAutoresizingMaskIntoConstraints = false
        chevron.contentTintColor = Theme.text3
        let row = NSStackView.horizontal([chevron, title], spacing: 6)
        let click = NSClickGestureRecognizer(target: self, action: #selector(clicked))
        row.addGestureRecognizer(click)
        addArrangedSubview(row)
        cardHolder.orientation = .vertical
        cardHolder.translatesAutoresizingMaskIntoConstraints = false
        addArrangedSubview(cardHolder)
        cardHolder.leadingAnchor.constraint(equalTo: leadingAnchor).isActive = true
        cardHolder.trailingAnchor.constraint(equalTo: trailingAnchor).isActive = true
        isHidden = true
    }
    required init?(coder: NSCoder) { fatalError() }

    @objc private func clicked() { expanded.toggle() }

    func set(_ headers: [Neko_Api_V1_Header]) {
        self.headers = headers
        isHidden = headers.isEmpty
        rebuild()
    }

    private func rebuild() {
        title.set("Auto-generated headers (\(headers.count))", .bodyMedium, Theme.text2)
        chevron.image = symbol(expanded ? "chevron.down" : "chevron.right", size: 10, weight: .semibold)
        cardHolder.arrangedSubviews.forEach { cardHolder.removeArrangedSubview($0); $0.removeFromSuperview() }
        cardHolder.isHidden = !expanded
        guard expanded else { return }
        let rows: [NSView] = headers.map { h in
            let n = label(h.name, .mono, Theme.text2)
            n.widthAnchor.constraint(equalToConstant: 170).isActive = true
            let v = label(h.value, .mono, Theme.text3)
            v.lineBreakMode = .byTruncatingTail
            v.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
            let tag = label("added when sent", .caption, Theme.text3)
            tag.setContentCompressionResistancePriority(.required, for: .horizontal)
            let r = NSStackView.horizontal([n, v, NSView(), tag], spacing: 10)
            return r
        }
        let card = groupCard(rows, padding: NSEdgeInsets(top: 7, left: 12, bottom: 7, right: 12))
        cardHolder.addArrangedSubview(card)
        card.leadingAnchor.constraint(equalTo: cardHolder.leadingAnchor).isActive = true
        card.trailingAnchor.constraint(equalTo: cardHolder.trailingAnchor).isActive = true
    }
}

// MARK: - The editor

/// The request half of the composer: name and Save, the address bar, and the
/// section tabs (Params, Authorization, Headers, Body, Tests). The header stays
/// put; only the section content scrolls.
@MainActor
final class RequestEditor: NSView {
    let model: ComposerModel
    var onSend: (() -> Void)?
    override var isFlipped: Bool { true }

    private let crumbs = label("", .body, Theme.text3)
    private let title = InlineTitle()
    private let dot = DirtyDot()
    private let saveChip = Chip("Save", style: .plain)
    private let codeButton = IconButton("chevron.left.forwardslash.chevron.right", tip: "Code snippet")
    private let moreButton = IconButton("ellipsis", tip: "More")
    private let method = MethodPicker("GET", height: 36)
    private let urlField = URLField()
    private let sendButton = SendButton()
    private let preview = label("", .monoSmall, Theme.text3)
    private let tabs = EditorTabs([
        .init(title: "Params"), .init(title: "Authorization"), .init(title: "Headers"), .init(title: "Body"), .init(title: "Tests"),
    ], selected: 0)
    private let scroll = NSScrollView()
    private let doc = FlippedView()
    private let content = NSStackView.vertical([], spacing: 18)

    private var section = 0
    private var paramsTable: KeyValueTable?
    private var pathTable: KeyValueTable?
    private var headersTable: KeyValueTable?
    private var autoHeaders: AutoHeaders?
    private var autoExpanded = false
    private var resolved: Neko_Api_V1_ComposerResolved?
    private var resolveTask: Task<Void, Never>?

    init(model: ComposerModel) {
        self.model = model
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        build()
        rebuild()
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) { Theme.content.setFill(); bounds.fill() }

    // MARK: layout

    private func build() {
        crumbs.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        crumbs.setContentHuggingPriority(.required, for: .horizontal)
        let spacer = NSView()
        spacer.setContentHuggingPriority(.init(1), for: .horizontal)
        spacer.setContentCompressionResistancePriority(.init(1), for: .horizontal)
        let top = NSStackView.horizontal([crumbs, title, dot, spacer, saveChip, codeButton, moreButton], spacing: 6)
        top.setCustomSpacing(8, after: dot)
        top.setCustomSpacing(10, after: spacer)
        top.setCustomSpacing(2, after: crumbs)
        top.setCustomSpacing(8, after: saveChip)

        let address = NSStackView.horizontal([method, urlField, sendButton], spacing: 8)
        urlField.setContentHuggingPriority(.init(1), for: .horizontal)

        preview.lineBreakMode = .byTruncatingMiddle
        preview.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        doc.translatesAutoresizingMaskIntoConstraints = false
        scroll.documentView = doc
        content.translatesAutoresizingMaskIntoConstraints = false
        doc.addSubview(content)

        for v in [top, address, preview, tabs, scroll] as [NSView] { addSubview(v) }
        let clip = scroll.contentView
        NSLayoutConstraint.activate([
            top.topAnchor.constraint(equalTo: topAnchor, constant: 10),
            top.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            top.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            top.heightAnchor.constraint(equalToConstant: 30),
            address.topAnchor.constraint(equalTo: top.bottomAnchor, constant: 8),
            address.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            address.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            preview.topAnchor.constraint(equalTo: address.bottomAnchor, constant: 6),
            preview.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 18),
            preview.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            preview.heightAnchor.constraint(equalToConstant: 15),
            tabs.topAnchor.constraint(equalTo: preview.bottomAnchor, constant: 4),
            tabs.leadingAnchor.constraint(equalTo: leadingAnchor),
            tabs.trailingAnchor.constraint(equalTo: trailingAnchor),
            scroll.topAnchor.constraint(equalTo: tabs.bottomAnchor),
            scroll.leadingAnchor.constraint(equalTo: leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: trailingAnchor),
            scroll.bottomAnchor.constraint(equalTo: bottomAnchor),
            doc.topAnchor.constraint(equalTo: clip.topAnchor),
            doc.leadingAnchor.constraint(equalTo: clip.leadingAnchor),
            doc.trailingAnchor.constraint(equalTo: clip.trailingAnchor),
            content.topAnchor.constraint(equalTo: doc.topAnchor, constant: 14),
            content.bottomAnchor.constraint(equalTo: doc.bottomAnchor, constant: -24),
            content.leadingAnchor.constraint(equalTo: doc.leadingAnchor, constant: 16),
            content.trailingAnchor.constraint(equalTo: doc.trailingAnchor, constant: -16),
        ])

        saveChip.onClick = { [weak self] in self?.save() }
        codeButton.onClick = { [weak self] in self?.presentCode() }
        moreButton.onClick = { [weak self] in self?.showMore() }
        title.onChange = { [weak self] t in self?.model.edit { $0.name = t } }
        method.onPick = { [weak self] m in self?.model.edit { $0.method = m } }
        urlField.onEdit = { [weak self] t in self?.model.urlEdited(t) }
        urlField.onSubmit = { [weak self] in self?.send() }
        urlField.onCurl = { [weak self] t in self?.pastedCurl(t) }
        sendButton.onSend = { [weak self] in self?.send() }
        sendButton.menuProvider = { [weak self] in
            let m = ActionMenu()
            m.add("Send") { self?.send() }
            m.separator()
            m.add("Copy as cURL") { self?.copyCurl() }
            m.add("Generate code…") { self?.presentCode() }
            return m
        }
        tabs.onChange = { [weak self] i in
            guard let self else { return }
            self.section = i
            self.model.tab.section = i
            self.showSection(keepScroll: false)
        }
    }

    // MARK: API for the shell

    /// The active tab changed or its draft was replaced: rebuild everything.
    func rebuild() {
        section = min(max(model.tab.section, 0), 4)
        tabs.selected = section
        resolved = nil
        urlField.known = known()
        urlField.set(model.tab.display, force: true)
        method.method = model.draft.method.isEmpty ? "GET" : model.draft.method
        title.text = model.draft.name
        reloadChrome()
        showSection(keepScroll: false)
        applyResolved()
    }

    /// Counts, name, dirty mark and the resolved-URL preview changed.
    func reloadChrome() {
        let d = model.draft
        var parts = d.collection.split(separator: "/").map(String.init)
        if !parts.isEmpty { parts[parts.count - 1] += " /" }
        crumbs.set(parts.joined(separator: " / "), .body, Theme.text3)
        crumbs.isHidden = parts.isEmpty
        if title.input.currentEditor() == nil, title.text != d.name { title.text = d.name }
        dot.isHidden = !model.tab.dirty
        saveChip.style = model.tab.dirty ? .accent : .plain
        let m = d.method.isEmpty ? "GET" : d.method
        if method.method != m { method.method = m }
        sendButton.busy = model.tab.sending
        tabs.items = [
            .init(title: "Params", count: ComposerEdit.enabledCount(d.params)),
            .init(title: "Authorization"),
            .init(title: "Headers", count: ComposerEdit.enabledCount(d.headers)),
            .init(title: "Body", dot: d.bodyKind != .none),
            .init(title: "Tests", count: d.tests.filter { !$0.disabled }.count),
        ]
        let k = known()
        if k != urlField.known { urlField.known = k; urlField.retint() }
        scheduleResolve()
    }

    /// The URL text of `tab` changed because its params changed.
    func displayChanged(_ tab: RequestTab) {
        urlField.known = known()
        urlField.set(tab.display)
    }

    /// The core rewrote the params or path variables of `tab`.
    func paramsChanged(_ tab: RequestTab) {
        guard section == 0 else { return }
        if paramsTable?.hasFocus == true || pathTable?.hasFocus == true { return }
        if (pathTable == nil) != tab.draft.pathVars.isEmpty {
            showSection(keepScroll: true)
        } else {
            paramsTable?.setRows(tab.draft.params)
            pathTable?.setRows(tab.draft.pathVars)
        }
    }

    func focusURL() { window?.makeFirstResponder(urlField.input) }

    func save() {
        if model.draft.id.isEmpty { presentSaveSheet(); return }
        Task { [weak self] in
            guard let self else { return }
            if let e = await self.model.save() { tell(self.window, "Save", e) }
        }
    }

    func copyCurl() {
        Task { [weak self] in
            guard let self else { return }
            let r = await self.model.resolve()
            guard let r, !r.curl.isEmpty else {
                tell(self.window, "Copy cURL", r?.error.isEmpty == false ? r!.error : "The request can't be built yet.")
                return
            }
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(r.curl, forType: .string)
        }
    }

    func presentCode() { CodeSnippetSheet.present(model: model, from: window) }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        if flags == .command, event.keyCode == 36 || event.keyCode == 76, window != nil, !isHiddenOrHasHiddenAncestor {
            send()
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

    // MARK: actions

    private func send() {
        onSend?()
        DispatchQueue.main.async { [weak self] in self?.sendButton.busy = self?.model.tab.sending ?? false }
    }

    private func pastedCurl(_ text: String) {
        Task { [weak self] in
            guard let self else { return }
            if let e = await self.model.importCurl(text) {
                self.urlField.set(self.model.tab.display, force: true)
                tell(self.window, "Import cURL", e)
            }
        }
    }

    private func showMore() {
        let m = ActionMenu()
        m.add("Duplicate") { [weak self] in
            guard let self else { return }
            var r = self.model.draft
            r.id = ""
            if !r.name.isEmpty { r.name += " copy" }
            self.model.newTab(r)
        }
        m.add("Copy cURL") { [weak self] in self?.copyCurl() }
        m.add("Generate code…") { [weak self] in self?.presentCode() }
        m.separator()
        m.add("Save as…") { [weak self] in self?.presentSaveSheet() }
        moreButton.popMenu(m)
    }

    private func presentSaveSheet() {
        let a = NSAlert()
        a.messageText = "Save request"
        a.informativeText = "Name it and pick a collection or folder, such as Shop API/Orders."
        a.addButton(withTitle: "Save")
        a.addButton(withTitle: "Cancel")
        let box = NSView(frame: NSRect(x: 0, y: 0, width: 300, height: 58))
        let name = NSTextField(frame: NSRect(x: 0, y: 34, width: 300, height: 24))
        name.stringValue = model.draft.name
        name.placeholderString = "Request name"
        let combo = NSComboBox(frame: NSRect(x: 0, y: 0, width: 300, height: 26))
        combo.addItems(withObjectValues: model.state.collections)
        combo.placeholderString = "Collection (optional)"
        combo.stringValue = model.draft.collection
        combo.completes = true
        box.addSubview(name); box.addSubview(combo)
        a.accessoryView = box
        a.window.initialFirstResponder = name
        let handle: (NSApplication.ModalResponse) -> Void = { [weak self] r in
            guard r == .alertFirstButtonReturn, let self else { return }
            let n = name.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
            let c = combo.stringValue.trimmingCharacters(in: CharacterSet(charactersIn: "/ ").union(.whitespacesAndNewlines))
            Task {
                if let e = await self.model.save(name: n.isEmpty ? "Untitled request" : n, collection: c) { tell(self.window, "Save", e) }
            }
        }
        if let w = window { a.beginSheetModal(for: w, completionHandler: handle) } else { handle(a.runModal()) }
    }

    // MARK: resolve preview

    private func known() -> Set<String> {
        ComposerEdit.knownVariables(model.state, collection: model.draft.collection)
    }

    private func scheduleResolve() {
        resolveTask?.cancel()
        resolveTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 300_000_000)
            guard !Task.isCancelled, let self else { return }
            let r = await self.model.resolve()
            guard !Task.isCancelled else { return }
            self.resolved = r
            self.applyResolved()
        }
    }

    private func applyResolved() {
        if let r = resolved, !model.draft.url.isEmpty {
            if !r.missing.isEmpty {
                let names = r.missing.map { $0.hasPrefix("{{") ? $0 : "{{\($0)}}" }.joined(separator: ", ")
                preview.set("No value for \(names)", .monoSmall, Theme.orange)
            } else if !r.error.isEmpty {
                preview.set(r.error, .monoSmall, Theme.red)
            } else {
                preview.set(r.url, .monoSmall, Theme.text3)
            }
        } else {
            preview.set("", .monoSmall, Theme.text3)
        }
        if let r = resolved {
            let names = Set(r.generated.map { $0.lowercased() })
            autoHeaders?.set(r.headers.filter { names.contains($0.name.lowercased()) })
        } else {
            autoHeaders?.set([])
        }
    }

    // MARK: sections

    private func showSection(keepScroll: Bool) {
        let y = scroll.contentView.bounds.origin.y
        content.arrangedSubviews.forEach { content.removeArrangedSubview($0); $0.removeFromSuperview() }
        paramsTable = nil; pathTable = nil; headersTable = nil; autoHeaders = nil
        let views: [NSView]
        switch section {
        case 0: views = paramsViews()
        case 1: views = authViews()
        case 2: views = headerViews()
        case 3: views = [hosted(BodyEditor(model: model), minHeight: 240)]
        default: views = [hosted(TestsEditor(model: model), minHeight: 80)]
        }
        for v in views {
            content.addArrangedSubview(v)
            v.leadingAnchor.constraint(equalTo: content.leadingAnchor).isActive = true
            v.trailingAnchor.constraint(equalTo: content.trailingAnchor).isActive = true
            if section >= 3 {
                // Body and Tests scroll themselves: give them the visible height.
                let fill = v.heightAnchor.constraint(equalTo: scroll.contentView.heightAnchor, constant: -16)
                fill.priority = .defaultHigh
                fill.isActive = true
            }
        }
        layoutSubtreeIfNeeded()
        scroll.contentView.scroll(to: NSPoint(x: 0, y: keepScroll ? y : 0))
        scroll.reflectScrolledClipView(scroll.contentView)
        if section == 2 { applyResolved() }
    }

    private func hosted(_ v: NSView, minHeight: CGFloat) -> NSView {
        v.translatesAutoresizingMaskIntoConstraints = false
        v.heightAnchor.constraint(greaterThanOrEqualToConstant: minHeight).isActive = true
        return v
    }

    private func bulk(_ t: KeyValueTable) {
        t.enableBulkEdit(
            format: { [weak self] rows in await self?.model.bulkFormat(rows) ?? "" },
            parse: { [weak self] text in await self?.model.bulkParse(text) ?? [] })
    }

    private func paramsViews() -> [NSView] {
        let d = model.draft
        let t = KeyValueTable(d.params, description: true, title: "Query Params")
        t.onChange = { [weak self] rows in self?.model.paramsEdited(rows) }
        bulk(t)
        paramsTable = t
        var out: [NSView] = [t]
        if !d.pathVars.isEmpty {
            let p = KeyValueTable(d.pathVars, description: true, title: "Path Variables", fixed: true)
            p.onChange = { [weak self] rows in self?.model.paramsEdited(pathVars: rows) }
            pathTable = p
            out.append(p)
        }
        return out
    }

    private func headerViews() -> [NSView] {
        let t = KeyValueTable(model.draft.headers, key: "Key", value: "Value", description: true, title: "Headers")
        t.onChange = { [weak self] rows in self?.model.edit { $0.headers = rows } }
        t.keySuggestions = { ComposerEdit.headerSuggestions(prefix: $0) }
        bulk(t)
        headersTable = t
        let auto = AutoHeaders()
        auto.expanded = autoExpanded
        autoHeaders = auto
        return [t, auto]
    }

    // MARK: authorization

    private static let authKinds: [(Neko_Api_V1_ComposeAuth, String)] = [
        (.inherit, "Inherit auth from parent"), (.none, "No Auth"), (.apiKey, "API Key"), (.bearer, "Bearer Token"), (.basic, "Basic Auth"),
    ]

    private func authViews() -> [NSView] {
        let d = model.draft
        let kinds = Self.authKinds
        let pill = PillMenu(kinds.map(\.1), selected: kinds.firstIndex { $0.0 == d.auth } ?? 0, height: 30)
        pill.onPick = { [weak self] i in
            self?.model.edit { $0.auth = kinds[i].0 }
            self?.showSection(keepScroll: true)
        }
        let head = NSStackView.horizontal([label("Auth type", .bodyMedium, Theme.text), pill, NSView()], spacing: 12)

        func row(_ title: String, _ field: NSView) -> NSView {
            let l = label(title, .body, Theme.text2)
            l.widthAnchor.constraint(equalToConstant: 110).isActive = true
            l.setContentHuggingPriority(.required, for: .horizontal)
            field.setContentHuggingPriority(.init(1), for: .horizontal)
            let r = NSStackView.horizontal([l, field], spacing: 12)
            r.distribution = .fill
            return r
        }
        func field(_ text: String, _ ph: String, _ set: @escaping (inout ComposedRequest, String) -> Void) -> Field {
            let f = Field(text, placeholder: ph)
            f.onChange = { [weak self] t in self?.model.edit { set(&$0, t) } }
            return f
        }
        func note(_ s: String) -> NSView { label(s, .callout, Theme.text2, lines: 0) }

        let card: NSView
        switch d.auth {
        case .bearer:
            card = groupCard([
                row("Token", field(d.authToken, "{{token}}") { $0.authToken = $1 }),
                row("Prefix", field(d.authPrefix, "Bearer") { $0.authPrefix = $1 }),
            ], padding: NSEdgeInsets(top: 10, left: 16, bottom: 10, right: 16))
        case .basic:
            let pw = SecretField(d.authPassword, placeholder: "Password")
            pw.onChange = { [weak self] t in self?.model.edit { $0.authPassword = t } }
            card = groupCard([
                row("Username", field(d.authUser, "Username") { $0.authUser = $1 }),
                row("Password", pw),
            ], padding: NSEdgeInsets(top: 10, left: 16, bottom: 10, right: 16))
        case .apiKey:
            let seg = Segmented(["Header", "Query Params"], selected: d.authInQuery ? 1 : 0)
            seg.padding = 12
            seg.onChange = { [weak self] i in self?.model.edit { $0.authInQuery = i == 1 } }
            let segRow = NSStackView.horizontal([seg, NSView()], spacing: 0)
            card = groupCard([
                row("Key", field(d.authKey, "X-API-Key") { $0.authKey = $1 }),
                row("Value", field(d.authValue, "{{api_key}}") { $0.authValue = $1 }),
                row("Add to", segRow),
            ], padding: NSEdgeInsets(top: 10, left: 16, bottom: 10, right: 16))
        case .inherit:
            let icon = NSImageView()
            icon.image = symbol("arrow.turn.up.right", size: 13, weight: .medium)
            icon.contentTintColor = Theme.text3
            let summary = ComposerEdit.inheritSummary(collection: d.collection, configs: model.state.configs)
            let text = NSStackView.vertical([label(summary, .bodyMedium, Theme.text)], spacing: 3)
            if !d.collection.isEmpty {
                text.addArrangedSubview(label("Change it in the settings of the collection or folder.", .caption, Theme.text3))
            }
            card = groupCard([NSStackView.horizontal([icon, text, NSView()], spacing: 10)])
        default:
            card = groupCard([note("This request is sent without an Authorization header.")])
        }
        let hint = label("The header or query parameter is added when the request is sent, not to the Headers table.", .caption, Theme.text3)
        var out: [NSView] = [head, card]
        if d.auth == .bearer || d.auth == .basic || d.auth == .apiKey { out.append(hint) }
        return out
    }
}

private extension IconButton {
    /// Pops a menu under the button.
    func popMenu(_ m: NSMenu) {
        m.popUp(positioning: nil, at: NSPoint(x: 0, y: bounds.height + 3), in: self)
    }
}
