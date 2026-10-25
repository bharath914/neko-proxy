import AppKit
import NekoAPI

// MARK: - Sheet window

/// A window that closes on Escape.
final class SheetWindow: NSWindow {
    var onCancel: (() -> Void)?
    override func cancelOperation(_ sender: Any?) { onCancel?() }
    override var canBecomeKey: Bool { true }
}

private final class SheetBackground: NSView {
    override var isFlipped: Bool { true }
    override func draw(_ dirtyRect: NSRect) { Theme.content.setFill(); bounds.fill() }
}

/// The frame every composer sheet shares: a title, a content area and a footer
/// with Cancel and a primary button. Presented as a sheet on a window, or as a
/// modal window when there is none.
@MainActor
final class ComposerSheet: NSObject {
    let window: SheetWindow
    let body = FlippedView()
    let primary: Chip
    let cancel = Chip("Cancel")
    let leading = NSStackView.horizontal([], spacing: 8)
    private weak var parent: NSWindow?
    private var modal = false
    private static var live: [ComposerSheet] = []

    /// Called when the primary button is pressed. Call `close()` when done.
    var onPrimary: (() -> Void)?

    init(title: String, subtitle: String? = nil, size: NSSize, primary: String = "Save") {
        window = SheetWindow(contentRect: NSRect(origin: .zero, size: size), styleMask: [.titled], backing: .buffered, defer: false)
        self.primary = Chip(primary, style: .accent)
        super.init()
        window.title = title
        window.isReleasedWhenClosed = false
        window.setContentSize(size)
        let root = SheetBackground()
        window.contentView = root

        let heading = label(title, .title, Theme.text)
        var top: [NSView] = [heading]
        if let subtitle { top.append(label(subtitle, .callout, Theme.text2)) }
        let titleStack = NSStackView.vertical(top, spacing: 2)
        let topRule = Divider(), bottomRule = Divider()
        body.translatesAutoresizingMaskIntoConstraints = false
        let footer = NSStackView.horizontal([leading, NSView(), cancel, self.primary], spacing: 8)
        for v in [titleStack, topRule, body, bottomRule, footer] as [NSView] { root.addSubview(v) }
        NSLayoutConstraint.activate([
            titleStack.topAnchor.constraint(equalTo: root.topAnchor, constant: 18),
            titleStack.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 22),
            titleStack.trailingAnchor.constraint(lessThanOrEqualTo: root.trailingAnchor, constant: -22),
            topRule.topAnchor.constraint(equalTo: titleStack.bottomAnchor, constant: 14),
            topRule.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            topRule.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            body.topAnchor.constraint(equalTo: topRule.bottomAnchor),
            body.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            body.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            body.bottomAnchor.constraint(equalTo: bottomRule.topAnchor),
            bottomRule.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            bottomRule.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            bottomRule.bottomAnchor.constraint(equalTo: footer.topAnchor, constant: -12),
            footer.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 22),
            footer.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -22),
            footer.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -14),
        ])
        cancel.onClick = { [weak self] in self?.close() }
        self.primary.onClick = { [weak self] in self?.onPrimary?() }
        window.onCancel = { [weak self] in self?.close() }
    }

    func present(from parent: NSWindow?) {
        Self.live.append(self)
        if let parent {
            self.parent = parent
            parent.beginSheet(window)
        } else {
            modal = true
            window.center()
            NSApp.runModal(for: window)
        }
    }

    func close() {
        if let parent { parent.endSheet(window) }
        else if modal { NSApp.stopModal() }
        window.orderOut(nil)
        Self.live.removeAll { $0 === self }
    }
}

// MARK: - Dialog helpers

/// Asks "are you sure" in a sheet. `done` runs only on the destructive answer.
@MainActor
func confirmAction(_ window: NSWindow?, title: String, info: String, button: String, destructive: Bool = true, done: @escaping () -> Void) {
    let a = NSAlert()
    a.alertStyle = destructive ? .warning : .informational
    a.messageText = title
    a.informativeText = info
    let b = a.addButton(withTitle: button)
    if destructive { b.hasDestructiveAction = true }
    a.addButton(withTitle: "Cancel")
    let handle: (NSApplication.ModalResponse) -> Void = { if $0 == .alertFirstButtonReturn { done() } }
    if let window { a.beginSheetModal(for: window, completionHandler: handle) } else { handle(a.runModal()) }
}

enum UnsavedChoice { case save, discard, cancel }

