import AppKit
import NekoAPI

/// The language list in the snippet sheet: one row per language, arrow keys move.
private final class LanguageList: NSView {
    var items: [CodeLanguage] = [] { didSet { needsDisplay = true } }
    var selected = 0 { didSet { needsDisplay = true } }
    var onSelect: ((Int) -> Void)?
    private var hover: Int?
    private let rowH: CGFloat = 30
    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }
    override var focusRingType: NSFocusRingType { get { .none } set {} }

    override func draw(_ dirtyRect: NSRect) {
        for (i, l) in items.enumerated() {
            let r = NSRect(x: 8, y: 8 + CGFloat(i) * rowH, width: bounds.width - 16, height: rowH - 2)
            if i == selected {
                Theme.accentSoft.setFill()
                NSBezierPath(roundedRect: r, xRadius: Theme.Radius.control, yRadius: Theme.Radius.control).fill()
            } else if i == hover {
                Theme.fillHover.setFill()
                NSBezierPath(roundedRect: r, xRadius: Theme.Radius.control, yRadius: Theme.Radius.control).fill()
            }
            let a = Theme.attrs(i == selected ? .bodyMedium : .body, i == selected ? Theme.accent : Theme.text)
            let s = l.label as NSString
            let z = s.size(withAttributes: a)
            s.draw(at: NSPoint(x: r.minX + 10, y: r.midY - z.height / 2), withAttributes: a)
        }
    }

    private func index(_ e: NSEvent) -> Int? {
        let p = convert(e.locationInWindow, from: nil)
        let i = Int((p.y - 8) / rowH)
        return items.indices.contains(i) && p.y >= 8 ? i : nil
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        if let i = index(event) { selected = i; onSelect?(i) }
    }
    override func mouseMoved(with event: NSEvent) {
        let i = index(event)
        if i != hover { hover = i; needsDisplay = true }
    }
    override func mouseExited(with event: NSEvent) { hover = nil; needsDisplay = true }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }

    override func keyDown(with event: NSEvent) {
        switch event.keyCode {
        case 125: move(1)
        case 126: move(-1)
        default: super.keyDown(with: event)
        }
    }

    private func move(_ d: Int) {
        guard !items.isEmpty else { return }
        let i = max(0, min(items.count - 1, selected + d))
        if i != selected { selected = i; onSelect?(i) }
    }
}

private final class SnippetWindow: NSWindow {
    var onCancel: (() -> Void)?
    override func cancelOperation(_ sender: Any?) { onCancel?() }
    override var canBecomeKey: Bool { true }
}

private final class SnippetBackground: NSView {
    override var isFlipped: Bool { true }
    override func draw(_ dirtyRect: NSRect) { Theme.content.setFill(); bounds.fill() }
}

/// Code snippets for the active request: a language list and the generated code.
/// The core writes the snippets; this only shows them.
@MainActor
final class CodeSnippetController {
    private let model: ComposerModel
    private let window: SnippetWindow
    private weak var parent: NSWindow?
    private let list = LanguageList()
    private let text = NSTextView()
    private let note = label("", .callout, Theme.orange, lines: 0)
    private let copy = Chip("Copy", symbol: "doc.on.doc")
    private var languages: [CodeLanguage] = []
    private var code = ""
    private var seq = 0
    private static var live: [CodeSnippetController] = []
    static let defaultsKey = "composerCodeLang"

