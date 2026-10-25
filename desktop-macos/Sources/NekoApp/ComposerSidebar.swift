import AppKit
import NekoAPI

/// The composer's left column: Collections, Environments and History, with a
/// search box and a "+" menu. It reads the model and asks it to change; the tree
/// itself is flattened by `SidebarTree`.
@MainActor
final class ComposerSidebar: NSView {
    let model: ComposerModel
    var onNewRequest: (() -> Void)?
    var onImportCurl: (() -> Void)?
    /// 0 Collections, 1 Environments, 2 History.
    var selectedTab = 0 {
        didSet {
            seg.selected = selectedTab
            reload(force: true)
        }
    }
    override var isFlipped: Bool { true }

    private let seg = Segmented(["Collections", "Environments", "History"])
    private let search = Field("", placeholder: "Search", mono: false)
    private let plus = IconButton("plus", tip: "New…")
    private let scroll = NSScrollView()
    private let doc = FlippedView()
    private let stack = NSStackView.vertical([], spacing: 0)
    private let footer = FlippedView()
    private let footerRule = Divider()
    private let footerChip = Chip("Edit environments…", symbol: "slider.horizontal.3")
    private var footerHeight: NSLayoutConstraint!
    private var collapsed: Set<String> = []
    private var query = ""

    private struct HRow: Equatable {
        var method: String, url: String
        var status: UInt32
        var ms: Double
        var error: String
        var sentMs: Int64
    }
    private enum Snap: Equatable {
        case collections(rows: [SidebarRow], active: String, dirty: Set<String>, any: Bool, searching: Bool)
        case envs(names: [String], counts: [Int], active: String, globals: Int, query: String)
        case history(rows: [HRow], query: String)
    }
    private var last: Snap?

