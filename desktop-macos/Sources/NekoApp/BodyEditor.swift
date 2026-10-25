import AppKit
import NekoAPI

/// The Body section: none, form-data, x-www-form-urlencoded, raw, binary, GraphQL.
/// Every edit goes through the model; switching kinds keeps the other kinds' content.
@MainActor
final class BodyEditor: NSView {
    let model: ComposerModel
    private let radios: BodyRadioRow
    private let host = FlippedView()
    /// The language raw bodies use; remembered while the person tries other kinds.
    private var rawKind: Neko_Api_V1_ComposeBody
    private var hint: NSTextField?
    private var hintWork: DispatchWorkItem?
    override var isFlipped: Bool { true }

    init(model: ComposerModel) {
        self.model = model
        let kind = model.draft.bodyKind
        rawKind = BodyMode.rawKinds.contains(kind) ? kind : .json
        radios = BodyRadioRow(BodyMode.allCases.map(\.title), selected: BodyMode(kind).rawValue)
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        host.translatesAutoresizingMaskIntoConstraints = false
        addSubview(radios)
        addSubview(host)
        NSLayoutConstraint.activate([
            radios.topAnchor.constraint(equalTo: topAnchor),
            radios.leadingAnchor.constraint(equalTo: leadingAnchor),
            host.topAnchor.constraint(equalTo: radios.bottomAnchor, constant: 8),
            host.leadingAnchor.constraint(equalTo: leadingAnchor),
            host.trailingAnchor.constraint(equalTo: trailingAnchor),
            host.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
        radios.onPick = { [weak self] i in self?.select(BodyMode(rawValue: i) ?? .none) }
        showContent()
    }
    required init?(coder: NSCoder) { fatalError() }

    private var mode: BodyMode { BodyMode(model.draft.bodyKind) }

    private func select(_ m: BodyMode) {
        let kind = m.kind(raw: rawKind)
        model.edit { $0.bodyKind = kind }
        showContent()
    }

    private func showContent() {
        hintWork?.cancel()
        hint = nil
        host.subviews.forEach { $0.removeFromSuperview() }
        radios.selected = mode.rawValue
        let v: NSView
        switch mode {
        case .none: v = noneView()
        case .formData: v = formView(files: true)
        case .urlEncoded: v = formView(files: false)
        case .raw: v = rawView()
        case .binary: v = binaryView()
        case .graphql: v = graphqlView()
        }
        host.addSubview(v)
        v.pin(to: host)
    }

    // MARK: none

    private func noneView() -> NSView {
        let box = FlippedView()
        box.translatesAutoresizingMaskIntoConstraints = false
        let l = label("This request does not have a body", .callout, Theme.text3)
        box.addSubview(l)
        NSLayoutConstraint.activate([
            l.centerXAnchor.constraint(equalTo: box.centerXAnchor),
            l.topAnchor.constraint(equalTo: box.topAnchor, constant: 36),
        ])
        return box
    }

    // MARK: forms

    private func formView(files: Bool) -> NSView {
        let table = KeyValueTable(model.draft.form, key: "Key", value: "Value", description: true, files: files, secrets: false)
        table.onChange = { [weak self] rows in self?.model.edit { $0.form = rows } }
        return bodyScroll(table)
    }

    // MARK: raw

    private func rawView() -> NSView {
        let box = FlippedView()
        box.translatesAutoresizingMaskIntoConstraints = false
        let kinds = BodyMode.rawKinds
        let popup = BodyPopup(kinds.map(BodyMode.rawTitle), selected: kinds.firstIndex(of: rawKind) ?? 0, width: 120)
        let pretty = Chip("Beautify", symbol: "text.alignleft")
        let mini = Chip("Minify", symbol: "arrow.right.to.line")
        let hintLabel = label("", .callout, Theme.text3)
        hintLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        hintLabel.setContentHuggingPriority(.defaultLow, for: .horizontal)
        hint = hintLabel
        let bar = NSStackView.horizontal([popup, hintLabel, pretty, mini], spacing: 8)
        bar.distribution = .fill
        let editor = BodyCodeEditor(model.draft.body, language: BodySyntax.Language(rawKind), placeholder: Self.placeholder(rawKind), minHeight: 160)
        box.addSubview(bar)
        box.addSubview(editor)
        NSLayoutConstraint.activate([
            bar.topAnchor.constraint(equalTo: box.topAnchor),
            bar.leadingAnchor.constraint(equalTo: box.leadingAnchor),
            bar.trailingAnchor.constraint(equalTo: box.trailingAnchor),
            editor.topAnchor.constraint(equalTo: bar.bottomAnchor, constant: 8),
            editor.leadingAnchor.constraint(equalTo: box.leadingAnchor),
            editor.trailingAnchor.constraint(equalTo: box.trailingAnchor),
            editor.bottomAnchor.constraint(equalTo: box.bottomAnchor, constant: -2),
        ])

        func refreshTools() {
            let json = self.rawKind == .json
            pretty.isEnabled = json
            mini.isEnabled = json
            pretty.toolTip = json ? nil : "Beautify works for JSON"
            mini.toolTip = pretty.toolTip
        }
        refreshTools()
        refreshHint(editor.text, immediately: true)

        popup.onPick = { [weak self, weak editor] i in
            guard let self, let editor else { return }
            self.rawKind = kinds[i]
            self.model.edit { $0.bodyKind = kinds[i] }
            editor.language = BodySyntax.Language(kinds[i])
            editor.placeholder = Self.placeholder(kinds[i])
            refreshTools()
            self.refreshHint(editor.text, immediately: true)
        }
        editor.onChange = { [weak self] t in
            self?.model.edit { $0.body = t }
            self?.refreshHint(t, immediately: false)
        }
        pretty.onClick = { [weak self, weak editor] in
            guard let self, let editor else { return }
            if let s = JSONFormat.pretty(editor.text) { editor.replaceAll(s) }
            else { self.showHint(JSONFormat.error(editor.text) ?? "Invalid JSON", red: true, prefix: "Can't beautify: ") }
        }
        mini.onClick = { [weak self, weak editor] in
            guard let self, let editor else { return }
            if let s = JSONFormat.minified(editor.text) { editor.replaceAll(s) }
            else { self.showHint(JSONFormat.error(editor.text) ?? "Invalid JSON", red: true, prefix: "Can't minify: ") }
        }
        return box
    }

    private static func placeholder(_ k: Neko_Api_V1_ComposeBody) -> String {
        k == .json ? "{\n  \"name\": \"value\"\n}" : "Request body"
    }

    private func showHint(_ text: String, red: Bool, prefix: String = "") {
        guard let hint else { return }
        hint.set(text.isEmpty ? "" : prefix + text, .callout, red ? Theme.red : Theme.text3)
    }

    /// An "Invalid JSON" line under the toolbar: muted while typing, red once the
    /// person pauses for 400 ms.
    private func refreshHint(_ text: String, immediately: Bool) {
        hintWork?.cancel()
        guard rawKind == .json, (text as NSString).length <= BodySyntax.limit else { showHint("", red: false); return }
        if immediately {
            if let m = JSONFormat.validationMessage(text) { showHint(m, red: true, prefix: "Invalid JSON: ") } else { showHint("", red: false) }
            return
        }
        // While typing, only dim what is already showing.
        if hint?.stringValue.isEmpty == false { showHint(hint?.stringValue ?? "", red: false) }
        let w = DispatchWorkItem { [weak self] in
            guard let self, self.rawKind == .json else { return }
            if let m = JSONFormat.validationMessage(text) { self.showHint(m, red: true, prefix: "Invalid JSON: ") } else { self.showHint("", red: false) }
        }
        hintWork = w
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.4, execute: w)
    }

    // MARK: binary

    private func binaryView() -> NSView {
        let box = FlippedView()
        box.translatesAutoresizingMaskIntoConstraints = false
        let path = model.draft.binaryPath
        let card = Surface()
        card.fill = Theme.card
        card.border = Theme.separator
        card.radius = Theme.Radius.card
        card.translatesAutoresizingMaskIntoConstraints = false

        let title: NSTextField, sub: NSTextField
        if path.isEmpty {
            title = label("No file selected", .bodyMedium, Theme.text)
            sub = label("The file is sent as the raw request body.", .callout, Theme.text2)
        } else {
            let url = URL(fileURLWithPath: (path as NSString).expandingTildeInPath)
            let attrs = try? FileManager.default.attributesOfItem(atPath: url.path)
            title = label(url.lastPathComponent, .bodyMedium, Theme.text)
            if let size = attrs?[.size] as? NSNumber {
                sub = label("\(ByteCountFormatter.string(fromByteCount: size.int64Value, countStyle: .file))  ·  \(path)", .callout, Theme.text2)
            } else {
                sub = label("File not found  ·  \(path)", .callout, Theme.red)
            }
            sub.lineBreakMode = .byTruncatingMiddle
        }
        sub.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        let icon = NSImageView(image: symbol(path.isEmpty ? "doc" : "doc.fill", size: 22, weight: .regular) ?? NSImage())
        icon.contentTintColor = Theme.text3
        icon.translatesAutoresizingMaskIntoConstraints = false
        icon.widthAnchor.constraint(equalToConstant: 28).isActive = true

        let pick = Chip("Select file…", symbol: "folder")
        let clear = Chip("Clear", style: .danger)
        clear.isEnabled = !path.isEmpty
        pick.onClick = { [weak self] in self?.chooseFile() }
        clear.onClick = { [weak self] in
            self?.model.edit { $0.binaryPath = "" }
            self?.showContent()
        }
        let text = NSStackView.vertical([title, sub], spacing: 3)
        text.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        let row = NSStackView.horizontal([icon, text, NSView(), pick, clear], spacing: 12)
        card.addSubview(row)
        row.pin(to: card, insets: NSEdgeInsets(top: 16, left: 16, bottom: 16, right: 16))
        box.addSubview(card)
        let fill = card.widthAnchor.constraint(equalTo: box.widthAnchor)
        fill.priority = .defaultHigh
        NSLayoutConstraint.activate([
            card.topAnchor.constraint(equalTo: box.topAnchor, constant: 2),
            card.leadingAnchor.constraint(equalTo: box.leadingAnchor),
            card.widthAnchor.constraint(lessThanOrEqualToConstant: 640),
            fill,
        ])
        return box
    }

    private func chooseFile() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.prompt = "Select"
        let done: (NSApplication.ModalResponse) -> Void = { [weak self] r in
            guard r == .OK, let url = panel.url else { return }
            self?.model.edit { $0.binaryPath = url.path }
            self?.showContent()
        }
        if let w = window { panel.beginSheetModal(for: w, completionHandler: done) } else { done(panel.runModal()) }
    }

