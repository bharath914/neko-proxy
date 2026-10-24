import AppKit
import NekoAPI

/// Mock, map remote, rewrite, throttle and breakpoint rules, with the inline editor.
@MainActor
final class RulesView: Page {
    let admin: AdminModel
    private var form: RuleForm?
    private var error: String?
    private var local: LocalForm?
    private var localError: String?
    private var ssl: SslDraft?
    private var sslError: String?
    var onToast: ((String) -> Void)?

    init(admin: AdminModel) {
        self.admin = admin
        super.init(maxWidth: 780)
    }
    required init?(coder: NSCoder) { fatalError() }

    private static func color(_ k: RuleKind?) -> NSColor {
        switch k {
        case .mock?: return Theme.purple
        case .mapRemote?: return Theme.teal
        case .rewrite?: return Theme.indigo
        case .throttle?: return Theme.orange
        case .breakpoint?: return Theme.red
        case nil: return Theme.green
        }
    }

    /// Open the editor, optionally pre-filled to match a flow.
    func startNew(_ kind: RuleKind, flow: Flow? = nil) {
        form = flow.map { RuleForm(kind: kind, flow: $0) } ?? RuleForm(kind: kind)
        error = nil
        rebuild()
    }

    func cancelEditing() { form = nil; error = nil; local = nil; localError = nil; rebuild() }

    func reload() {
        if form != nil || local != nil || ssl != nil { return }  // keep typing undisturbed; Save and Cancel rebuild
        rebuild()
    }

    private func rebuild() {
        var views: [NSView] = []
        let title = label("Rules", .largeTitle, Theme.text)
        let new = Chip("New rule", symbol: "plus", style: .accent)
        new.onClick = { [weak self] in self?.startNew(.mock) }
        let newLocal = Chip("Map local", symbol: "doc", style: .text)
        newLocal.onClick = { [weak self] in self?.local = LocalForm(); self?.localError = nil; self?.rebuild() }
        let sslChip = Chip("SSL proxying", symbol: "lock", style: .text)
        sslChip.onClick = { [weak self] in
            guard let self else { return }
            self.ssl = SslDraft(self.admin.rules.ssl); self.sslError = nil; self.rebuild()
        }
        let head = NSStackView.horizontal([title, NSView(), sslChip, newLocal, new], spacing: 8)
        views.append(head)
        let d = label("Rules change traffic as it passes through. They apply to new requests, in the order listed.", .callout, Theme.text2)
        views.append(d)
        if ssl != nil { views.append(sslCard()) }
        if local != nil { views.append(localCard()) }
        if form != nil { views.append(editorCard()) }
        views.append(listCard())
        setContent(views)
        column.setCustomSpacing(4, after: head)
        column.setCustomSpacing(16, after: d)
    }

    // MARK: list

    private func listCard() -> NSView {
        let rules = admin.rules.rules
        let local = admin.rules.mapLocal
        if rules.isEmpty && local.isEmpty {
            let t = label("No rules yet", .headline, Theme.text)
            let s = label("Mock a response, point a host somewhere else, add latency, rewrite headers, or hold a request at a breakpoint. You can also ask a connected AI client to do it (Settings → AI).", .callout, Theme.text2, lines: 0)
            s.preferredMaxLayoutWidth = 680
            return groupCard([NSStackView.vertical([t, s], spacing: 6)], padding: NSEdgeInsets(top: 20, left: 20, bottom: 20, right: 20))
        }
        var rows: [NSView] = rules.map(ruleRow)
        rows += local.map(localRow)
        return groupCard(rows, padding: NSEdgeInsets(top: 12, left: 16, bottom: 12, right: 16))
    }