    init(model: ComposerModel) {
        self.model = model
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false

        seg.equalWidths = true
        seg.role = .callout
        seg.onChange = { [weak self] i in self?.selectedTab = i }
        search.onChange = { [weak self] t in self?.query = t; self?.reload() }
        plus.onClick = { [weak self] in self?.showPlusMenu() }

        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        doc.translatesAutoresizingMaskIntoConstraints = false
        scroll.documentView = doc
        doc.addSubview(stack)
        stack.pin(to: doc, insets: NSEdgeInsets(top: 4, left: 0, bottom: 10, right: 0))

        footer.translatesAutoresizingMaskIntoConstraints = false
        footer.addSubview(footerRule)
        footer.addSubview(footerChip)
        footerChip.onClick = { [weak self] in self?.footerAction() }

        for v in [seg, search, plus, scroll, footer] as [NSView] { addSubview(v) }
        let clip = scroll.contentView
        footerHeight = footer.heightAnchor.constraint(equalToConstant: 0)
        NSLayoutConstraint.activate([
            seg.topAnchor.constraint(equalTo: topAnchor, constant: 12),
            seg.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            seg.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            search.topAnchor.constraint(equalTo: seg.bottomAnchor, constant: 10),
            search.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            search.trailingAnchor.constraint(equalTo: plus.leadingAnchor, constant: -6),
            plus.centerYAnchor.constraint(equalTo: search.centerYAnchor),
            plus.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -9),
            scroll.topAnchor.constraint(equalTo: search.bottomAnchor, constant: 8),
            scroll.leadingAnchor.constraint(equalTo: leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: trailingAnchor),
            scroll.bottomAnchor.constraint(equalTo: footer.topAnchor),
            footer.leadingAnchor.constraint(equalTo: leadingAnchor),
            footer.trailingAnchor.constraint(equalTo: trailingAnchor),
            footer.bottomAnchor.constraint(equalTo: bottomAnchor),
            footerHeight,
            footerRule.topAnchor.constraint(equalTo: footer.topAnchor),
            footerRule.leadingAnchor.constraint(equalTo: footer.leadingAnchor),
            footerRule.trailingAnchor.constraint(equalTo: footer.trailingAnchor),
            footerChip.leadingAnchor.constraint(equalTo: footer.leadingAnchor, constant: 12),
            footerChip.trailingAnchor.constraint(equalTo: footer.trailingAnchor, constant: -12),
            footerChip.centerYAnchor.constraint(equalTo: footer.centerYAnchor),
            doc.topAnchor.constraint(equalTo: clip.topAnchor),
            doc.leadingAnchor.constraint(equalTo: clip.leadingAnchor),
            doc.trailingAnchor.constraint(equalTo: clip.trailingAnchor),
        ])
        reload(force: true)
    }
    required init?(coder: NSCoder) { fatalError() }

    // MARK: reload

    /// The saved state, open tabs or active tab changed.
    func reload() { reload(force: false) }

    private func reload(force: Bool) {
        let snap = snapshot()
        updateFooter()
        if !force, snap == last { return }
        last = snap
        let y = scroll.contentView.bounds.origin.y
        stack.arrangedSubviews.forEach { stack.removeArrangedSubview($0); $0.removeFromSuperview() }
        let views: [NSView]
        switch selectedTab {
        case 1: views = environmentRows()
        case 2: views = historyRows()
        default: views = collectionRows(snap)
        }
        for v in views {
            stack.addArrangedSubview(v)
            v.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        }
        layoutSubtreeIfNeeded()
        scroll.contentView.scroll(to: NSPoint(x: 0, y: force ? 0 : y))
        scroll.reflectScrolledClipView(scroll.contentView)
    }

    private func snapshot() -> Snap {
        let s = model.state
        let q = query.trimmingCharacters(in: .whitespaces)
        switch selectedTab {
        case 1:
            let envs = s.environments.filter { q.isEmpty || $0.name.localizedCaseInsensitiveContains(q) }
            return .envs(names: envs.map(\.name), counts: envs.map { $0.vars.count }, active: s.activeEnvironment, globals: s.globals.count, query: q)
        case 2:
            return .history(rows: filteredHistory().map { HRow(method: $0.method, url: $0.url, status: $0.status, ms: $0.durationMs, error: $0.error, sentMs: $0.sentMs) }, query: q)
        default:
            let rows = SidebarTree.rows(tree: model.tree(), orphans: SidebarTree.orphans(collections: s.collections, requests: s.requests), collapsed: collapsed, query: q)
            let dirty = Set(model.tabs.filter { $0.dirty && !$0.draft.id.isEmpty }.map { $0.draft.id })
            return .collections(rows: rows, active: model.tab.draft.id, dirty: dirty, any: !s.collections.isEmpty || !s.requests.isEmpty, searching: !q.isEmpty)
        }
    }

    private func filteredHistory() -> [Neko_Api_V1_ComposerHistoryEntry] {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        let all = model.state.history
        if q.isEmpty { return all }
        return all.filter { $0.url.lowercased().contains(q) || $0.method.lowercased().contains(q) || $0.request.name.lowercased().contains(q) }
    }

    private func updateFooter() {
        switch selectedTab {
        case 1:
            footerHeight.constant = 52; footer.isHidden = false
            footerChip.title = "Edit environments…"; footerChip.symbolName = "slider.horizontal.3"; footerChip.style = .plain; footerChip.isEnabled = true
        case 2:
            footerHeight.constant = 52; footer.isHidden = false
            footerChip.title = "Clear history"; footerChip.symbolName = "trash"; footerChip.style = .danger
            footerChip.isEnabled = !model.state.history.isEmpty
        default:
            footerHeight.constant = 0; footer.isHidden = true
        }
    }

    private func footerAction() {
        if selectedTab == 1 { EnvironmentsSheet.present(model: model, from: window); return }
        confirmAction(window, title: "Clear history?", info: "All sent requests are removed from the list. Saved requests and captured flows stay.", button: "Clear history") { [weak self] in
            Task { await self?.model.clearHistory() }
        }
    }

    // MARK: collections

    private func collectionRows(_ snap: Snap) -> [NSView] {
        guard case .collections(let rows, _, let dirty, let any, let searching) = snap else { return [] }
        if rows.isEmpty {
            if searching { return [hint("No matches", "No request in your collections matches the search.")] }
            if !any { return [emptyState()] }
            return [hint("Nothing here yet", "Use + to add a request.")]
        }
        let activeID = model.tab.draft.id
        return rows.map { row in
            switch row.kind {
            case .folder(let path, let name, let count, let expanded):
                return folderRow(path: path, name: name, count: count, expanded: expanded, depth: row.depth)
            case .request(let id, let name, let method):
                return requestRow(id: id, name: name, method: method, depth: row.depth, active: !id.isEmpty && id == activeID, dirty: dirty.contains(id))
            }
        }
    }

    private func emptyState() -> NSView {
        let t = label("No collections yet", .headline, Theme.text)
        let s = label("Group your requests into collections and folders, or bring in a Postman file.", .callout, Theme.text2, lines: 0)
        s.preferredMaxLayoutWidth = 220
        let b = Chip("New collection", symbol: "plus", style: .accent)
        b.onClick = { [weak self] in self?.newCollection() }
        let i = Chip("Import Postman file…", style: .text)
        i.onClick = { [weak self] in self?.importPostman() }
        let col = NSStackView.vertical([t, s, b, i], spacing: 8)
        col.setCustomSpacing(14, after: s)
        col.setCustomSpacing(2, after: b)
        let box = FlippedView()
        box.translatesAutoresizingMaskIntoConstraints = false
        box.addSubview(col)
        col.pin(to: box, insets: NSEdgeInsets(top: 28, left: 22, bottom: 12, right: 22))
        return box
    }

    private func hint(_ title: String, _ text: String) -> NSView {
        let t = label(title, .bodyMedium, Theme.text)
        let s = label(text, .callout, Theme.text3, lines: 0)
        s.preferredMaxLayoutWidth = 230
        let col = NSStackView.vertical([t, s], spacing: 4)
        let box = FlippedView()
        box.translatesAutoresizingMaskIntoConstraints = false
        box.addSubview(col)
        col.pin(to: box, insets: NSEdgeInsets(top: 20, left: 22, bottom: 12, right: 22))
        return box
    }

    private func folderRow(path: String, name: String, count: Int, expanded: Bool, depth: Int) -> ListRow {
        let row = ListRow(height: 28)
        let x0 = 14 + CGFloat(depth) * 14
        row.paint = { r, _ in
            let h = r.bounds.height
            drawSymbol(expanded ? "chevron.down" : "chevron.right", in: NSRect(x: x0, y: 0, width: 14, height: h), color: Theme.text3, size: 8.5, weight: .bold)
            let c = String(count)
            let ca = Theme.attrs(.monoSmall, Theme.text3)
            let cw = (c as NSString).size(withAttributes: ca).width
            r.text(c, ca, x: r.bounds.width - 18 - cw, midY: h / 2, maxX: r.bounds.width - 12)
            r.text(name, Theme.attrs(depth == 0 ? .bodyMedium : .body, Theme.text), x: x0 + 18, midY: h / 2, maxX: r.bounds.width - 18 - cw - 8)
        }
        row.onClick = { [weak self] in
            guard let self else { return }
            if self.collapsed.contains(path) { self.collapsed.remove(path) } else { self.collapsed.insert(path) }
            self.reload()
        }
        row.menuProvider = { [weak self] in self?.folderMenu(path: path, isRoot: depth == 0) }
        return row
    }

    private func requestRow(id: String, name: String, method: String, depth: Int, active: Bool, dirty: Bool) -> ListRow {
        let row = ListRow(height: 28)
        row.selected = active
        let x0 = 14 + CGFloat(depth) * 14 + 4
        row.paint = { r, _ in
            let h = r.bounds.height
            let ma: [NSAttributedString.Key: Any] = [.font: Theme.monoMedium(10), .foregroundColor: Theme.method(method.isEmpty ? "GET" : method)]
            r.text(SidebarTree.methodLabel(method), ma, x: x0, midY: h / 2, maxX: x0 + 34)
            var limit = r.bounds.width - 14
            if dirty {
                Theme.orange.setFill()
                NSBezierPath(ovalIn: NSRect(x: r.bounds.width - 22, y: h / 2 - 3, width: 6, height: 6)).fill()
                limit -= 12
            }
            r.text(name, Theme.attrs(active ? .bodyMedium : .body, Theme.text), x: x0 + 38, midY: h / 2, maxX: limit)
        }
        if !id.isEmpty {
            row.onClick = { [weak self] in self?.model.open(id) }
            row.menuProvider = { [weak self] in self?.requestMenu(id: id) }
        }
        return row
    }

    // MARK: menus

    private func requestMenu(id: String) -> NSMenu? {
        guard let req = model.state.requests.first(where: { $0.id == id }) else { return nil }
        let m = ActionMenu()
        m.add("Open") { [weak self] in self?.model.open(id) }
        m.add("Rename…") { [weak self] in
            guard let self else { return }
            askText(self.window, title: "Rename request", initial: req.name, placeholder: "Request name", button: "Rename") { name in
                var r = req
                r.name = name
                Task { if let e = await self.model.renameRequest(r) { tell(self.window, "Rename", e) } }
            }
        }
        m.add("Duplicate") { [weak self] in Task { await self?.model.duplicate(id) } }
        let move = ActionMenu()
        for p in SidebarTree.destinations(model.state.collections) {
            move.add(p, checked: p == req.collection, enabled: p != req.collection) { [weak self] in
                guard let self else { return }
                Task { if let e = await self.model.move(request: id, toCollection: p) { tell(self.window, "Move", e) } }
            }
        }
        let item = NSMenuItem(title: "Move to", action: nil, keyEquivalent: "")
        item.submenu = move
        item.isEnabled = !model.state.collections.isEmpty
        m.addItem(item)
        m.separator()
        m.add("Delete") { [weak self] in
            guard let self else { return }
            confirmAction(self.window, title: "Delete “\(req.name.isEmpty ? "Untitled request" : req.name)”?", info: "The saved request is removed. This can’t be undone.", button: "Delete") {
                Task { await self.model.remove(id) }
            }
        }
        return m
    }

    private func folderMenu(path: String, isRoot: Bool) -> NSMenu {
        let m = ActionMenu()
        m.add("New request here") { [weak self] in self?.newRequest(in: path) }
        m.add("New folder…") { [weak self] in self?.newFolder(in: path) }
        m.separator()
        m.add("Rename…") { [weak self] in
            guard let self else { return }
            askText(self.window, title: isRoot ? "Rename collection" : "Rename folder", initial: SidebarTree.leaf(of: path), button: "Rename") { name in
                let to = SidebarTree.child(SidebarTree.parent(of: path), name)
                Task {
                    if let e = await self.model.renameCollection(path, to: to) { tell(self.window, "Rename", e); return }
                    self.collapsed = Set(self.collapsed.map { $0 == path ? to : ($0.hasPrefix(path + "/") ? to + $0.dropFirst(path.count) : $0) })
                    self.reload()
                }
            }
        }
        m.add("Settings…") { [weak self] in
            guard let self else { return }
            CollectionSettingsSheet.present(model: self.model, path: path, from: self.window)
        }
        m.add("Export as Postman collection…") { [weak self] in
            guard let self else { return }
            exportPostmanFile(model: self.model, environment: false, name: path, window: self.window)
        }
        m.separator()
        m.add("Delete") { [weak self] in
            guard let self else { return }
            let n = self.model.state.requests.filter { $0.collection == path || $0.collection.hasPrefix(path + "/") }.count
            let what = isRoot ? "collection" : "folder"
            confirmAction(self.window, title: "Delete “\(SidebarTree.leaf(of: path))”?",
                          info: "The \(what) and its \(n) saved request\(n == 1 ? "" : "s") are removed. This can’t be undone.", button: "Delete") {
                Task { await self.model.removeCollection(path) }
            }
        }
        return m
    }

    private func showPlusMenu() {
        let m = ActionMenu()
        if selectedTab == 1 {
            m.add("New environment…") { [weak self] in self?.newEnvironment() }
            m.separator()
        }
        m.add("New request") { [weak self] in self?.onNewRequest?() }
        m.add("New collection…") { [weak self] in self?.newCollection() }
        let folders = ActionMenu()
        for p in SidebarTree.destinations(model.state.collections) {
            folders.add(p) { [weak self] in self?.newFolder(in: p) }
        }
        let item = NSMenuItem(title: "New folder in", action: nil, keyEquivalent: "")
        item.submenu = folders
        item.isEnabled = !model.state.collections.isEmpty
        m.addItem(item)
        m.separator()
        m.add("Import cURL…") { [weak self] in self?.onImportCurl?() }
        m.add("Import Postman file…") { [weak self] in self?.importPostman() }
        m.popUp(positioning: nil, at: NSPoint(x: 0, y: plus.bounds.height + 4), in: plus)
    }

    // MARK: actions

    private func newRequest(in path: String) {
        var r = ComposerModel.blank()
        r.collection = path
        let t = model.newTab(r)
        // Starts out clean: the collection is a choice, not an edit.
        t.original = r
        collapsed.remove(path)
    }

    private func newCollection() {
        askText(window, title: "New collection", placeholder: "Collection name", button: "Create") { [weak self] name in
            guard let self else { return }
            Task { if let e = await self.model.addCollection(name) { tell(self.window, "New collection", e) } }
        }
    }

    private func newFolder(in parent: String) {
        askText(window, title: "New folder", info: "In “\(parent)”.", placeholder: "Folder name", button: "Create") { [weak self] name in
            guard let self else { return }
            Task {
                if let e = await self.model.addCollection(SidebarTree.child(parent, name)) { tell(self.window, "New folder", e); return }
                self.collapsed.remove(parent)
                self.reload()
            }
        }
    }

    private func importPostman() {
        importPostmanFile(model: model, window: window)
    }

    // MARK: environments

    private func environmentRows() -> [NSView] {
        let s = model.state
        let q = query.trimmingCharacters(in: .whitespaces)
        let envs = s.environments.filter { q.isEmpty || $0.name.localizedCaseInsensitiveContains(q) }
        var out: [NSView] = [sectionHeader("Environments")]
        if q.isEmpty { out.append(environmentRow(name: "No environment", count: nil, active: s.activeEnvironment.isEmpty, target: "")) }
        for e in envs { out.append(environmentRow(name: e.name, count: e.vars.count, active: e.name == s.activeEnvironment, target: e.name)) }
        if envs.isEmpty && !q.isEmpty { out.append(hint("No matches", "No environment matches the search.")) }
        if q.isEmpty {
            out.append(sectionHeader("Globals"))
            let g = ListRow(height: 32)
            let n = s.globals.count
            g.paint = { r, _ in
                let h = r.bounds.height
                drawSymbol("globe", in: NSRect(x: 14, y: 0, width: 18, height: h), color: Theme.text3, size: 12, weight: .medium)
                let c = n == 1 ? "1 variable" : "\(n) variables"
                let ca = Theme.attrs(.callout, Theme.text3)
                let cw = (c as NSString).size(withAttributes: ca).width
                r.text(c, ca, x: r.bounds.width - 16 - cw, midY: h / 2, maxX: r.bounds.width - 12)
                r.text("Globals", Theme.attrs(.body, Theme.text), x: 40, midY: h / 2, maxX: r.bounds.width - 16 - cw - 8)
            }
            g.onClick = { [weak self] in
                guard let self else { return }
                EnvironmentsSheet.present(model: self.model, from: self.window, selecting: nil)
            }
            out.append(g)
        }
        return out
    }

    private func sectionHeader(_ s: String) -> NSView {
        let box = FlippedView()
        box.translatesAutoresizingMaskIntoConstraints = false
        let l = overline(s)
        box.addSubview(l)
        NSLayoutConstraint.activate([
            box.heightAnchor.constraint(equalToConstant: 30),
            l.leadingAnchor.constraint(equalTo: box.leadingAnchor, constant: 20),
            l.bottomAnchor.constraint(equalTo: box.bottomAnchor, constant: -6),
        ])
        return box
    }

    private func environmentRow(name: String, count: Int?, active: Bool, target: String) -> ListRow {
        let row = ListRow(height: 32)
        row.selected = active
        let none = count == nil
        row.paint = { r, _ in
            let h = r.bounds.height
            var limit = r.bounds.width - 16
            if let count {
                let c = String(count)
                let ca = Theme.attrs(.monoSmall, Theme.text3)
                let cw = (c as NSString).size(withAttributes: ca).width
                r.text(c, ca, x: r.bounds.width - 16 - cw, midY: h / 2, maxX: r.bounds.width - 12)
                limit -= cw + 8
            }
            if active { drawSymbol("checkmark", in: NSRect(x: 14, y: 0, width: 18, height: h), color: Theme.accent, size: 11, weight: .bold) }
            r.text(name, Theme.attrs(active ? .bodyMedium : .body, none ? Theme.text2 : Theme.text), x: 40, midY: h / 2, maxX: limit)
        }
        row.onClick = { [weak self] in Task { await self?.model.setActive(target) } }
        if !none {
            row.menuProvider = { [weak self] in self?.environmentMenu(name: name, active: active) }
        }
        return row
    }

    private func environmentMenu(name: String, active: Bool) -> NSMenu {
        let m = ActionMenu()
        m.add(active ? "Deactivate" : "Make active") { [weak self] in Task { await self?.model.setActive(active ? "" : name) } }
        m.add("Edit…") { [weak self] in
            guard let self else { return }
            EnvironmentsSheet.present(model: self.model, from: self.window, selecting: name)
        }
        m.add("Duplicate") { [weak self] in
            guard let self else { return }
            var envs = self.model.state.environments
            guard let e = envs.first(where: { $0.name == name }) else { return }
            var copy = e
            var n = name + " copy", i = 2
            while envs.contains(where: { $0.name == n }) { n = "\(name) copy \(i)"; i += 1 }
            copy.name = n
            envs.append(copy)
            Task { if let err = await self.model.setEnvironments(envs, active: self.model.state.activeEnvironment) { tell(self.window, "Duplicate", err) } }
        }
        m.separator()
        m.add("Delete") { [weak self] in
            guard let self else { return }
            confirmAction(self.window, title: "Delete “\(name)”?", info: "Its variables are removed. This can’t be undone.", button: "Delete") {
                let envs = self.model.state.environments.filter { $0.name != name }
                let act = self.model.state.activeEnvironment == name ? "" : self.model.state.activeEnvironment
                Task { if let err = await self.model.setEnvironments(envs, active: act) { tell(self.window, "Delete", err) } }
            }
        }
        return m
    }

    private func newEnvironment() {
        askText(window, title: "New environment", placeholder: "Environment name", button: "Create") { [weak self] name in
            guard let self else { return }
            var envs = self.model.state.environments
            if envs.contains(where: { $0.name == name }) { tell(self.window, "New environment", "An environment named “\(name)” already exists."); return }
            envs.append(.with { $0.name = name })
            Task {
                if let e = await self.model.setEnvironments(envs, active: self.model.state.activeEnvironment) { tell(self.window, "New environment", e); return }
                EnvironmentsSheet.present(model: self.model, from: self.window, selecting: name)
            }
        }
    }

    // MARK: history

    private func historyRows() -> [NSView] {
        let entries = filteredHistory()
        if entries.isEmpty {
            return [query.trimmingCharacters(in: .whitespaces).isEmpty
                    ? hint("No history yet", "Requests you send from the Composer show up here.")
                    : hint("No matches", "No sent request matches the search.")]
        }
        var out: [NSView] = []
        for g in HistoryGrouping.groups(sentMs: entries.map(\.sentMs)) {
            out.append(sectionHeader(g.title))
            for i in g.indices { out.append(historyRow(entries[i])) }
        }
        return out
    }

    private func historyRow(_ h: Neko_Api_V1_ComposerHistoryEntry) -> ListRow {
        let row = ListRow(height: 46)
        let method = h.method.isEmpty ? h.request.method : h.method
        let url = h.url.isEmpty ? h.request.url : h.url
        let (host, path) = HistoryGrouping.pathAndHost(url)
        let failed = !h.error.isEmpty
        let statusText = failed ? "Error" : (h.status == 0 ? "–" : String(h.status))
        let color = Theme.status(h.status, failed: failed)
        let dur = failed ? "" : "  ·  " + Fmt.duration(h.durationMs)
        row.paint = { r, _ in
            let h1 = r.bounds.height / 2 - 8, h2 = r.bounds.height / 2 + 9
            let ma: [NSAttributedString.Key: Any] = [.font: Theme.monoMedium(10), .foregroundColor: Theme.method(method)]
            r.text(SidebarTree.methodLabel(method), ma, x: 16, midY: h1, maxX: 50)
            r.text(path, Theme.attrs(.mono, Theme.text), x: 52, midY: h1, maxX: r.bounds.width - 16)
            let sa: [NSAttributedString.Key: Any] = [.font: Theme.monoMedium(11), .foregroundColor: color]
            let da = Theme.attrs(.monoSmall, Theme.text3)
            let dw = (dur as NSString).size(withAttributes: da).width
            let sw = (statusText as NSString).size(withAttributes: sa).width
            let right = r.bounds.width - 16
            r.text(dur, da, x: right - dw, midY: h2, maxX: right)
            r.text(statusText, sa, x: right - dw - sw, midY: h2, maxX: right - dw)
            r.text(host.isEmpty ? url : host, Theme.attrs(.callout, Theme.text3), x: 52, midY: h2, maxX: right - dw - sw - 10)
        }
        row.onClick = { [weak self] in self?.model.openHistory(h) }
        return row
    }
}
