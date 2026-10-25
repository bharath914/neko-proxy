import AppKit
import NekoAPI

// MARK: - One tab

/// A single open request in the strip: method, title, unsaved dot or close ×.
@MainActor
private final class RequestTabItem: NSView {
    var method = "GET" { didSet { needsDisplay = true } }
    var title = "" { didSet { needsDisplay = true } }
    var dirty = false { didSet { needsDisplay = true } }
    var active = false { didSet { needsDisplay = true } }
    var onSelect: (() -> Void)?
    var onClose: (() -> Void)?
    var menuProvider: (() -> NSMenu?)?
    /// Drag progress in points from where the press began; `ended` on release.
    var onDrag: ((RequestTabItem, CGFloat, Bool) -> Void)?
    private var hover = false, slotHover = false
    private var downX: CGFloat = 0
    private var pressedSlot = false
    private(set) var dragging = false
    override var isFlipped: Bool { true }

    static let minWidth: CGFloat = 124, maxWidth: CGFloat = 220

    var naturalWidth: CGFloat {
        let tw = (title as NSString).size(withAttributes: Theme.attrs(.body)).width
        let mw = (SidebarTree.methodLabel(method) as NSString).size(withAttributes: [.font: Theme.monoMedium(10)]).width
        return min(max(14 + mw + 7 + tw + 8 + 22 + 6, Self.minWidth), Self.maxWidth)
    }

    private var slot: NSRect { NSRect(x: bounds.maxX - 28, y: bounds.midY - 9, width: 20, height: 18) }

    override func draw(_ dirtyRect: NSRect) {
        let h = bounds.height
        if active || hover {
            // Rounded at the top only: the bottom corners sit below the view and are clipped.
            let r = NSRect(x: 1, y: 6, width: bounds.width - 2, height: h - 6 + 10)
            let p = NSBezierPath(roundedRect: r, xRadius: Theme.Radius.control, yRadius: Theme.Radius.control)
            (active ? Theme.card : Theme.fill).setFill()
            p.fill()
        }
        if active {
            Theme.accent.setFill()
            NSBezierPath(roundedRect: NSRect(x: 10, y: h - 2, width: bounds.width - 20, height: 2), xRadius: 1, yRadius: 1).fill()
        }
        let ma: [NSAttributedString.Key: Any] = [.font: Theme.monoMedium(10), .foregroundColor: Theme.method(method)]
        let ms = SidebarTree.methodLabel(method) as NSString
        let msz = ms.size(withAttributes: ma)
        let midY = (h - 2) / 2 + 3
        ms.draw(at: NSPoint(x: 14, y: midY - msz.height / 2), withAttributes: ma)

        let showClose = hover || (active && !dirty)
        let textRight = bounds.maxX - 32
        let p = NSMutableParagraphStyle(); p.lineBreakMode = .byTruncatingTail
        var ta = Theme.attrs(active ? .bodyMedium : .body, active ? Theme.text : Theme.text2)
        ta[.paragraphStyle] = p
        let tsz = (title as NSString).size(withAttributes: ta)
        let tx = 14 + msz.width + 7
        (title as NSString).draw(in: NSRect(x: tx, y: midY - tsz.height / 2, width: max(textRight - tx, 10), height: tsz.height), withAttributes: ta)

        if showClose {
            if slotHover {
                Theme.fillHover.setFill()
                NSBezierPath(roundedRect: slot, xRadius: 5, yRadius: 5).fill()
            }
            drawSymbol("xmark", in: NSRect(x: slot.minX, y: slot.minY - 3, width: slot.width, height: slot.height), color: slotHover ? Theme.text : Theme.text3, size: 9, weight: .bold)
        } else if dirty {
            Theme.orange.setFill()
            NSBezierPath(ovalIn: NSRect(x: slot.midX - 3.5, y: midY - 3.5, width: 7, height: 7)).fill()
        }
    }

    override func mouseDown(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        downX = event.locationInWindow.x
        dragging = false
        pressedSlot = (hover || active && !dirty) && slot.contains(p)
        if !pressedSlot { onSelect?() }
    }