    private func ruleRow(_ r: Rule) -> NSView {
        let kind = RuleKind(r)
        let sw = Switch(on: r.enabled)
        sw.onToggle = { [weak self] on in Task { await self?.admin.setEnabled(r.id, on) } }
        let name = r.name.isEmpty ? describeMatch(r.matcher) : r.name
        let first = NSStackView.horizontal([Badge(kind?.label ?? "Rule", color: Self.color(kind), role: .caption),
                                            label(name, .bodyMedium, r.enabled ? Theme.text : Theme.text3)], spacing: 8)
        let sub = label("\(describeAction(r))  ·  \(describeMatch(r.matcher))", .callout, r.enabled ? Theme.text2 : Theme.text3)
        let text = NSStackView.vertical([first, sub], spacing: 4)
        let edit = Chip("Edit", style: .text)
        edit.onClick = { [weak self] in self?.form = RuleForm(r); self?.error = nil; self?.rebuild() }
        let trash = IconButton("trash", tip: "Remove rule")
        trash.onClick = { [weak self] in Task { await self?.admin.remove(r.id) } }
        let row = NSStackView.horizontal([sw, text, NSView(), edit, trash], spacing: 12)
        return row
    }

    private func localRow(_ m: Neko_Api_V1_MapLocalRule) -> NSView {
        let sw = Switch(on: m.enabled)
        sw.onToggle = { [weak self] on in Task { await self?.admin.setEnabled(m.id, on) } }
        let first = NSStackView.horizontal([Badge("Map local", color: Self.color(nil), role: .caption),
                                            label(m.host + m.path, .bodyMedium, m.enabled ? Theme.text : Theme.text3)], spacing: 8)
        let sub = label("Answer from \((m.file as NSString).lastPathComponent)", .callout, m.enabled ? Theme.text2 : Theme.text3)
        let edit = Chip("Edit", style: .text)
        edit.onClick = { [weak self] in self?.local = LocalForm(m); self?.localError = nil; self?.rebuild() }
        let trash = IconButton("trash", tip: "Remove rule")
        trash.onClick = { [weak self] in Task { await self?.admin.remove(m.id) } }
        return NSStackView.horizontal([sw, NSStackView.vertical([first, sub], spacing: 4), NSView(), edit, trash], spacing: 12)
    }

    // MARK: editor

    private func bound(_ kp: WritableKeyPath<RuleForm, String>, _ placeholder: String, width: CGFloat? = nil) -> Field {
        let f = Field(form?[keyPath: kp] ?? "", placeholder: placeholder, width: width)
        f.onChange = { [weak self] v in self?.form?[keyPath: kp] = v }
        return f
    }

    private func boundArea(_ kp: WritableKeyPath<RuleForm, String>, rows: Int, _ placeholder: String) -> Area {
        let a = Area(form?[keyPath: kp] ?? "", rows: rows, placeholder: placeholder)
        a.onChange = { [weak self] v in self?.form?[keyPath: kp] = v }
        return a
    }

