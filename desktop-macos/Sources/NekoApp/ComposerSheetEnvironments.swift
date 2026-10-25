import AppKit
import NekoAPI

/// The environments and globals editor: a list of variable sets on the left,
/// the variables of the selected one on the right. Nothing is written until Save.
@MainActor
final class EnvironmentsEditor {
    private let model: ComposerModel
    private let sheet: ComposerSheet
    private var globals: [KeyValue]
    private var envs: [Environment]
    private var activeIndex: Int?
    /// 0 is Globals, n is `envs[n - 1]`.
    private var selection: Int
    private let listStack = NSStackView.vertical([], spacing: 0)
    private let listScroll = ScrollColumn()
    private let detail = ScrollColumn()
    private let removeButton = IconButton("minus", tip: "Remove environment")
    private let duplicateButton = IconButton("plus.square.on.square", tip: "Duplicate environment")
    private let exportButton = IconButton("square.and.arrow.up", tip: "Export as Postman environment")

    init(model: ComposerModel, selecting name: String?) {
        self.model = model
        let all = model.state.environments
        globals = model.state.globals
        envs = all
        activeIndex = all.firstIndex { $0.name == model.state.activeEnvironment }
        selection = name.flatMap { n in all.firstIndex { $0.name == n } }.map { $0 + 1 } ?? 0
        sheet = ComposerSheet(title: "Environments", subtitle: "Variables you reference as {{name}} in a request.", size: NSSize(width: 800, height: 560), primary: "Save")
        build()
    }

    func present(from window: NSWindow?) { sheet.present(from: window) }
    /// The sheet window, for screenshots.
    var sheetWindow: NSWindow { sheet.window }

    // MARK: layout

    private func build() {
        let left = FlippedView()
        left.translatesAutoresizingMaskIntoConstraints = false
        let rule = Divider(vertical: true)
        let bar = Divider()

        let add = IconButton("plus", tip: "New environment")
        add.onClick = { [weak self] in self?.addEnvironment() }
        removeButton.onClick = { [weak self] in self?.removeSelected() }
        duplicateButton.onClick = { [weak self] in self?.duplicateSelected() }
        let imp = IconButton("square.and.arrow.down", tip: "Import a Postman environment")
        imp.onClick = { [weak self] in self?.importFile() }
        exportButton.onClick = { [weak self] in self?.exportSelected() }
        let tools = NSStackView.horizontal([add, removeButton, duplicateButton, NSView(), imp, exportButton], spacing: 2)

        listScroll.set(listStack, insets: NSEdgeInsets(top: 10, left: 0, bottom: 10, right: 0))
        for v in [listScroll, bar, tools] as [NSView] { left.addSubview(v) }
        NSLayoutConstraint.activate([
            listScroll.topAnchor.constraint(equalTo: left.topAnchor),
            listScroll.leadingAnchor.constraint(equalTo: left.leadingAnchor),
            listScroll.trailingAnchor.constraint(equalTo: left.trailingAnchor),
            listScroll.bottomAnchor.constraint(equalTo: bar.topAnchor),
            bar.leadingAnchor.constraint(equalTo: left.leadingAnchor),
            bar.trailingAnchor.constraint(equalTo: left.trailingAnchor),
            bar.bottomAnchor.constraint(equalTo: tools.topAnchor, constant: -6),
            tools.leadingAnchor.constraint(equalTo: left.leadingAnchor, constant: 10),
            tools.trailingAnchor.constraint(equalTo: left.trailingAnchor, constant: -10),
            tools.bottomAnchor.constraint(equalTo: left.bottomAnchor, constant: -6),
        ])

        detail.translatesAutoresizingMaskIntoConstraints = false
        for v in [left, rule, detail] as [NSView] { sheet.body.addSubview(v) }
        NSLayoutConstraint.activate([
            left.topAnchor.constraint(equalTo: sheet.body.topAnchor),
            left.bottomAnchor.constraint(equalTo: sheet.body.bottomAnchor),
            left.leadingAnchor.constraint(equalTo: sheet.body.leadingAnchor),
            left.widthAnchor.constraint(equalToConstant: 224),
            rule.topAnchor.constraint(equalTo: sheet.body.topAnchor),
            rule.bottomAnchor.constraint(equalTo: sheet.body.bottomAnchor),
            rule.leadingAnchor.constraint(equalTo: left.trailingAnchor),
            detail.topAnchor.constraint(equalTo: sheet.body.topAnchor),
            detail.bottomAnchor.constraint(equalTo: sheet.body.bottomAnchor),
            detail.leadingAnchor.constraint(equalTo: rule.trailingAnchor),
            detail.trailingAnchor.constraint(equalTo: sheet.body.trailingAnchor),
        ])
        sheet.onPrimary = { [weak self] in self?.save() }
        rebuildList()
        rebuildDetail()
    }