    override func mouseDragged(with event: NSEvent) {
        guard !pressedSlot else { return }
        let dx = event.locationInWindow.x - downX
        if !dragging && abs(dx) < 4 { return }
        dragging = true
        onDrag?(self, dx, false)
    }

    override func mouseUp(with event: NSEvent) {
        if dragging {
            dragging = false
            onDrag?(self, event.locationInWindow.x - downX, true)
        } else if pressedSlot, slot.contains(convert(event.locationInWindow, from: nil)) {
            onClose?()
        }
        pressedSlot = false
    }

    override func otherMouseUp(with event: NSEvent) {
        if event.buttonNumber == 2 { onClose?() }
    }

    override func menu(for event: NSEvent) -> NSMenu? { menuProvider?() }

    override func mouseEntered(with event: NSEvent) { hover = true; needsDisplay = true }
    override func mouseExited(with event: NSEvent) { hover = false; slotHover = false; needsDisplay = true }
    override func mouseMoved(with event: NSEvent) {
        let s = slot.contains(convert(event.locationInWindow, from: nil))
        if s != slotHover { slotHover = s; needsDisplay = true }
    }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .mouseMoved, .activeInKeyWindow, .inVisibleRect], owner: self))
    }
}

/// A horizontal scroll view that also scrolls sideways for a mouse wheel.
private final class SidewaysScroll: NSScrollView {
    override func scrollWheel(with event: NSEvent) {
        guard abs(event.scrollingDeltaY) > abs(event.scrollingDeltaX), let doc = documentView else { super.scrollWheel(with: event); return }
        let clip = contentView
        let maxX = max(doc.frame.width - clip.bounds.width, 0)
        let x = min(max(clip.bounds.origin.x - event.scrollingDeltaY, 0), maxX)
        clip.scroll(to: NSPoint(x: x, y: 0))
        reflectScrolledClipView(clip)
    }
}

// MARK: - The bar

/// The strip of open requests with the environment picker at its right.
@MainActor
final class RequestTabBar: NSView {
    let model: ComposerModel
    override var isFlipped: Bool { true }

    private let scroll = SidewaysScroll()
    private let strip = FlippedView()
    private let plus = IconButton("plus", tip: "New request")
    private let env = PillButton("No environment")
    private let eye = IconButton("eye", tip: "Variables in scope")
    private var scrollWidth: NSLayoutConstraint!
    private var items: [ObjectIdentifier: RequestTabItem] = [:]
    private var order: [RequestTabItem] = []
    private var popover: NSPopover?

    init(model: ComposerModel) {
        self.model = model
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        heightAnchor.constraint(equalToConstant: 40).isActive = true

        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.drawsBackground = false
        scroll.hasHorizontalScroller = false
        scroll.hasVerticalScroller = false
        scroll.horizontalScrollElasticity = .allowed
        scroll.verticalScrollElasticity = .none
        strip.translatesAutoresizingMaskIntoConstraints = true
        scroll.documentView = strip

        plus.onClick = { [weak self] in self?.model.newTab() }
        env.dimmed = true
        env.menuProvider = { [weak self] in self?.envMenu() ?? NSMenu() }
        eye.onClick = { [weak self] in self?.showVariables() }
        let right = NSStackView.horizontal([env, eye], spacing: 6)

        for v in [scroll, plus, right] as [NSView] { addSubview(v) }
        scrollWidth = scroll.widthAnchor.constraint(equalToConstant: 0)
        scrollWidth.priority = NSLayoutConstraint.Priority(750)
        NSLayoutConstraint.activate([
            scroll.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            scroll.topAnchor.constraint(equalTo: topAnchor),
            scroll.bottomAnchor.constraint(equalTo: bottomAnchor),
            scrollWidth,
            plus.leadingAnchor.constraint(equalTo: scroll.trailingAnchor, constant: 2),
            plus.centerYAnchor.constraint(equalTo: centerYAnchor, constant: 1),
            right.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            right.centerYAnchor.constraint(equalTo: centerYAnchor, constant: 1),
            right.leadingAnchor.constraint(greaterThanOrEqualTo: plus.trailingAnchor, constant: 12),
        ])
        reload()
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) { Theme.grouped.setFill(); bounds.fill() }