    init(model: ComposerModel) {
        self.model = model
        let size = NSSize(width: 720, height: 520)
        window = SnippetWindow(contentRect: NSRect(origin: .zero, size: size), styleMask: [.titled], backing: .buffered, defer: false)
        window.title = "Code snippet"
        window.isReleasedWhenClosed = false
        window.setContentSize(size)
        let root = SnippetBackground()
        window.contentView = root

        let heading = label("Code snippet", .title, Theme.text)
        let sub = label("The active request, as code. Variables are filled in.", .callout, Theme.text2)
        let titleStack = NSStackView.vertical([heading, sub], spacing: 2)
        let topRule = Divider(), bottomRule = Divider(), sideRule = Divider(vertical: true)

        list.translatesAutoresizingMaskIntoConstraints = false
        list.widthAnchor.constraint(equalToConstant: 180).isActive = true

        let card = Surface()
        card.fill = Theme.code; card.border = Theme.separator; card.radius = Theme.Radius.card
        card.translatesAutoresizingMaskIntoConstraints = false
        let scroll = NSScrollView()
        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        text.isEditable = false
        text.isSelectable = true
        text.drawsBackground = false
        text.isRichText = false
        text.textContainerInset = NSSize(width: 8, height: 10)
        text.isHorizontallyResizable = true
        text.isVerticallyResizable = true
        text.textContainer?.widthTracksTextView = false
        text.textContainer?.containerSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        text.selectedTextAttributes = [.backgroundColor: Theme.accentSoft]
        scroll.documentView = text
        card.addSubview(scroll)
        scroll.pin(to: card, insets: NSEdgeInsets(top: 1, left: 1, bottom: 1, right: 1))

        note.preferredMaxLayoutWidth = 480
        let right = NSStackView.vertical([note, card], spacing: 8)
        right.alignment = .width
        note.isHidden = true

        let close = Chip("Close", style: .accent)
        let footer = NSStackView.horizontal([NSView(), copy, close], spacing: 8)

        for v in [titleStack, topRule, list, sideRule, right, bottomRule, footer] as [NSView] { root.addSubview(v) }
        NSLayoutConstraint.activate([
            titleStack.topAnchor.constraint(equalTo: root.topAnchor, constant: 18),
            titleStack.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 22),
            topRule.topAnchor.constraint(equalTo: titleStack.bottomAnchor, constant: 14),
            topRule.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            topRule.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            list.topAnchor.constraint(equalTo: topRule.bottomAnchor),
            list.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            list.bottomAnchor.constraint(equalTo: bottomRule.topAnchor),
            sideRule.leadingAnchor.constraint(equalTo: list.trailingAnchor),
            sideRule.topAnchor.constraint(equalTo: topRule.bottomAnchor),
            sideRule.bottomAnchor.constraint(equalTo: bottomRule.topAnchor),
            right.topAnchor.constraint(equalTo: topRule.bottomAnchor, constant: 14),
            right.leadingAnchor.constraint(equalTo: sideRule.trailingAnchor, constant: 14),
            right.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -22),
            right.bottomAnchor.constraint(equalTo: bottomRule.topAnchor, constant: -14),
            bottomRule.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            bottomRule.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            bottomRule.bottomAnchor.constraint(equalTo: footer.topAnchor, constant: -12),
            footer.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 22),
            footer.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -22),
            footer.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -14),
        ])

        close.onClick = { [weak self] in self?.close() }
        window.onCancel = { [weak self] in self?.close() }
        copy.onClick = { [weak self] in self?.copyCode() }
        copy.isEnabled = false
        list.onSelect = { [weak self] i in self?.choose(i) }
        show("", error: nil)
    }

    func present(from parent: NSWindow?) {
        Self.live.append(self)
        if let parent {
            self.parent = parent
            parent.beginSheet(window)
        } else {
            window.center()
            window.makeKeyAndOrderFront(nil)
        }
        window.makeFirstResponder(list)
        Task { await load() }
    }

    private func close() {
        if let parent { parent.endSheet(window) }
        window.orderOut(nil)
        Self.live.removeAll { $0 === self }
    }

    private func load() async {
        languages = await model.codeLanguages()
        list.items = languages
        guard !languages.isEmpty else {
            show("", error: "Could not load the languages. Is the core running?")
            return
        }
        let saved = UserDefaults.standard.string(forKey: Self.defaultsKey) ?? "curl"
        let i = languages.firstIndex { $0.id == saved } ?? languages.firstIndex { $0.id == "curl" } ?? 0
        list.selected = i
        choose(i, remember: false)
    }

    private func choose(_ i: Int, remember: Bool = true) {
        guard languages.indices.contains(i) else { return }
        let id = languages[i].id
        if remember { UserDefaults.standard.set(id, forKey: Self.defaultsKey) }
        seq += 1
        let mine = seq
        Task {
            let (code, error) = await model.code(id)
            guard mine == seq else { return }
            show(code, error: error)
        }
    }

    private func show(_ code: String, error: String?) {
        self.code = code
        text.textStorage?.setAttributedString(NSAttributedString(string: code, attributes: Theme.attrs(.code, Theme.Syn.text)))
        text.scroll(.zero)
        note.isHidden = error == nil
        if let error { note.set(error, .callout, Theme.orange) }
        copy.isEnabled = !code.isEmpty
    }

    private func copyCode() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(code, forType: .string)
        copy.title = "Copied"
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { [weak self] in self?.copy.title = "Copy" }
    }
}

/// Code snippets for the active request, shown as a sheet.
@MainActor
enum CodeSnippetSheet {
    static func present(model: ComposerModel, from window: NSWindow?) {
        CodeSnippetController(model: model).present(from: window)
    }
}