    // MARK: list

    private func rebuildList() {
        listStack.arrangedSubviews.forEach { listStack.removeArrangedSubview($0); $0.removeFromSuperview() }
        func add(_ v: NSView) {
            listStack.addArrangedSubview(v)
            v.widthAnchor.constraint(equalTo: listStack.widthAnchor).isActive = true
        }
        let g = ListRow(height: 32)
        g.selected = selection == 0
        let gc = globals.count
        g.paint = { r, _ in
            let h = r.bounds.height
            drawSymbol("globe", in: NSRect(x: 14, y: 0, width: 18, height: h), color: Theme.text3, size: 12, weight: .medium)
            let c = String(gc)
            let ca = Theme.attrs(.monoSmall, Theme.text3)
            let cw = (c as NSString).size(withAttributes: ca).width
            r.text(c, ca, x: r.bounds.width - 18 - cw, midY: h / 2, maxX: r.bounds.width - 12)
            r.text("Globals", Theme.attrs(.bodyMedium, Theme.text), x: 40, midY: h / 2, maxX: r.bounds.width - 18 - cw - 6)
        }
        g.onClick = { [weak self] in self?.select(0) }
        add(g)

        let head = FlippedView()
        head.translatesAutoresizingMaskIntoConstraints = false
        let l = overline("Environments")
        head.addSubview(l)
        NSLayoutConstraint.activate([
            head.heightAnchor.constraint(equalToConstant: 34),
            l.leadingAnchor.constraint(equalTo: head.leadingAnchor, constant: 20),
            l.bottomAnchor.constraint(equalTo: head.bottomAnchor, constant: -6),
        ])
        add(head)

        if envs.isEmpty {
            let t = label("No environments yet. Use + to add one.", .callout, Theme.text3, lines: 0)
            t.preferredMaxLayoutWidth = 180
            let box = FlippedView()
            box.translatesAutoresizingMaskIntoConstraints = false
            box.addSubview(t)
            t.pin(to: box, insets: NSEdgeInsets(top: 4, left: 20, bottom: 4, right: 16))
            add(box)
        }
        for (i, e) in envs.enumerated() {
            let r = ListRow(height: 32)
            r.selected = selection == i + 1
            let isActive = activeIndex == i
            let count = e.vars.count
            let name = e.name.isEmpty ? "Untitled" : e.name
            r.paint = { row, _ in
                let h = row.bounds.height
                var limit = row.bounds.width - 16
                let c = String(count)
                let ca = Theme.attrs(.monoSmall, Theme.text3)
                let cw = (c as NSString).size(withAttributes: ca).width
                row.text(c, ca, x: row.bounds.width - 18 - cw, midY: h / 2, maxX: row.bounds.width - 12)
                limit -= cw + 8
                if isActive { drawSymbol("checkmark", in: NSRect(x: 14, y: 0, width: 18, height: h), color: Theme.accent, size: 11, weight: .bold) }
                row.text(name, Theme.attrs(.body, e.name.isEmpty ? Theme.text3 : Theme.text), x: 40, midY: h / 2, maxX: limit)
            }
            r.onClick = { [weak self] in self?.select(i + 1) }
            add(r)
        }
        removeButton.isEnabled = selection > 0
        duplicateButton.isEnabled = selection > 0
        exportButton.isEnabled = selection > 0
    }

    private func select(_ i: Int) {
        guard i != selection else { return }
        selection = i
        rebuildList()
        rebuildDetail()
    }

    // MARK: detail