    // MARK: reload

    /// Tabs, titles, dirty marks, the active tab or the environment changed.
    func reload() {
        var seen = Set<ObjectIdentifier>()
        order = []
        for (i, t) in model.tabs.enumerated() {
            let key = ObjectIdentifier(t)
            seen.insert(key)
            let item = items[key] ?? makeItem(for: t)
            items[key] = item
            item.method = t.draft.method.isEmpty ? "GET" : t.draft.method
            item.title = t.title
            item.dirty = t.dirty
            item.active = i == model.active
            order.append(item)
        }
        for (k, v) in items where !seen.contains(k) { v.removeFromSuperview(); items[k] = nil }
        for v in order where v.superview == nil { strip.addSubview(v) }
        layoutTabs()
        if let a = order[safe: model.active] { strip.scrollToVisible(a.frame.insetBy(dx: -24, dy: 0)) }

        let name = model.state.activeEnvironment
        env.title = name.isEmpty ? "No environment" : name
        env.dimmed = name.isEmpty
    }

    private func makeItem(for tab: RequestTab) -> RequestTabItem {
        let item = RequestTabItem()
        item.onSelect = { [weak self, weak tab] in
            guard let self, let tab, let i = self.model.tabs.firstIndex(where: { $0 === tab }) else { return }
            self.model.select(i)
        }
        item.onClose = { [weak self, weak tab] in
            guard let self, let tab else { return }
            self.requestClose(tab)
        }
        item.menuProvider = { [weak self, weak tab] in
            guard let self, let tab else { return nil }
            return self.tabMenu(tab)
        }
        item.onDrag = { [weak self] item, dx, ended in self?.drag(item, dx: dx, ended: ended) }
        return item
    }

    private func layoutTabs(dragging: RequestTabItem? = nil, dragX: CGFloat = 0) {
        let h = bounds.height > 0 ? bounds.height : 40
        let widths = order.map { $0.naturalWidth }
        var total: CGFloat = 0
        var slots: [CGFloat] = []
        for w in widths { slots.append(total); total += w }
        if let d = dragging, let from = order.firstIndex(of: d) {
            // Others slide into the gap; the dragged tab follows the pointer.
            let others = order.indices.filter { $0 != from }
            let dw = widths[from]
            let center = dragX + dw / 2
            var insert = 0
            var x: CGFloat = 0
            for i in others {
                if x + widths[i] / 2 < center { insert += 1 }
                x += widths[i]
            }
            var cursor: CGFloat = 0
            for (n, i) in others.enumerated() {
                if n == insert { cursor += dw }
                order[i].frame = NSRect(x: cursor, y: 0, width: widths[i], height: h)
                cursor += widths[i]
            }
            d.frame = NSRect(x: min(max(dragX, 0), max(total - dw, 0)), y: 0, width: dw, height: h)
            dropIndex = insert
        } else {
            for (i, v) in order.enumerated() { v.frame = NSRect(x: slots[i], y: 0, width: widths[i], height: h) }
        }
        strip.frame = NSRect(x: 0, y: 0, width: total, height: h)
        scrollWidth.constant = total
    }

    override func layout() {
        super.layout()
        if abs(strip.frame.height - bounds.height) > 0.5, !order.isEmpty { layoutTabs() }
    }

    // MARK: drag to reorder

    private var dropIndex = 0
    private var dragStart: CGFloat?

    private func drag(_ item: RequestTabItem, dx: CGFloat, ended: Bool) {
        if dragStart == nil { dragStart = item.frame.minX }
        let x = (dragStart ?? 0) + dx
        if ended {
            layoutTabs(dragging: item, dragX: x)
            let from = order.firstIndex(of: item) ?? 0
            let to = dropIndex
            dragStart = nil
            if from != to { model.move(from, to: to) } else { reload() }
        } else {
            layoutTabs(dragging: item, dragX: x)
            item.superview?.addSubview(item, positioned: .above, relativeTo: nil)
        }
    }

    // MARK: closing