/// Save, Discard or Cancel for a request with unsaved changes.
@MainActor
func askUnsaved(_ window: NSWindow?, name: String, done: @escaping (UnsavedChoice) -> Void) {
    let a = NSAlert()
    a.messageText = "Save changes to “\(name)”?"
    a.informativeText = "Your changes will be lost if you don’t save them."
    a.addButton(withTitle: "Save")
    a.addButton(withTitle: "Discard").hasDestructiveAction = true
    a.addButton(withTitle: "Cancel")
    let handle: (NSApplication.ModalResponse) -> Void = {
        switch $0 {
        case .alertFirstButtonReturn: done(.save)
        case .alertSecondButtonReturn: done(.discard)
        default: done(.cancel)
        }
    }
    if let window { a.beginSheetModal(for: window, completionHandler: handle) } else { handle(a.runModal()) }
}

/// Reads a Postman JSON file chosen by the person and imports it through the core.
@MainActor
func importPostmanFile(model: ComposerModel, window: NSWindow?, done: (() -> Void)? = nil) {
    let panel = NSOpenPanel()
    panel.allowedContentTypes = [.json]
    panel.allowsMultipleSelection = false
    panel.message = "Choose a Postman collection or environment file."
    let handle: (NSApplication.ModalResponse) -> Void = { r in
        guard r == .OK, let url = panel.url else { return }
        guard let text = try? String(contentsOf: url, encoding: .utf8) else {
            tell(window, "Import", "That file could not be read as text.")
            return
        }
        Task { @MainActor in
            switch await model.importPostman(text) {
            case .ok(let notice):
                done?()
                tell(window, "Import", notice.isEmpty ? "Nothing was added." : notice)
            case .failed(let error):
                tell(window, "Import", error)
            }
        }
    }
    if let window { panel.beginSheetModal(for: window, completionHandler: handle) } else { handle(panel.runModal()) }
}

/// Saves the JSON of a collection or environment export where the person chooses.
@MainActor
func exportPostmanFile(model: ComposerModel, environment: Bool, name: String, window: NSWindow?) {
    Task { @MainActor in
        switch await model.exportPostman(environment: environment, name: name) {
        case .failed(let error):
            tell(window, "Export", error)
        case .ok(let (json, filename)):
            let panel = NSSavePanel()
            panel.allowedContentTypes = [.json]
            panel.nameFieldStringValue = filename.isEmpty ? name + ".json" : filename
            let handle: (NSApplication.ModalResponse) -> Void = { r in
                guard r == .OK, let url = panel.url else { return }
                do { try json.write(to: url, atomically: true, encoding: .utf8) }
                catch { tell(window, "Export", error.localizedDescription) }
            }
            if let window { panel.beginSheetModal(for: window, completionHandler: handle) } else { handle(panel.runModal()) }
        }
    }
}

// MARK: - Pill button

/// A rounded fill with a label and a chevron that pops a menu: the environment
/// picker and the choices in the settings sheets.
final class PillButton: NSView {
    var title: String { didSet { invalidateIntrinsicContentSize(); needsDisplay = true } }
    var dimmed = false { didSet { needsDisplay = true } }
    var mono = false
    var leadingSymbol: String? { didSet { invalidateIntrinsicContentSize(); needsDisplay = true } }
    var menuProvider: (() -> NSMenu)?
    var maxWidth: CGFloat = 220
    private var hover = false
    override var isFlipped: Bool { true }

    init(_ title: String) {
        self.title = title
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
    }
    required init?(coder: NSCoder) { fatalError() }

    private var attrs: [NSAttributedString.Key: Any] {
        mono ? [.font: Theme.monoMedium(12), .foregroundColor: dimmed ? Theme.text3 : Theme.text] : Theme.attrs(.bodyMedium, dimmed ? Theme.text3 : Theme.text)
    }

    override var intrinsicContentSize: NSSize {
        let w = (title as NSString).size(withAttributes: attrs).width
        return NSSize(width: min(w + 36 + (leadingSymbol == nil ? 0 : 18), maxWidth), height: 28)
    }

    override func draw(_ dirtyRect: NSRect) {
        let p = NSBezierPath(roundedRect: bounds, xRadius: Theme.Radius.control, yRadius: Theme.Radius.control)
        (hover ? Theme.fillHover : Theme.fill).setFill(); p.fill()
        var x: CGFloat = 11
        if let s = leadingSymbol {
            drawSymbol(s, in: NSRect(x: 8, y: 0, width: 16, height: bounds.height), color: dimmed ? Theme.text3 : Theme.text2, size: 11, weight: .medium)
            x += 18
        }
        let para = NSMutableParagraphStyle(); para.lineBreakMode = .byTruncatingTail
        var a = attrs; a[.paragraphStyle] = para
        let sz = (title as NSString).size(withAttributes: attrs)
        (title as NSString).draw(in: NSRect(x: x, y: bounds.midY - sz.height / 2, width: max(bounds.width - x - 24, 10), height: sz.height), withAttributes: a)
        drawSymbol("chevron.down", in: NSRect(x: bounds.maxX - 22, y: 0, width: 14, height: bounds.height), color: Theme.text3, size: 8.5, weight: .bold)
    }

