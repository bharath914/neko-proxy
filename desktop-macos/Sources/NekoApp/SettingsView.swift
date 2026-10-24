import AppKit
import NekoAPI

@MainActor
final class SettingsView: NSView {
    let model: FlowModel
    var onInstall: (() -> Void)?
    var onShow: ((String) -> Void)?
    private let scroll = NSScrollView()
    private let doc = FlippedView()
    private let column = NSStackView()
    override var isFlipped: Bool { true }

    init(model: FlowModel) {
        self.model = model
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        doc.translatesAutoresizingMaskIntoConstraints = false
        scroll.documentView = doc
        column.orientation = .vertical
        column.alignment = .leading
        column.spacing = 14
        column.translatesAutoresizingMaskIntoConstraints = false
        doc.addSubview(column)
        addSubview(scroll)
        scroll.pin(to: self)
        let clip = scroll.contentView
        let w = column.widthAnchor.constraint(equalToConstant: 700)
        w.priority = .defaultHigh
        NSLayoutConstraint.activate([
            doc.topAnchor.constraint(equalTo: clip.topAnchor),
            doc.leadingAnchor.constraint(equalTo: clip.leadingAnchor),
            doc.trailingAnchor.constraint(equalTo: clip.trailingAnchor),
            column.topAnchor.constraint(equalTo: doc.topAnchor, constant: 32),
            column.bottomAnchor.constraint(equalTo: doc.bottomAnchor, constant: -40),
            column.centerXAnchor.constraint(equalTo: doc.centerXAnchor),
            column.leadingAnchor.constraint(greaterThanOrEqualTo: doc.leadingAnchor, constant: 24),
            column.trailingAnchor.constraint(lessThanOrEqualTo: doc.trailingAnchor, constant: -24),
            w,
        ])
        rebuild()
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) { Theme.content.setFill(); bounds.fill() }

    private func full(_ v: NSView) {
        column.addArrangedSubview(v)
        v.widthAnchor.constraint(equalTo: column.widthAnchor).isActive = true
    }

    private func copy(_ s: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(s, forType: .string)
    }

    private func row(_ title: String, _ sub: String?, mono: Bool = false, trailing: [NSView] = []) -> NSView {
        let t = label(title, .body, Theme.text)
        var left: [NSView] = [t]
        if let sub {
            let s = label(sub, mono ? .mono : .callout, Theme.text2, lines: 0)
            s.preferredMaxLayoutWidth = 520
            s.isSelectable = mono
            left.append(s)
        }
        let l = NSStackView.vertical(left, spacing: 5)
        l.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        let tr = NSStackView.horizontal(trailing, spacing: 6)
        tr.setContentHuggingPriority(.required, for: .horizontal)
        let h = NSStackView.horizontal([l, NSView(), tr], spacing: 12)
        h.alignment = .centerY
        return h
    }

    private func card(_ rows: [NSView]) -> NSView {
        let c = Surface()
        c.fill = Theme.card; c.border = Theme.separator; c.radius = Theme.Radius.card
        c.translatesAutoresizingMaskIntoConstraints = false
        var parts: [NSView] = []
        for (i, r) in rows.enumerated() {
            let box = FlippedView(); box.translatesAutoresizingMaskIntoConstraints = false
            box.addSubview(r)
            r.pin(to: box, insets: NSEdgeInsets(top: 14, left: 16, bottom: 14, right: 16))
            parts.append(box)
            if i < rows.count - 1 { parts.append(Divider()) }
        }
        let st = NSStackView.vertical(parts, spacing: 0)
        c.addSubview(st)
        stretch(st)
        st.pin(to: c, insets: NSEdgeInsets(top: 1, left: 1, bottom: 1, right: 1))
        return c
    }

    /// Pins every child to the stack's leading and trailing edges.
    private func stretch(_ st: NSStackView) {
        st.alignment = .leading
        for v in st.arrangedSubviews {
            v.leadingAnchor.constraint(equalTo: st.leadingAnchor).isActive = true
            v.trailingAnchor.constraint(equalTo: st.trailingAnchor).isActive = true
        }
    }