    private func rebuildDetail() {
        var views: [NSView] = []
        let table: KeyValueTable
        if selection == 0 {
            let title = label("Globals", .headline, Theme.text)
            let sub = label("Shared by every request. A collection, folder or environment variable with the same name wins.", .callout, Theme.text2, lines: 0)
            sub.preferredMaxLayoutWidth = 520
            views += [title, sub]
            table = KeyValueTable(globals, key: "Variable", value: "Value", secrets: true)
            table.onChange = { [weak self] rows in self?.globals = rows; self?.refreshCounts() }
        } else {
            let i = selection - 1
            let name = Field(envs[i].name, placeholder: "Environment name", mono: false)
            name.onChange = { [weak self] t in
                guard let self, self.envs.indices.contains(i) else { return }
                self.envs[i].name = t
                self.rebuildList()
            }
            let head: NSStackView
            if activeIndex == i {
                head = NSStackView.horizontal([name, Badge("Active", color: Theme.green, role: .caption)], spacing: 10)
            } else {
                let use = Chip("Make active", style: .text)
                use.onClick = { [weak self] in self?.activeIndex = i; self?.rebuildList(); self?.rebuildDetail() }
                head = NSStackView.horizontal([name, use], spacing: 10)
            }
            let sub = label("Values here win over globals, collection and folder variables. Mark a value secret to mask it.", .callout, Theme.text2, lines: 0)
            sub.preferredMaxLayoutWidth = 520
            views += [head, sub]
            table = KeyValueTable(envs[i].vars, key: "Variable", value: "Value", secrets: true)
            table.onChange = { [weak self] rows in
                guard let self, self.envs.indices.contains(i) else { return }
                self.envs[i].vars = rows
                self.refreshCounts()
            }
        }
        views.append(table)
        let col = NSStackView.vertical(views, spacing: 10)
        col.setCustomSpacing(16, after: views[views.count - 2])
        for v in views { v.widthAnchor.constraint(equalTo: col.widthAnchor).isActive = true }
        detail.set(col)
    }

    /// Counts in the list follow the rows without rebuilding the editor.
    private var countTimer: Timer?
    private func refreshCounts() {
        countTimer?.invalidate()
        countTimer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: false) { [weak self] _ in
            MainActor.assumeIsolated { self?.rebuildList() }
        }
    }

    // MARK: edits

    private func uniqueName(_ base: String) -> String {
        var n = base, i = 2
        while envs.contains(where: { $0.name.caseInsensitiveCompare(n) == .orderedSame }) { n = "\(base) \(i)"; i += 1 }
        return n
    }

    private func addEnvironment() {
        envs.append(.with { $0.name = uniqueName("New environment") })
        selection = envs.count
        rebuildList()
        rebuildDetail()
    }

    private func duplicateSelected() {
        guard selection > 0 else { return }
        var copy = envs[selection - 1]
        copy.name = uniqueName(copy.name + " copy")
        envs.append(copy)
        selection = envs.count
        rebuildList()
        rebuildDetail()
    }

    private func removeSelected() {
        guard selection > 0 else { return }
        let i = selection - 1
        envs.remove(at: i)
        if activeIndex == i { activeIndex = nil } else if let a = activeIndex, a > i { activeIndex = a - 1 }
        selection = min(selection, envs.count)
        rebuildList()
        rebuildDetail()
    }

    private func importFile() {
        let before = Set(envs.map(\.name))
        importPostmanFile(model: model, window: sheet.window) { [weak self] in
            guard let self else { return }
            // The core added them already; bring them into the list without losing edits.
            for e in self.model.state.environments where !before.contains(e.name) && !self.envs.contains(where: { $0.name == e.name }) {
                self.envs.append(e)
            }
            self.selection = self.envs.count
            self.rebuildList()
            self.rebuildDetail()
        }
    }

    private func exportSelected() {
        guard selection > 0, envs.indices.contains(selection - 1) else { return }
        exportPostmanFile(model: model, environment: true, name: envs[selection - 1].name, window: sheet.window)
    }

    // MARK: save

    private func save() {
        let names = envs.map { $0.name.trimmingCharacters(in: .whitespaces) }
        if names.contains(where: \.isEmpty) { tell(sheet.window, "Environments", "Every environment needs a name."); return }
        if Set(names.map { $0.lowercased() }).count != names.count { tell(sheet.window, "Environments", "Two environments have the same name."); return }
        var out = envs
        for i in out.indices {
            out[i].name = names[i]
            out[i].vars = Self.cleaned(out[i].vars)
        }
        let active = activeIndex.flatMap { out.indices.contains($0) ? out[$0].name : nil } ?? ""
        let vars = Self.cleaned(globals)
        Task { @MainActor in
            if let e = await model.setEnvironments(out, active: active) { tell(sheet.window, "Environments", e); return }
            if let e = await model.setGlobals(vars) { tell(sheet.window, "Globals", e); return }
            sheet.close()
        }
    }

    /// Rows without a name carry nothing; drop them.
    static func cleaned(_ rows: [KeyValue]) -> [KeyValue] {
        rows.filter { !$0.name.trimmingCharacters(in: .whitespaces).isEmpty }
    }
}
