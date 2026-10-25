import AppKit
import NekoAPI

/// Settings of a collection or folder: notes, the authorization its requests
/// inherit, shared headers and variables. Written to the core on Save.
@MainActor
final class CollectionSettingsEditor {
    private let model: ComposerModel
    private let path: String
    private let isRoot: Bool
    private let sheet: ComposerSheet
    private var config: CollectionConfig
    private let seg = Segmented(["Overview", "Authorization", "Headers", "Variables"], selected: 0)
    private let content = ScrollColumn()

    init(model: ComposerModel, path: String) {
        self.model = model
        self.path = path
        isRoot = !path.contains("/")
        var c = model.config(for: path)
        // The top of the tree has nothing above to inherit from.
        if isRoot && c.auth.kind == .inherit { c.auth.kind = .none }
        config = c
        let what = isRoot ? "Collection" : "Folder"
        sheet = ComposerSheet(title: "\(what) settings", subtitle: path.replacingOccurrences(of: "/", with: "  ›  "), size: NSSize(width: 660, height: 560), primary: "Save")
        build()
    }

    func present(from window: NSWindow?) { sheet.present(from: window) }
    /// The sheet window, for screenshots.
    var sheetWindow: NSWindow { sheet.window }

    private func build() {
        seg.role = .callout
        seg.padding = 14
        seg.onChange = { [weak self] _ in self?.show() }
        content.translatesAutoresizingMaskIntoConstraints = false
        sheet.body.addSubview(seg)
        sheet.body.addSubview(content)
        NSLayoutConstraint.activate([
            seg.topAnchor.constraint(equalTo: sheet.body.topAnchor, constant: 16),
            seg.leadingAnchor.constraint(equalTo: sheet.body.leadingAnchor, constant: 22),
            content.topAnchor.constraint(equalTo: seg.bottomAnchor, constant: 4),
            content.leadingAnchor.constraint(equalTo: sheet.body.leadingAnchor),
            content.trailingAnchor.constraint(equalTo: sheet.body.trailingAnchor),
            content.bottomAnchor.constraint(equalTo: sheet.body.bottomAnchor),
        ])
        sheet.onPrimary = { [weak self] in self?.save() }
        show()
    }

    /// Switches the visible tab (0 Overview … 3 Variables).
    func select(tab i: Int) { seg.selected = i; show() }

    private func show() {
        let view: NSView
        switch seg.selected {
        case 1: view = authView()
        case 2: view = headersView()
        case 3: view = variablesView()
        default: view = overviewView()
        }
        content.set(view)
    }

    private func column(_ views: [NSView]) -> NSStackView {
        let col = NSStackView.vertical(views, spacing: 12)
        for v in views { v.widthAnchor.constraint(equalTo: col.widthAnchor).isActive = true }
        return col
    }

    private func note(_ s: String) -> NSTextField {
        let l = label(s, .callout, Theme.text2, lines: 0)
        l.preferredMaxLayoutWidth = 560
        return l
    }

    // MARK: overview

    private func overviewView() -> NSView {
        let area = Area(config.description_p, rows: 12, placeholder: "Notes about this \(isRoot ? "collection" : "folder"): what it is for, how to get access, who owns it.")
        area.onChange = { [weak self] t in self?.config.description_p = t }
        return column([note("Notes are kept with the \(isRoot ? "collection" : "folder") and exported to Postman as its description."), area])
    }

    // MARK: authorization

    private var kinds: [(Neko_Api_V1_ComposeAuth, String)] {
        var out: [(Neko_Api_V1_ComposeAuth, String)] = []
        if !isRoot { out.append((.inherit, "Inherit from parent")) }
        out += [(.none, "No Auth"), (.bearer, "Bearer Token"), (.basic, "Basic Auth"), (.apiKey, "API Key")]
        return out
    }

    private func authView() -> NSView {
        let current = kinds.first { $0.0 == config.auth.kind } ?? kinds[0]
        let pill = PillButton(current.1)
        pill.maxWidth = 260
        pill.menuProvider = { [weak self] in
            guard let self else { return NSMenu() }
            let m = ActionMenu()
            for (k, title) in self.kinds {
                m.add(title, checked: k == self.config.auth.kind) { [weak self] in
                    self?.config.auth.kind = k
                    self?.show()
                }
            }
            return m
        }
        let typeRow = NSStackView.horizontal([labelled("Type", pill, fill: false), NSView()], spacing: 8)
        var views: [NSView] = [typeRow]

        func field(_ title: String, _ value: String, _ placeholder: String = "", set: @escaping (String) -> Void) -> NSView {
            let f = Field(value, placeholder: placeholder)
            f.onChange = set
            return labelled(title, f)
        }

        switch config.auth.kind {
        case .bearer:
            views.append(field("Token", config.auth.token, "{{token}}") { [weak self] in self?.config.auth.token = $0 })
            views.append(field("Prefix", config.auth.prefix, "Bearer") { [weak self] in self?.config.auth.prefix = $0 })
        case .basic:
            views.append(field("Username", config.auth.user) { [weak self] in self?.config.auth.user = $0 })
            views.append(field("Password", config.auth.password) { [weak self] in self?.config.auth.password = $0 })
        case .apiKey:
            views.append(field("Key", config.auth.key, "X-API-Key") { [weak self] in self?.config.auth.key = $0 })
            views.append(field("Value", config.auth.value, "{{api_key}}") { [weak self] in self?.config.auth.value = $0 })
            let place = Segmented(["Header", "Query params"], selected: config.auth.inQuery ? 1 : 0)
            place.role = .callout
            place.onChange = { [weak self] i in self?.config.auth.inQuery = i == 1 }
            views.append(NSStackView.horizontal([labelled("Add to", place, fill: false), NSView()], spacing: 8))
        case .inherit:
            views.append(note("Requests in this folder use the authorization of the nearest collection or folder above it that sets one."))
        default:
            views.append(note("Requests here send no authorization of their own. A request can still set one itself."))
        }
        if config.auth.kind == .bearer || config.auth.kind == .basic || config.auth.kind == .apiKey {
            views.append(note("Requests that choose “Inherit from parent” use this. Variables like {{token}} are filled in when the request is sent."))
        }
        return column(views)
    }

    // MARK: headers and variables

    private func headersView() -> NSView {
        let t = KeyValueTable(config.headers, key: "Header", value: "Value", description: true)
        t.onChange = { [weak self] in self?.config.headers = $0 }
        return column([note("Sent with every request in this \(isRoot ? "collection" : "folder")."), t])
    }

    private func variablesView() -> NSView {
        let t = KeyValueTable(config.vars, key: "Variable", value: "Value", secrets: true)
        t.onChange = { [weak self] in self?.config.vars = $0 }
        return column([note("Available as {{name}} to every request in this \(isRoot ? "collection" : "folder"). The active environment overrides them."), t])
    }

    // MARK: save

    private func save() {
        var c = config
        c.headers = EnvironmentsEditor.cleaned(c.headers)
        c.vars = EnvironmentsEditor.cleaned(c.vars)
        Task { @MainActor in
            if let e = await model.setConfig(c) { tell(sheet.window, "Settings", e); return }
            sheet.close()
        }
    }
}