    private func codeBlock(_ s: String) -> NSView {
        let b = Surface()
        b.fill = Theme.code; b.border = Theme.separator; b.radius = 8
        b.translatesAutoresizingMaskIntoConstraints = false
        let l = label(s, .mono, Theme.Syn.text, lines: 0)
        l.lineBreakMode = .byCharWrapping
        l.isSelectable = true
        l.preferredMaxLayoutWidth = 620
        b.addSubview(l)
        l.pin(to: b, insets: NSEdgeInsets(top: 10, left: 12, bottom: 10, right: 12))
        return b
    }

    func rebuild() {
        column.arrangedSubviews.forEach { column.removeArrangedSubview($0); $0.removeFromSuperview() }
        column.addArrangedSubview(label("Settings", .largeTitle, Theme.text))
        column.setCustomSpacing(22, after: column.arrangedSubviews.last!)
        column.addArrangedSubview(label("Appearance", .headline, Theme.text))
        column.setCustomSpacing(3, after: column.arrangedSubviews.last!)
        column.addArrangedSubview(label("Follow the system, or always use light or dark.", .callout, Theme.text2))
        let theme = Segmented(["Auto", "Light", "Dark"], selected: Appearance.current)
        theme.onChange = { Appearance.set($0) }
        let themeRow = NSStackView.horizontal([label("Theme", .body, Theme.text), NSView(), theme], spacing: 12)
        themeRow.alignment = .centerY
        full(card([themeRow]))
        column.setCustomSpacing(20, after: column.arrangedSubviews.last!)
        column.addArrangedSubview(label("Certificate", .headline, Theme.text))
        column.setCustomSpacing(3, after: column.arrangedSubviews.last!)
        column.addArrangedSubview(label("HTTPS traffic is decrypted with a certificate made for this computer. Trust it once so apps accept it.", .callout, Theme.text2))

        let ca = model.ca
        let badge: NSView
        switch model.caTrusted {
        case true?: badge = Badge("Installed", color: Theme.green)
        case false?: badge = Badge("Not installed", color: Theme.orange)
        default: badge = Badge("Checking…", color: Theme.text3)
        }
        let subject = ca?.subject.isEmpty == false ? ca!.subject : "Neko Local CA"
        let expires = ca.map { "Expires \($0.notAfter.prefix(10))" } ?? "—"
        let fp = ca?.sha256 ?? ""
        let fpShort = fp.isEmpty ? "—" : String(fp.prefix(32)) + "…"
        let copyFp = Chip("Copy", symbol: "doc.on.doc", style: .text); copyFp.onClick = { [weak self] in self?.copy(fp) }
        let path = ca?.certPath ?? CorePaths.caPath
        let copyPath = Chip("Copy path", symbol: "doc.on.doc", style: .text); copyPath.onClick = { [weak self] in self?.copy(path) }
        let show = Chip("Show", symbol: "folder", style: .text); show.onClick = { [weak self] in self?.onShow?(path) }
        let keyNote = ca?.keyStorage == "keyring"
            ? "The private key is in your macOS Keychain and never leaves this computer."
            : "The private key is stored in a file only you can read and never leaves this computer."
        full(card([
            row(subject, expires, trailing: [badge]),
            row("Fingerprint (SHA-256)", fpShort, mono: true, trailing: [copyFp]),
            row("Private key", keyNote),
            row("Certificate file", path, mono: true, trailing: [copyPath, show]),
        ]))

        let cmd = "security add-trusted-cert -r trustRoot -k ~/Library/Keychains/login.keychain-db \"\(path)\""
        let install = Chip("Install…", symbol: "shield", style: .accent); install.onClick = { [weak self] in self?.onInstall?() }
        install.isEnabled = model.caTrusted != true
        let copyCmd = Chip("Copy command", symbol: "doc.on.doc"); copyCmd.onClick = { [weak self] in self?.copy(cmd) }
        let btns = NSStackView.horizontal([install, copyCmd, NSView()], spacing: 8)
        let installBlock = NSStackView.vertical([
            label("Install for this system", .body, Theme.text),
            label("Adds the certificate to your login keychain and trusts it for SSL. macOS asks for your password.", .callout, Theme.text2, lines: 0),
            codeBlock(cmd), btns,
        ], spacing: 8)
        installBlock.setCustomSpacing(12, after: installBlock.arrangedSubviews[2])
        stretch(installBlock)

        let env = "export HTTP_PROXY=http://\(model.listen.isEmpty ? "127.0.0.1:8899" : model.listen) HTTPS_PROXY=http://\(model.listen.isEmpty ? "127.0.0.1:8899" : model.listen)\nexport SSL_CERT_FILE=\"\(path)\" CURL_CA_BUNDLE=\"\(path)\" NODE_EXTRA_CA_CERTS=\"\(path)\""
        let copyEnv = Chip("Copy", symbol: "doc.on.doc"); copyEnv.onClick = { [weak self] in self?.copy(env) }
        let envBlock = NSStackView.vertical([
            label("Terminal", .body, Theme.text),
            label("Send command-line tools through the proxy and trust its certificate.", .callout, Theme.text2),
            codeBlock(env), NSStackView.horizontal([copyEnv, NSView()], spacing: 8),
        ], spacing: 8)
        envBlock.setCustomSpacing(12, after: envBlock.arrangedSubviews[2])
        stretch(envBlock)

        let browsers = NSStackView.vertical([
            label("Safari, Chrome and Edge", .body, Theme.text),
            label("These use the macOS trust store, so installing the certificate above is all they need.", .callout, Theme.text2, lines: 0),
            label("Firefox keeps its own store. Turn on security.enterprise_roots.enabled in about:config, or import the certificate file in Settings → Privacy & Security → Certificates.", .callout, Theme.text3, lines: 0),
        ], spacing: 6)
        stretch(browsers)
        for b in browsers.arrangedSubviews { (b as? NSTextField)?.preferredMaxLayoutWidth = 640 }

        column.setCustomSpacing(20, after: column.arrangedSubviews.last!)
        full(card([installBlock, browsers, envBlock]))
        aiSection()
    }