    private func editorCard() -> NSView {
        guard let f = form else { return NSView() }
        var rows: [NSView] = [label(f.id.isEmpty ? "New rule" : "Edit rule", .headline, Theme.text)]
        if f.id.isEmpty {
            let seg = Segmented(RuleKind.allCases.map(\.label), selected: f.kind.rawValue)
            seg.onChange = { [weak self] i in self?.form?.kind = RuleKind(rawValue: i) ?? .mock; self?.rebuild() }
            rows.append(seg)
        }
        rows.append(label(f.kind.hint, .callout, Theme.text2))
        rows.append(labelled("Name (optional)", bound(\.name, "Checkout is down")))
        let m1 = NSStackView.horizontal([bound(\.method, "GET", width: 84), bound(\.host, "host: api.example.com or *", width: 250), bound(\.port, "port", width: 74)], spacing: 8)
        let m2 = NSStackView.horizontal([bound(\.path, "path: /v1/* (empty = any)", width: 290), bound(\.query, "query (exact)", width: 170), bound(\.scheme, "https", width: 74)], spacing: 8)
        let match = NSStackView.vertical([m1, m2], spacing: 8)
        rows.append(labelled("Applies to requests where", match, fill: false))
        switch f.kind {
        case .mock:
            let st = NSStackView.horizontal([bound(\.status, "200", width: 74), bound(\.delayMs, "delay ms", width: 114)], spacing: 8)
            rows.append(labelled("Respond with", st, fill: false))
            rows.append(labelled("Headers, one per line", boundArea(\.headers, rows: 3, "content-type: application/json")))
            rows.append(labelled("Body", boundArea(\.body, rows: 6, "{\"ok\": true}")))
        case .mapRemote:
            let st = NSStackView.horizontal([bound(\.toScheme, "https", width: 84), bound(\.toHost, "staging.example.com", width: 270), bound(\.toPort, "port", width: 84)], spacing: 8)
            rows.append(labelled("Send to (leave empty to keep)", st, fill: false))
        case .rewrite:
            rows.append(labelled("Request headers: set, add or remove, one per line", boundArea(\.requestEdits, rows: 3, "set X-Env: staging\nremove Cookie")))
            rows.append(labelled("Response headers", boundArea(\.responseEdits, rows: 3, "set Cache-Control: no-store")))
        case .throttle:
            rows.append(labelled("Delay each request by (ms)", bound(\.delayMs, "800", width: 114), fill: false))
        case .breakpoint:
            let seg = Segmented(["Request and response", "Request", "Response"], selected: min(f.phase, 2))
            seg.onChange = { [weak self] i in self?.form?.phase = i }
            let st = NSStackView.horizontal([seg, bound(\.timeoutS, "give up after s (60)", width: 160)], spacing: 8)
            rows.append(labelled("Hold the", st, fill: false))
        }
        if let error { rows.append(label(error, .callout, Theme.red, lines: 2)) }
        let save = Chip("Save", symbol: "checkmark", style: .accent)
        save.onClick = { [weak self] in self?.save() }
        let cancel = Chip("Cancel", style: .text)
        cancel.onClick = { [weak self] in self?.form = nil; self?.error = nil; self?.rebuild() }
        rows.append(NSStackView.horizontal([save, cancel, NSView()], spacing: 8))

        let card = Surface()
        card.fill = Theme.card; card.border = Theme.accent; card.radius = Theme.Radius.card
        card.translatesAutoresizingMaskIntoConstraints = false
        let st = NSStackView.vertical(rows, spacing: 10)
        card.addSubview(st)
        st.pin(to: card, insets: NSEdgeInsets(top: 16, left: 18, bottom: 16, right: 18))
        for r in rows where !(r is Segmented) && r !== rows.last {
            r.widthAnchor.constraint(equalTo: st.widthAnchor).isActive = true
        }
        rows.last?.widthAnchor.constraint(equalTo: st.widthAnchor).isActive = true
        return card
    }

    // MARK: SSL proxying

    private func card(_ rows: [NSView]) -> NSView {
        let card = Surface()
        card.fill = Theme.card; card.border = Theme.accent; card.radius = Theme.Radius.card
        card.translatesAutoresizingMaskIntoConstraints = false
        let st = NSStackView.vertical(rows, spacing: 10)
        card.addSubview(st)
        st.pin(to: card, insets: NSEdgeInsets(top: 16, left: 18, bottom: 16, right: 18))
        for r in rows where !(r is Segmented) { r.widthAnchor.constraint(equalTo: st.widthAnchor).isActive = true }
        return card
    }

    private func sslCard() -> NSView {
        guard let d = ssl else { return NSView() }
        var rows: [NSView] = [label("SSL proxying", .headline, Theme.text)]
        rows.append(label("Choose which HTTPS hosts Neko decrypts. Other hosts are tunneled untouched. Patterns: example.com, *.example.com, api.example.com:8443.", .callout, Theme.text2, lines: 0))
        let seg = Segmented(["Decrypt all hosts", "Only listed hosts"], selected: d.listed ? 1 : 0)
        seg.onChange = { [weak self] i in self?.ssl?.listed = (i == 1) }
        rows.append(seg)
        let hosts = Area(d.hosts, rows: 4, placeholder: "api.example.com\n*.example.com")
        hosts.onChange = { [weak self] v in self?.ssl?.hosts = v }
        rows.append(labelled("Decrypt these hosts, one per line (used by \"Only listed hosts\")", hosts))
        let excl = Area(d.exclude, rows: 3, placeholder: "bank.example.com")
        excl.onChange = { [weak self] v in self?.ssl?.exclude = v }
        rows.append(labelled("Never decrypt these hosts, one per line", excl))
        if let sslError { rows.append(label(sslError, .callout, Theme.red, lines: 2)) }
        let save = Chip("Save", symbol: "checkmark", style: .accent)
        save.onClick = { [weak self] in self?.saveSsl() }
        let cancel = Chip("Cancel", style: .text)
        cancel.onClick = { [weak self] in self?.ssl = nil; self?.sslError = nil; self?.rebuild() }
        rows.append(NSStackView.horizontal([save, cancel, NSView()], spacing: 8))
        return card(rows)
    }