    // MARK: GraphQL

    private func graphqlView() -> NSView {
        let box = FlippedView()
        box.translatesAutoresizingMaskIntoConstraints = false
        let query = BodyCodeEditor(model.draft.body, language: .graphql, placeholder: "query {\n  viewer { id }\n}", minHeight: 120)
        let vars = BodyCodeEditor(model.draft.graphqlVariables, language: .json, placeholder: "{ \"id\": 1 }", minHeight: 80)
        query.onChange = { [weak self] t in self?.model.edit { $0.body = t } }
        vars.onChange = { [weak self] t in self?.model.edit { $0.graphqlVariables = t } }
        let qh = overline("Query"), vh = overline("Variables")
        let note = label("Sent as JSON {\"query\", \"variables\"} with Content-Type application/json.", .callout, Theme.text3)
        for v in [qh, query, vh, vars, note] as [NSView] { box.addSubview(v) }
        let ratio = query.heightAnchor.constraint(equalTo: vars.heightAnchor, multiplier: 1.5)
        ratio.priority = NSLayoutConstraint.Priority(900)
        NSLayoutConstraint.activate([
            qh.topAnchor.constraint(equalTo: box.topAnchor),
            qh.leadingAnchor.constraint(equalTo: box.leadingAnchor, constant: 2),
            query.topAnchor.constraint(equalTo: qh.bottomAnchor, constant: 6),
            vh.topAnchor.constraint(equalTo: query.bottomAnchor, constant: 12),
            vh.leadingAnchor.constraint(equalTo: box.leadingAnchor, constant: 2),
            vars.topAnchor.constraint(equalTo: vh.bottomAnchor, constant: 6),
            note.topAnchor.constraint(equalTo: vars.bottomAnchor, constant: 8),
            note.leadingAnchor.constraint(equalTo: box.leadingAnchor, constant: 2),
            note.trailingAnchor.constraint(lessThanOrEqualTo: box.trailingAnchor),
            note.bottomAnchor.constraint(equalTo: box.bottomAnchor, constant: -2),
            ratio,
        ])
        for e in [query, vars] {
            e.leadingAnchor.constraint(equalTo: box.leadingAnchor).isActive = true
            e.trailingAnchor.constraint(equalTo: box.trailingAnchor).isActive = true
        }
        return box
    }
}
