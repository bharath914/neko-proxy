import AppKit
import NekoAPI

/// Held requests and responses: a list on the left, an editor on the right.
@MainActor
final class BreakpointsView: NSView {
    let admin: AdminModel
    var onNewBreakpoint: (() -> Void)?
    private var selected: UInt64?
    private var edits: [UInt64: BreakpointEdit] = [:]
    private let list = FlippedView()
    private let editor = FlippedView()
    private var signature = ""
    override var isFlipped: Bool { true }

    init(admin: AdminModel) {
        self.admin = admin
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        list.translatesAutoresizingMaskIntoConstraints = false
        editor.translatesAutoresizingMaskIntoConstraints = false
        addSubview(list); addSubview(editor)
        NSLayoutConstraint.activate([
            list.topAnchor.constraint(equalTo: topAnchor),
            list.bottomAnchor.constraint(equalTo: bottomAnchor),
            list.leadingAnchor.constraint(equalTo: leadingAnchor),
            list.widthAnchor.constraint(equalToConstant: 340),
            editor.topAnchor.constraint(equalTo: topAnchor),
            editor.bottomAnchor.constraint(equalTo: bottomAnchor),
            editor.leadingAnchor.constraint(equalTo: list.trailingAnchor, constant: 1),
            editor.trailingAnchor.constraint(equalTo: trailingAnchor),
        ])
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        Theme.content.setFill(); NSRect(x: 0, y: 0, width: 340, height: bounds.height).fill()
        Theme.grouped.setFill(); NSRect(x: 341, y: 0, width: bounds.width - 341, height: bounds.height).fill()
        Theme.separator.setFill(); NSRect(x: 340, y: 0, width: 1, height: bounds.height).fill()
    }

    func reload() {
        let items = admin.paused
        if selected == nil || !items.contains(where: { $0.flowID == selected }) { selected = items.first?.flowID }
        let sig = items.map { "\($0.flowID)" }.joined(separator: ",") + "|\(selected ?? 0)"
        guard sig != signature else { return }
        signature = sig
        edits = edits.filter { e in items.contains { $0.flowID == e.key } }
        buildList(items)
        buildEditor(items.first { $0.flowID == selected })
    }

    private func clear(_ v: NSView) { v.subviews.forEach { $0.removeFromSuperview() } }

    private func buildList(_ items: [PausedFlow]) {
        clear(list)
        let head = label("Held", .headline, Theme.text)
        list.addSubview(head)
        NSLayoutConstraint.activate([
            head.topAnchor.constraint(equalTo: list.topAnchor, constant: 18),
            head.leadingAnchor.constraint(equalTo: list.leadingAnchor, constant: 14),
        ])
        var rows: [NSView] = []
        for p in items { rows.append(row(p)) }
        let st = NSStackView.vertical(rows, spacing: 6)
        list.addSubview(st)
        NSLayoutConstraint.activate([
            st.topAnchor.constraint(equalTo: head.bottomAnchor, constant: 10),
            st.leadingAnchor.constraint(equalTo: list.leadingAnchor, constant: 10),
            st.trailingAnchor.constraint(equalTo: list.trailingAnchor, constant: -10),
        ])
        for r in rows { r.widthAnchor.constraint(equalTo: st.widthAnchor).isActive = true }
    }