    override func mouseDown(with event: NSEvent) {
        guard let m = menuProvider?() else { return }
        m.popUp(positioning: nil, at: NSPoint(x: 0, y: bounds.height + 4), in: self)
    }
    override func mouseEntered(with event: NSEvent) { hover = true; needsDisplay = true }
    override func mouseExited(with event: NSEvent) { hover = false; needsDisplay = true }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }
}

// MARK: - Scrolling column

/// A vertical scroll view whose content is one view pinned to its width.
@MainActor
final class ScrollColumn: NSView {
    let scroll = NSScrollView()
    private let doc = FlippedView()
    private var content: NSView?
    override var isFlipped: Bool { true }

    init() {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        doc.translatesAutoresizingMaskIntoConstraints = false
        scroll.documentView = doc
        addSubview(scroll)
        scroll.pin(to: self)
        let clip = scroll.contentView
        NSLayoutConstraint.activate([
            doc.topAnchor.constraint(equalTo: clip.topAnchor),
            doc.leadingAnchor.constraint(equalTo: clip.leadingAnchor),
            doc.trailingAnchor.constraint(equalTo: clip.trailingAnchor),
        ])
    }
    required init?(coder: NSCoder) { fatalError() }

    func set(_ view: NSView, insets: NSEdgeInsets = NSEdgeInsets(top: 18, left: 22, bottom: 22, right: 22)) {
        content?.removeFromSuperview()
        view.translatesAutoresizingMaskIntoConstraints = false
        doc.addSubview(view)
        view.pin(to: doc, insets: insets)
        content = view
        scroll.contentView.scroll(to: .zero)
    }
}

// MARK: - Save request

/// Name and place for a request that has never been saved. Saving an already
/// saved request just writes it back.
@MainActor
enum SaveRequestSheet {
    /// Saves the active tab: straight away when it has an id, otherwise after asking
    /// for a name and a collection. `completion` gets true once it is saved.
    static func saveActive(model: ComposerModel, from window: NSWindow?, completion: ((Bool) -> Void)? = nil) {
        if model.tab.draft.id.isEmpty {
            present(model: model, from: window, completion: completion)
            return
        }
        Task { @MainActor in
            if let error = await model.save() { tell(window, "Save", error); completion?(false) }
            else { completion?(true) }
        }
    }

    static func present(model: ComposerModel, from window: NSWindow?, completion: ((Bool) -> Void)? = nil) {
        let draft = model.tab.draft
        let sheet = ComposerSheet(title: "Save request", size: NSSize(width: 440, height: 264), primary: "Save")
        let name = Field(draft.name, placeholder: "Request name", mono: false)
        let combo = NSComboBox()
        combo.usesDataSource = false
        combo.addItems(withObjectValues: SidebarTree.destinations(model.state.collections))
        combo.stringValue = draft.collection
        combo.placeholderString = "Choose or type a collection"
        combo.font = Theme.font(.body)
        combo.completes = true
        combo.translatesAutoresizingMaskIntoConstraints = false
        combo.heightAnchor.constraint(equalToConstant: 28).isActive = true
        let hint = label("Use a slash to put it in a folder, like Shop API/Orders. A new name makes a new collection.", .caption, Theme.text3, lines: 0)
        hint.preferredMaxLayoutWidth = 396
        let col = NSStackView.vertical([labelled("Name", name), labelled("Save to", combo), hint], spacing: 14)
        col.alignment = .leading
        sheet.body.addSubview(col)
        col.pin(to: sheet.body, insets: NSEdgeInsets(top: 18, left: 22, bottom: 0, right: 22))
        for v in [col.arrangedSubviews[0], col.arrangedSubviews[1]] { v.leadingAnchor.constraint(equalTo: col.leadingAnchor).isActive = true; v.trailingAnchor.constraint(equalTo: col.trailingAnchor).isActive = true }
        sheet.window.initialFirstResponder = name.input

        let finish: () -> Void = {
            let n = name.text.trimmingCharacters(in: .whitespacesAndNewlines)
            let c = combo.stringValue.trimmingCharacters(in: CharacterSet(charactersIn: "/ ").union(.whitespaces))
            if n.isEmpty { tell(sheet.window, "Save request", "Give the request a name."); return }
            if c.isEmpty { tell(sheet.window, "Save request", "Choose a collection to save it in."); return }
            Task { @MainActor in
                if !model.state.collections.contains(c), let error = await model.addCollection(c) {
                    tell(sheet.window, "Save request", error); return
                }
                if let error = await model.save(name: n, collection: c) {
                    tell(sheet.window, "Save request", error); return
                }
                sheet.close()
                completion?(true)
            }
        }
        sheet.onPrimary = finish
        name.onSubmit = finish
        sheet.cancel.onClick = { sheet.close(); completion?(false) }
        sheet.present(from: window)
    }
}