    private func saveSsl() {
        guard let d = ssl else { return }
        Task {
            if let why = await admin.setSsl(d.settings) { sslError = why } else { ssl = nil; sslError = nil }
            rebuild()
        }
    }

    // MARK: Map Local

    private func localCard() -> NSView {
        guard let f = local else { return NSView() }
        func bind(_ kp: WritableKeyPath<LocalForm, String>, _ ph: String, width: CGFloat? = nil) -> Field {
            let fld = Field(local?[keyPath: kp] ?? "", placeholder: ph, width: width)
            fld.onChange = { [weak self] v in self?.local?[keyPath: kp] = v }
            return fld
        }
        var rows: [NSView] = [label(f.id.isEmpty ? "New Map Local rule" : "Edit Map Local rule", .headline, Theme.text)]
        rows.append(label("Answer matching requests from a file on this Mac. The file is read on every request, so edits show up immediately.", .callout, Theme.text2, lines: 0))
        let m = NSStackView.horizontal([bind(\.host, "host: api.example.com", width: 250), bind(\.port, "port", width: 74),
                                        bind(\.path, "path: /v1/me (or /v1/*)", width: 240)], spacing: 8)
        rows.append(labelled("Applies to requests where", m, fill: false))
        rows.append(labelled("Query (exact, empty = any)", bind(\.query, "page=1", width: 240), fill: false))
        let file = bind(\.file, "/Users/you/mock/me.json", width: 420)
        let pick = Chip("Choose…", symbol: "folder", style: .text)
        pick.onClick = { [weak self, weak file] in
            let panel = NSOpenPanel()
            panel.canChooseFiles = true; panel.canChooseDirectories = false; panel.allowsMultipleSelection = false
            panel.message = "Choose the file to serve"
            if panel.runModal() == .OK, let url = panel.url {
                self?.local?.file = url.path
                file?.text = url.path
            }
        }
        rows.append(labelled("Serve this file", NSStackView.horizontal([file, pick], spacing: 8), fill: false))
        let st = NSStackView.horizontal([bind(\.status, "200", width: 74), bind(\.contentType, "content type (from extension)", width: 250)], spacing: 8)
        rows.append(labelled("Status and content type", st, fill: false))
        if let localError { rows.append(label(localError, .callout, Theme.red, lines: 2)) }
        let save = Chip("Save", symbol: "checkmark", style: .accent)
        save.onClick = { [weak self] in self?.saveLocal() }
        let cancel = Chip("Cancel", style: .text)
        cancel.onClick = { [weak self] in self?.local = nil; self?.localError = nil; self?.rebuild() }
        rows.append(NSStackView.horizontal([save, cancel, NSView()], spacing: 8))
        return card(rows)
    }

    private func saveLocal() {
        guard let f = local else { return }
        do {
            let rule = try f.toRule()
            Task {
                if let why = await admin.putMapLocal(rule) { localError = why } else { local = nil; localError = nil }
                rebuild()
            }
        } catch let e as LocalForm.Invalid {
            localError = e.message; rebuild()
        } catch {
            localError = error.localizedDescription; rebuild()
        }
    }

    private func save() {
        guard let f = form else { return }
        do {
            let rule = try f.toRule()
            Task {
                if let why = await admin.put(rule) {
                    error = why
                } else {
                    form = nil; error = nil
                }
                rebuild()
            }
        } catch let e as RuleForm.Invalid {
            error = e.message
            rebuild()
        } catch {
            self.error = error.localizedDescription
            rebuild()
        }
    }
}