    private func aiSection() {
        let head = label("AI clients", .headline, Theme.text)
        column.addArrangedSubview(head)
        column.setCustomSpacing(20, after: column.arrangedSubviews[column.arrangedSubviews.count - 2])
        column.setCustomSpacing(3, after: head)
        column.addArrangedSubview(label("Let a local AI client such as Claude Code read captured traffic and, with your approval, change it.", .callout, Theme.text2))
        guard let mcp = model.admin.mcp, mcp.enabled else {
            full(card([row("MCP server is off", "Start the core without --no-mcp to let AI clients connect.")]))
            return
        }
        let cmd = mcp.command.isEmpty ? "claude mcp add --transport http neko \(mcp.url) --header \"Authorization: Bearer \(mcp.token)\"" : mcp.command
        let copy = Chip("Copy command", symbol: "doc.on.doc"); copy.onClick = { [weak self] in self?.copy(cmd) }
        let connect = NSStackView.vertical([
            label("Connect Claude Code", .body, Theme.text),
            label("Run this once in a terminal. It only works from this computer.", .callout, Theme.text2),
            codeBlock(cmd), NSStackView.horizontal([copy, NSView()], spacing: 8),
        ], spacing: 8)
        connect.setCustomSpacing(12, after: connect.arrangedSubviews[2])
        stretch(connect)

        let modes: [(String, Neko_Api_V1_McpWrites)] = [("Ask me", .mcpAsk), ("Allow automatically", .mcpAuto), ("Read only", .mcpOff)]
        let seg = Segmented(modes.map(\.0), selected: modes.firstIndex { $0.1 == mcp.writes } ?? 0)
        seg.onChange = { [weak self] i in Task { await self?.model.admin.setMcpWrites(modes[i].1) } }
        let blurb: String
        switch mcp.writes {
        case .mcpAuto: blurb = "Changes from AI clients apply at once. Only use this on a machine you trust."
        case .mcpOff: blurb = "AI clients can read traffic but cannot change anything."
        default: blurb = "Every change an AI client wants to make waits for your approval."
        }
        let policy = NSStackView.vertical([
            label("Changes from AI clients", .body, Theme.text), seg, label(blurb, .callout, Theme.text2, lines: 0),
            label("Authorization, Cookie and token values are hidden before anything reaches an AI client.", .callout, Theme.text3, lines: 0),
        ], spacing: 8)
        stretch(policy)
        full(card([connect, policy]))
    }
}