    private func requestClose(_ tab: RequestTab) {
        guard let i = model.tabs.firstIndex(where: { $0 === tab }) else { return }
        guard tab.dirty else { model.close(i); return }
        model.select(i)
        askUnsaved(window, name: tab.title) { [weak self, weak tab] choice in
            guard let self, let tab else { return }
            switch choice {
            case .cancel: break
            case .discard:
                if let j = self.model.tabs.firstIndex(where: { $0 === tab }) { self.model.close(j) }
            case .save:
                SaveRequestSheet.saveActive(model: self.model, from: self.window) { [weak self, weak tab] ok in
                    guard ok, let self, let tab, let j = self.model.tabs.firstIndex(where: { $0 === tab }) else { return }
                    self.model.close(j)
                }
            }
        }
    }

    private func tabMenu(_ tab: RequestTab) -> NSMenu {
        let m = ActionMenu()
        m.add("Close") { [weak self, weak tab] in if let self, let tab { self.requestClose(tab) } }
        m.add("Close Others", enabled: model.tabs.count > 1) { [weak self, weak tab] in
            guard let self, let tab, let i = self.model.tabs.firstIndex(where: { $0 === tab }) else { return }
            let unsaved = self.model.tabs.filter { $0 !== tab && $0.dirty }.count
            if unsaved == 0 { self.model.closeOthers(keeping: i); return }
            confirmAction(self.window, title: "Close other tabs?", info: "\(unsaved) of them \(unsaved == 1 ? "has" : "have") unsaved changes that will be lost.", button: "Close Others") { [weak self, weak tab] in
                guard let self, let tab, let j = self.model.tabs.firstIndex(where: { $0 === tab }) else { return }
                self.model.closeOthers(keeping: j)
            }
        }
        m.separator()
        m.add("Duplicate Tab") { [weak self, weak tab] in
            guard let self, let tab, let i = self.model.tabs.firstIndex(where: { $0 === tab }) else { return }
            var copy = tab.draft
            copy.id = ""
            self.model.select(i)
            self.model.newTab(copy)
        }
        m.add("Copy URL") { [weak tab] in
            guard let tab else { return }
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(tab.display.isEmpty ? tab.draft.url : tab.display, forType: .string)
        }
        return m
    }

    // MARK: environment

    private func envMenu() -> NSMenu {
        let m = ActionMenu()
        let active = model.state.activeEnvironment
        m.add("No environment", checked: active.isEmpty) { [weak self] in Task { await self?.model.setActive("") } }
        if !model.state.environments.isEmpty { m.separator() }
        for e in model.state.environments {
            m.add(e.name, checked: e.name == active) { [weak self] in Task { await self?.model.setActive(e.name) } }
        }
        m.separator()
        m.add("Manage environments…") { [weak self] in
            guard let self else { return }
            EnvironmentsSheet.present(model: self.model, from: self.window)
        }
        return m
    }

    private func showVariables() {
        if let p = popover, p.isShown { p.close(); return }
        let content = VariablesQuickLook(model: model)
        content.onEdit = { [weak self] in
            guard let self else { return }
            self.popover?.close()
            EnvironmentsSheet.present(model: self.model, from: self.window)
        }
        let vc = NSViewController()
        vc.view = content
        vc.preferredContentSize = content.size
        let p = NSPopover()
        p.behavior = .transient
        p.contentViewController = vc
        p.contentSize = content.size
        p.show(relativeTo: eye.bounds, of: eye, preferredEdge: .maxY)
        popover = p
    }
}

private extension Array {
    subscript(safe i: Int) -> Element? { indices.contains(i) ? self[i] : nil }
}

// MARK: - Quick look

/// What the eye button shows: the variables that apply to the active request,
/// from the globals, the active environment and the request's collections.
@MainActor
final class VariablesQuickLook: NSView {
    var onEdit: (() -> Void)?
    private(set) var size = NSSize(width: 340, height: 120)
    override var isFlipped: Bool { true }