    private func row(_ p: PausedFlow) -> NSView {
        let on = p.flowID == selected
        let card = Surface()
        card.fill = on ? Theme.accentSoft : Theme.card
        card.border = on ? Theme.tint(Theme.accent, 0.6) : nil
        card.radius = 8
        card.translatesAutoresizingMaskIntoConstraints = false
        let isReq = p.phase == .phaseRequest
        let method = isReq ? p.method : "\(p.status)"
        let title = NSStackView.horizontal([label(method, .bodyMedium, Theme.text), label(p.summary.host, .body, Theme.text), NSView(),
                                            label(isReq ? "Request" : "Response", .caption, Theme.orange)], spacing: 8)
        let path = label(isReq ? p.path : p.summary.path, .monoSmall, Theme.text2)
        let st = NSStackView.vertical([title, path], spacing: 4)
        card.addSubview(st)
        st.pin(to: card, insets: NSEdgeInsets(top: 10, left: 12, bottom: 10, right: 12))
        title.widthAnchor.constraint(equalTo: st.widthAnchor).isActive = true
        let click = NSClickGestureRecognizer(target: self, action: #selector(rowClicked(_:)))
        card.addGestureRecognizer(click)
        card.identifier = NSUserInterfaceItemIdentifier(String(p.flowID))
        return card
    }

    @objc private func rowClicked(_ g: NSClickGestureRecognizer) {
        guard let id = g.view?.identifier?.rawValue, let n = UInt64(id) else { return }
        selected = n
        signature = ""
        reload()
    }

    private func buildEditor(_ p: PausedFlow?) {
        clear(editor)
        guard let p else {
            let t = label("Nothing is held", .title, Theme.text)
            let s = label("A breakpoint holds a request or response here until you resume it.", .body, Theme.text2)
            let active = admin.rules.rules.filter { $0.enabled && RuleKind($0) == .breakpoint }.count
            let n = label(active == 0 ? "No breakpoint rules yet." : "\(active) breakpoint rule\(active == 1 ? "" : "s") watching.", .callout, Theme.text3)
            let b = Chip("New breakpoint", symbol: "plus", style: .accent)
            b.onClick = { [weak self] in self?.onNewBreakpoint?() }
            let st = NSStackView.vertical([t, s, n, b], spacing: 8)
            st.alignment = .centerX
            st.setCustomSpacing(18, after: n)
            editor.addSubview(st)
            NSLayoutConstraint.activate([
                st.centerXAnchor.constraint(equalTo: editor.centerXAnchor),
                st.centerYAnchor.constraint(equalTo: editor.centerYAnchor, constant: -40),
            ])
            return
        }
        let e = edits[p.flowID] ?? BreakpointEdit(p)
        edits[p.flowID] = e
        let isReq = e.isRequest

        let title = label(isReq ? "Request is held" : "Response is held", .title, Theme.text)
        var head: [NSView] = [title]
        if !p.ruleID.isEmpty { head.append(Badge("rule \(p.ruleID)", color: Theme.text2)) }
        let s = p.summary
        let scheme = s.scheme.isEmpty ? "https" : s.scheme
        let port = s.port != 0 && s.port != (scheme == "https" ? 443 : 80) ? ":\(s.port)" : ""
        let url = "\(scheme)://\(s.host)\(port)\(isReq ? p.path : s.path)"
        let secs = p.timeoutS == 0 ? 60 : p.timeoutS
        let sub = label("\(url)  ·  gives up after \(secs) s and lets it through", .callout, Theme.text2)
        let resume = Chip("Resume", symbol: "checkmark", style: .accent)
        let abort = Chip("Abort", symbol: "xmark", style: .danger)
        resume.onClick = { [weak self] in
            guard let self, let cur = self.edits[p.flowID] else { return }
            Task { await self.admin.resume(cur.resume(p)) }
        }
        abort.onClick = { [weak self] in Task { await self?.admin.abort(p.flowID) } }

        var fields: [NSView] = []
        let bind = { [weak self] (_ f: @escaping (inout BreakpointEdit, String) -> Void) -> ((String) -> Void) in
            { v in
                guard let self, var cur = self.edits[p.flowID] else { return }
                f(&cur, v); self.edits[p.flowID] = cur
            }
        }
        if isReq {
            let m = Field(e.method, width: 130); m.onChange = bind { $0.method = $1 }
            let path = Field(e.path); path.onChange = bind { $0.path = $1 }
            fields += [labelled("Method", m, fill: false), labelled("Path and query", path)]
        } else {
            let st = Field(e.status, width: 110); st.onChange = bind { $0.status = $1 }
            fields += [labelled("Status", st, fill: false)]
        }
        let hd = Area(e.headers, rows: 5); hd.onChange = bind { $0.headers = $1 }
        fields.append(labelled("Headers, one per line", hd))
        let body = Area(e.binary ? "Binary body, passed through untouched" : e.body, rows: 9)
        body.isEditable = !e.binary
        body.onChange = bind { $0.body = $1 }
        fields.append(labelled("Body", body))

        let card = Surface()
        card.fill = Theme.card; card.border = Theme.separator; card.radius = Theme.Radius.card
        card.translatesAutoresizingMaskIntoConstraints = false
        let fs = NSStackView.vertical(fields, spacing: 14)
        card.addSubview(fs)
        fs.pin(to: card, insets: NSEdgeInsets(top: 16, left: 16, bottom: 16, right: 16))
        for f in fields { f.widthAnchor.constraint(equalTo: fs.widthAnchor).isActive = true }

        let top = NSStackView.horizontal(head, spacing: 10)
        let btns = NSStackView.horizontal([resume, abort, NSView()], spacing: 8)
        let col = NSStackView.vertical([top, sub, btns, card], spacing: 10)
        col.setCustomSpacing(14, after: sub)
        col.setCustomSpacing(16, after: btns)
        editor.addSubview(col)
        NSLayoutConstraint.activate([
            col.topAnchor.constraint(equalTo: editor.topAnchor, constant: 22),
            col.leadingAnchor.constraint(equalTo: editor.leadingAnchor, constant: 24),
            col.trailingAnchor.constraint(equalTo: editor.trailingAnchor, constant: -24),
            card.widthAnchor.constraint(equalTo: col.widthAnchor),
        ])
    }
}