    init(model: ComposerModel) {
        super.init(frame: .zero)
        let state = model.state
        var sections: [(String, [KeyValue])] = [("Globals", state.globals)]
        if let e = state.environments.first(where: { $0.name == state.activeEnvironment }) {
            sections.append(("Environment · \(e.name)", e.vars))
        }
        let path = model.tab.draft.collection
        if !path.isEmpty {
            var prefix = ""
            for (n, part) in path.split(separator: "/").enumerated() {
                prefix = SidebarTree.child(prefix, String(part))
                if let c = state.configs.first(where: { $0.name == prefix }), !c.vars.isEmpty {
                    sections.append(((n == 0 ? "Collection" : "Folder") + " · \(part)", c.vars))
                }
            }
        }

        let title = label("Variables in scope", .headline, Theme.text)
        let edit = Chip("Edit", style: .text)
        edit.onClick = { [weak self] in self?.onEdit?() }
        let head = NSStackView.horizontal([title, NSView(), edit], spacing: 8)
        var rows: [NSView] = []
        var headerGaps: [NSView] = []
        let any = sections.contains { !$0.1.isEmpty }
        for (name, vars) in sections {
            if let prev = rows.last { headerGaps.append(prev) }
            rows.append(overline(name))
            if vars.isEmpty {
                rows.append(label("None", .callout, Theme.text3))
            } else {
                for v in vars { rows.append(row(v)) }
            }
        }
        if !any {
            rows.append(label("No variables are defined yet. Add some with Edit, and use them as {{name}} in a request.", .callout, Theme.text3, lines: 0))
        }
        let list = NSStackView.vertical(rows, spacing: 6)
        for v in headerGaps { list.setCustomSpacing(16, after: v) }
        list.widthAnchor.constraint(equalToConstant: 312).isActive = true
        for r in rows where r is NSStackView { r.widthAnchor.constraint(equalTo: list.widthAnchor).isActive = true }
        list.layoutSubtreeIfNeeded()
        let listHeight = list.fittingSize.height
        let body: NSView
        if listHeight > 330 {
            let sc = ScrollColumn()
            sc.set(list, insets: NSEdgeInsets())
            sc.heightAnchor.constraint(equalToConstant: 330).isActive = true
            body = sc
        } else {
            body = list
        }
        let rule = Divider()
        let col = NSStackView.vertical([head, rule, body], spacing: 10)
        col.alignment = .leading
        addSubview(col)
        col.pin(to: self, insets: NSEdgeInsets(top: 12, left: 14, bottom: 14, right: 14))
        for v in [head, rule, body] as [NSView] { v.widthAnchor.constraint(equalTo: col.widthAnchor).isActive = true }
        widthAnchor.constraint(equalToConstant: 340).isActive = true
        layoutSubtreeIfNeeded()
        size = NSSize(width: 340, height: fittingSize.height)
        frame = NSRect(origin: .zero, size: size)
    }
    required init?(coder: NSCoder) { fatalError() }

    private func row(_ v: KeyValue) -> NSView {
        let dim = v.disabled
        let name = label(v.name, .mono, dim ? Theme.text3 : Theme.text)
        name.widthAnchor.constraint(equalToConstant: 112).isActive = true
        name.lineBreakMode = .byTruncatingMiddle
        let value = label(v.secret ? "••••••••" : v.value, .mono, Theme.text2)
        value.lineBreakMode = .byTruncatingTail
        value.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        let r = NSStackView.horizontal([name, value], spacing: 10)
        r.distribution = .fill
        return r
    }
}

// MARK: - Sheet entry points

/// Opens the environments and globals editor as a sheet on `window`.
@MainActor
enum EnvironmentsSheet {
    /// Opens on the active environment, or on Globals when none is active.
    static func present(model: ComposerModel, from window: NSWindow?) {
        let active = model.state.activeEnvironment
        present(model: model, from: window, selecting: active.isEmpty ? nil : active)
    }

    /// Opens on one environment by name; nil selects Globals.
    static func present(model: ComposerModel, from window: NSWindow?, selecting name: String?) {
        EnvironmentsEditor(model: model, selecting: name).present(from: window)
    }
}

/// Opens a collection's or folder's settings (auth, headers, variables, notes) as a sheet.
@MainActor
enum CollectionSettingsSheet {
    static func present(model: ComposerModel, path: String, from window: NSWindow?) {
        CollectionSettingsEditor(model: model, path: path).present(from: window)
    }
}
