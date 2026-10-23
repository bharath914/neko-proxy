import AppKit
import ProxyAPI

/// A rounded card of key/value rows.
func infoCard(_ rows: [(String, String, Bool)]) -> NSView {
    let card = Surface()
    card.fill = Theme.card
    card.border = Theme.separator
    card.radius = Theme.Radius.card
    card.translatesAutoresizingMaskIntoConstraints = false
    var views: [NSView] = []
    for (k, v, mono) in rows {
        let key = label(k, .body, Theme.text2)
        key.widthAnchor.constraint(equalToConstant: 84).isActive = true
        let val = label(v, mono ? .mono : .body, Theme.text, lines: 3)
        val.lineBreakMode = .byCharWrapping
        val.isSelectable = true
        val.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        let row = NSStackView.horizontal([key, val], spacing: 12, alignment: .firstBaseline)
        views.append(row)
    }
    let stack = NSStackView.vertical(views, spacing: 11)
    card.addSubview(stack)
    stack.pin(to: card, insets: NSEdgeInsets(top: 12, left: 14, bottom: 12, right: 14))
    return card
}

func overline(_ s: String) -> NSTextField { label(s.uppercased(), .overline, Theme.text3) }

/// A bar per timing phase, scaled to the largest.
final class TimingView: Surface {
    private let rows: [(String, Double, NSColor)]
    private let total: Double
    init(_ t: Proxy_Api_V1_Timings) {
        rows = [("DNS", t.dnsMs, Theme.purple), ("Connect", t.connectMs, Theme.orange), ("TLS", t.tlsMs, Theme.pink),
                ("Send", t.sendMs, Theme.teal), ("Wait", t.waitMs, Theme.accent), ("Receive", t.receiveMs, Theme.green)]
        total = max(t.totalMs, rows.reduce(0) { $0 + max($1.1, 0) }, 0.001)
        super.init(frame: .zero)
        fill = Theme.card; border = Theme.separator; radius = Theme.Radius.card
        translatesAutoresizingMaskIntoConstraints = false
        heightAnchor.constraint(equalToConstant: CGFloat(rows.count) * 30 + 56).isActive = true
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        let labelW: CGFloat = 70, valueW: CGFloat = 62
        let trackX = 14 + labelW, trackW = bounds.width - trackX - valueW - 14
        var y: CGFloat = 14
        var offset = 0.0
        for (name, ms, color) in rows {
            let la = Theme.attrs(.body, Theme.text2)
            let sz = (name as NSString).size(withAttributes: la)
            (name as NSString).draw(at: NSPoint(x: 14, y: y + 15 - sz.height / 2), withAttributes: la)
            Theme.fill.setFill()
            NSBezierPath(roundedRect: NSRect(x: trackX, y: y + 10, width: trackW, height: 10), xRadius: 5, yRadius: 5).fill()
            if ms > 0 {
                let x = trackX + trackW * CGFloat(offset / total)
                let w = max(trackW * CGFloat(ms / total), 4)
                color.setFill()
                NSBezierPath(roundedRect: NSRect(x: x, y: y + 10, width: w, height: 10), xRadius: 5, yRadius: 5).fill()
                offset += ms
            }
            let va = Theme.attrs(.mono, ms >= 0 ? Theme.text : Theme.text3)
            let vs = (ms >= 0 ? Fmt.duration(ms) : "—") as NSString
            let vz = vs.size(withAttributes: va)
            vs.draw(at: NSPoint(x: bounds.width - 14 - vz.width, y: y + 15 - vz.height / 2), withAttributes: va)
            y += 30
        }
        Theme.separator.setFill()
        NSRect(x: 14, y: y + 6, width: bounds.width - 28, height: 1).fill()
        let ta = Theme.attrs(.bodyMedium, Theme.text)
        ("Total" as NSString).draw(at: NSPoint(x: 14, y: y + 20), withAttributes: ta)
        let tv = Fmt.duration(total > 0.001 ? total : -1) as NSString
        let tz = tv.size(withAttributes: Theme.attrs(.mono, Theme.text))
        tv.draw(at: NSPoint(x: bounds.width - 14 - tz.width, y: y + 21), withAttributes: Theme.attrs(.mono, Theme.text))
    }
}

/// A clickable section header: chevron, overline title, count.
final class DisclosureHeader: NSView {
    var open: Bool { didSet { needsDisplay = true } }
    var onToggle: (() -> Void)?
    private let title: String, count: Int
    override var isFlipped: Bool { true }

    init(_ title: String, count: Int, open: Bool) {
        self.title = title; self.count = count; self.open = open
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        heightAnchor.constraint(equalToConstant: 28).isActive = true
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        drawSymbol(open ? "chevron.down" : "chevron.right", in: NSRect(x: 0, y: 0, width: 14, height: bounds.height), color: Theme.text3, size: 9, weight: .bold)
        let a = Theme.attrs(.overline, Theme.text3)
        let t = title.uppercased() as NSString
        let sz = t.size(withAttributes: a)
        t.draw(at: NSPoint(x: 20, y: bounds.midY - sz.height / 2), withAttributes: a)
        let c = String(count) as NSString
        let ca = Theme.attrs(.mono, Theme.text3)
        let cz = c.size(withAttributes: ca)
        c.draw(at: NSPoint(x: bounds.maxX - cz.width, y: bounds.midY - cz.height / 2), withAttributes: ca)
    }
    override func mouseUp(with event: NSEvent) { onToggle?() }
}

@MainActor
final class DetailPane: NSView {
    private(set) var detail: FlowDetail?
    var onCopyCurl: (() -> Void)?
    var onCopyURL: (() -> Void)?
    var onReplay: (() -> Void)?
    var onCertSettings: (() -> Void)?

    private let header = FlippedView()
    private let tabs = Segmented(["Overview", "Request", "Response", "Timing"])
    private let scroll = NSScrollView()
    private let doc = FlippedView()
    private let stack = NSStackView()
    private let placeholder = NSStackView()
    private var headerStack: NSStackView?
    private var headerBottom: NSLayoutConstraint?
    private var open: Set<String> = []
    private var raw: Set<String> = []
    private var folds: [String: Set<Int>] = [:]
    private var sep = Divider()
    override var isFlipped: Bool { true }

    override init(frame: NSRect) {
        super.init(frame: frame)
        translatesAutoresizingMaskIntoConstraints = false
        header.translatesAutoresizingMaskIntoConstraints = false
        tabs.equalWidths = true
        tabs.onChange = { [weak self] _ in self?.rebuild(keepScroll: false) }
        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        doc.translatesAutoresizingMaskIntoConstraints = false
        scroll.documentView = doc
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 8
        stack.translatesAutoresizingMaskIntoConstraints = false
        doc.addSubview(stack)

        let pt = label("Select a request", .headline, Theme.text2)
        let ps = label("Its headers, body and timing appear here.", .callout, Theme.text3)
        placeholder.orientation = .vertical
        placeholder.alignment = .centerX
        placeholder.spacing = 4
        placeholder.addArrangedSubview(pt)
        placeholder.addArrangedSubview(ps)
        placeholder.translatesAutoresizingMaskIntoConstraints = false

        addSubview(header); addSubview(sep); addSubview(scroll); addSubview(placeholder)
        let clip = scroll.contentView
        NSLayoutConstraint.activate([
            header.topAnchor.constraint(equalTo: topAnchor),
            header.leadingAnchor.constraint(equalTo: leadingAnchor),
            header.trailingAnchor.constraint(equalTo: trailingAnchor),
            sep.topAnchor.constraint(equalTo: header.bottomAnchor),
            sep.leadingAnchor.constraint(equalTo: leadingAnchor),
            sep.trailingAnchor.constraint(equalTo: trailingAnchor),
            scroll.topAnchor.constraint(equalTo: sep.bottomAnchor),
            scroll.leadingAnchor.constraint(equalTo: leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: trailingAnchor),
            scroll.bottomAnchor.constraint(equalTo: bottomAnchor),
            doc.topAnchor.constraint(equalTo: clip.topAnchor),
            doc.leadingAnchor.constraint(equalTo: clip.leadingAnchor),
            doc.trailingAnchor.constraint(equalTo: clip.trailingAnchor),
            stack.topAnchor.constraint(equalTo: doc.topAnchor, constant: 16),
            stack.leadingAnchor.constraint(equalTo: doc.leadingAnchor, constant: 16),
            stack.trailingAnchor.constraint(equalTo: doc.trailingAnchor, constant: -16),
            stack.bottomAnchor.constraint(equalTo: doc.bottomAnchor, constant: -24),
            placeholder.centerXAnchor.constraint(equalTo: centerXAnchor),
            placeholder.centerYAnchor.constraint(equalTo: centerYAnchor, constant: -20),
        ])
        rebuild(keepScroll: false)
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) { Theme.content.setFill(); bounds.fill() }

    func show(_ d: FlowDetail?, resetTab: Bool = false) {
        let same = d?.summary.id == detail?.summary.id
        detail = d
        if resetTab || !same { open = []; raw = []; folds = [:] }
        rebuild(keepScroll: same)
    }

    func selectTab(_ i: Int) { tabs.selected = i; rebuild(keepScroll: false) }

    // MARK: build

    private func addFull(_ v: NSView) {
        stack.addArrangedSubview(v)
        v.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
    }

    private func rebuild(keepScroll: Bool) {
        let y = scroll.contentView.bounds.origin.y
        stack.arrangedSubviews.forEach { stack.removeArrangedSubview($0); $0.removeFromSuperview() }
        header.subviews.forEach { $0.removeFromSuperview() }
        placeholder.isHidden = detail != nil
        scroll.isHidden = detail == nil
        sep.isHidden = detail == nil
        guard let d = detail else {
            let h = header.heightAnchor.constraint(equalToConstant: 0)
            h.priority = .required
            header.removeConstraints(header.constraints.filter { $0.firstAttribute == .height })
            h.isActive = true
            return
        }
        header.removeConstraints(header.constraints.filter { $0.firstAttribute == .height })
        buildHeader(d)
        switch tabs.selected {
        case 1: buildMessage(d, key: "req", headers: d.requestHeaders, body: d.requestBody, name: "Request")
        case 2: buildMessage(d, key: "res", headers: d.responseHeaders, body: d.responseBody, name: "Response")
        case 3: buildTiming(d)
        default: buildOverview(d)
        }
        layoutSubtreeIfNeeded()
        scroll.contentView.scroll(to: NSPoint(x: 0, y: keepScroll ? y : 0))
        scroll.reflectScrolledClipView(scroll.contentView)
    }

    private func buildHeader(_ d: FlowDetail) {
        let s = d.summary
        let method = s.isTunnel ? "CONNECT" : s.method
        var badges: [NSView] = [Badge(method, color: Theme.method(method), role: .overline)]
        if s.isPinned {
            badges.append(Badge("Pinned tunnel", color: Theme.orange))
        } else if s.status != 0 {
            badges.append(Badge(Fmt.statusLine(s.status), color: Theme.status(s.status, failed: false)))
        } else if s.state == .failed {
            badges.append(Badge("Failed", color: Theme.red))
        }
        if !s.protocol.isEmpty && !s.isPinned { badges.append(Badge(s.protocol, color: Theme.text2)) }
        let badgeRow = NSStackView.horizontal(badges, spacing: 6)

        var host = s.host
        if s.port != 0 && s.port != 80 && s.port != 443 { host += ":\(s.port)" }
        let title = label(host, .title, Theme.text)
        var rows: [NSView] = [badgeRow, title]
        if !s.isTunnel && !s.path.isEmpty {
            let p = label(s.path, .callout, Theme.text2)
            p.toolTip = s.path
            rows.append(p)
        }
        let wrap = WrapView()
        let curl = Chip("Copy as cURL", symbol: "terminal"); curl.onClick = { [weak self] in self?.onCopyCurl?() }
        let url = Chip("Copy URL", symbol: "doc.on.doc"); url.onClick = { [weak self] in self?.onCopyURL?() }
        let replay = Chip("Replay"); replay.onClick = { [weak self] in self?.onReplay?() }
        replay.isEnabled = !s.isTunnel
        let mock = Chip("Mock"); mock.isEnabled = false; mock.toolTip = "Rules are not in the macOS app yet"
        let brk = Chip("Break"); brk.isEnabled = false; brk.toolTip = "Breakpoints are not in the macOS app yet"
        for c in [curl, url, replay, mock, brk] { wrap.addSubview(c) }
        rows.append(wrap)
        rows.append(tabs)

        let hs = NSStackView.vertical(rows, spacing: 8)
        hs.setCustomSpacing(4, after: title)
        hs.setCustomSpacing(12, after: rows[rows.count - 3 + (rows.count > 4 ? 0 : 0)])
        header.addSubview(hs)
        hs.pin(to: header, insets: NSEdgeInsets(top: 16, left: 16, bottom: 14, right: 16))
        wrap.widthAnchor.constraint(equalTo: hs.widthAnchor).isActive = true
        tabs.widthAnchor.constraint(equalTo: hs.widthAnchor).isActive = true
        tabs.heightAnchor.constraint(equalToConstant: 30).isActive = true
    }

    private func buildOverview(_ d: FlowDetail) {
        let s = d.summary
        if s.isPinned {
            let banner = Surface()
            banner.fill = Theme.tint(Theme.orange, 0.15)
            banner.border = Theme.tint(Theme.orange, 0.4)
            banner.radius = Theme.Radius.card
            banner.translatesAutoresizingMaskIntoConstraints = false
            let t = label("Not inspectable", .headline, Theme.text)
            let b = label("The client rejected the proxy certificate, so traffic to this host is tunneled untouched. Install the certificate, or the app may pin its own.", .callout, Theme.text2, lines: 0)
            b.preferredMaxLayoutWidth = 280
            let lock = SymbolView("lock", Theme.orange, size: 13)
            let top = NSStackView.horizontal([lock, t], spacing: 8)
            let inner = NSStackView.vertical([top, b], spacing: 4)
            banner.addSubview(inner)
            inner.pin(to: banner, insets: NSEdgeInsets(top: 12, left: 14, bottom: 12, right: 14))
            addFull(banner)
            let cert = Chip("Certificate settings", symbol: "shield")
            cert.onClick = { [weak self] in self?.onCertSettings?() }
            stack.addArrangedSubview(cert)
            stack.setCustomSpacing(18, after: cert)
        }
        stack.addArrangedSubview(overline("Request"))
        var req: [(String, String, Bool)] = [("URL", d.url, true), ("Method", s.isTunnel ? "CONNECT" : s.method, false),
                                              ("Protocol", s.protocol.isEmpty ? "—" : s.protocol, false)]
        if !s.app.isEmpty { req.append(("Client", s.app, false)) }
        req.append(("Sent", Fmt.bytes(s.reqSize), false))
        addFull(infoCard(req))
        stack.setCustomSpacing(18, after: stack.arrangedSubviews.last!)

        if !s.isTunnel {
            stack.addArrangedSubview(overline("Response"))
            var res: [(String, String, Bool)] = [("Status", s.status == 0 ? s.statusText : Fmt.statusLine(s.status), false)]
            if !s.contentType.isEmpty { res.append(("Type", d.responseBody.contentType.isEmpty ? s.contentType : d.responseBody.contentType, false)) }
            res.append(("Received", Fmt.bytes(s.resSize), false))
            if s.durationMs > 0 { res.append(("Time", Fmt.duration(s.durationMs), false)) }
            if !s.error.isEmpty { res.append(("Error", s.error, false)) }
            addFull(infoCard(res))
            stack.setCustomSpacing(18, after: stack.arrangedSubviews.last!)
        }

        stack.addArrangedSubview(overline("Capture"))
        let f = DateFormatter(); f.dateFormat = "HH:mm:ss.SSS"
        let src = s.source == .sdk ? "In-app SDK" : s.source == .merged ? "SDK and proxy" : "Proxy"
        var cap: [(String, String, Bool)] = [("Started", f.string(from: Date(timeIntervalSince1970: Double(s.startedAtMs) / 1000)), false), ("Source", src, false)]
        var flags: [String] = []
        if s.flags & 64 != 0 { flags.append("Answered by a Map Local rule") }
        if s.flags & 256 != 0 { flags.append("Edited") }
        if s.flags & 1024 != 0 { flags.append("Replayed") }
        if s.flags & 8 != 0 || s.flags & 16 != 0 { flags.append("Body truncated") }
        if !flags.isEmpty { cap.append(("Notes", flags.joined(separator: ", "), false)) }
        addFull(infoCard(cap))
    }

    private func buildMessage(_ d: FlowDetail, key: String, headers: [Proxy_Api_V1_Header], body: Proxy_Api_V1_BodyView, name: String) {
        let hk = key + "-h"
        let dh = DisclosureHeader("\(name) headers", count: headers.count, open: open.contains(hk))
        dh.onToggle = { [weak self] in
            guard let self else { return }
            if self.open.contains(hk) { self.open.remove(hk) } else { self.open.insert(hk) }
            self.rebuild(keepScroll: true)
        }
        addFull(dh)
        if open.contains(hk) {
            if headers.isEmpty {
                stack.addArrangedSubview(label("No headers", .callout, Theme.text3))
            } else {
                addFull(headersCard(headers))
            }
        }
        stack.setCustomSpacing(18, after: stack.arrangedSubviews.last!)
        stack.addArrangedSubview(overline("\(name) body"))
        addFull(bodyView(body, key: key))
    }

    private func headersCard(_ headers: [Proxy_Api_V1_Header]) -> NSView {
        let card = Surface()
        card.fill = Theme.card; card.border = Theme.separator; card.radius = Theme.Radius.card
        card.translatesAutoresizingMaskIntoConstraints = false
        var rows: [NSView] = []
        for h in headers {
            let k = label(h.name, .mono, Theme.Syn.key)
            let v = label(h.value, .mono, Theme.text, lines: 4)
            v.lineBreakMode = .byCharWrapping
            v.isSelectable = true
            k.setContentCompressionResistancePriority(.required, for: .horizontal)
            rows.append(NSStackView.vertical([k, v], spacing: 1))
        }
        let st = NSStackView.vertical(rows, spacing: 9)
        card.addSubview(st)
        st.pin(to: card, insets: NSEdgeInsets(top: 12, left: 14, bottom: 12, right: 14))
        return card
    }

    private func buildTiming(_ d: FlowDetail) {
        stack.addArrangedSubview(overline("Timing"))
        addFull(TimingView(d.timings))
        let t = d.timings
        stack.setCustomSpacing(14, after: stack.arrangedSubviews.last!)
        addFull(infoCard([("Connection", t.reusedConnection ? "Reused" : "New connection", false)]))
    }

    // MARK: body

    private func bodyView(_ body: Proxy_Api_V1_BodyView, key: String) -> NSView {
        let card = Surface()
        card.fill = Theme.code; card.border = Theme.separator; card.radius = Theme.Radius.card
        card.translatesAutoresizingMaskIntoConstraints = false

        if body.kind == .empty && body.note.isEmpty {
            let l = label("No body", .callout, Theme.text3)
            card.addSubview(l)
            l.pin(to: card, insets: NSEdgeInsets(top: 14, left: 14, bottom: 14, right: 14))
            return card
        }
        let kindName: String, kindColor: NSColor
        switch body.kind {
        case .json: kindName = "JSON"; kindColor = Theme.accent
        case .html: kindName = "HTML"; kindColor = Theme.orange
        case .form: kindName = "FORM"; kindColor = Theme.teal
        case .image: kindName = "IMAGE"; kindColor = Theme.purple
        case .text: kindName = "TEXT"; kindColor = Theme.text2
        default: kindName = "BINARY"; kindColor = Theme.text2
        }
        var meta = Fmt.bytes(body.decodedSize > 0 ? body.decodedSize : body.wireSize)
        if !body.contentEncoding.isEmpty { meta += " · \(body.contentEncoding)" }
        let metaL = label(meta, .callout, Theme.text2)
        var bar: [NSView] = [Badge(kindName, color: kindColor, role: .caption), metaL, NSView()]
        let isJSON = body.kind == .json && !body.json.isEmpty
        if isJSON {
            let seg = Segmented(["Tree", "Raw"], selected: raw.contains(key) ? 1 : 0)
            seg.padding = 12
            seg.onChange = { [weak self] i in
                guard let self else { return }
                if i == 1 { self.raw.insert(key) } else { self.raw.remove(key) }
                self.rebuild(keepScroll: true)
            }
            bar.append(seg)
        }
        let copy = IconButton("doc.on.doc", tip: "Copy body")
        copy.onClick = { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(body.text, forType: .string) }
        bar.append(copy)
        let barStack = NSStackView.horizontal(bar, spacing: 8)
        barStack.translatesAutoresizingMaskIntoConstraints = false
        let barBox = Surface()
        barBox.fill = Theme.card
        barBox.translatesAutoresizingMaskIntoConstraints = false
        barBox.addSubview(barStack)
        barStack.pin(to: barBox, insets: NSEdgeInsets(top: 6, left: 10, bottom: 6, right: 8))
        let line = Divider()

        let content: NSView
        if body.kind == .image, let img = NSImage(data: body.image) {
            let iv = NSImageView(image: img)
            iv.imageScaling = .scaleProportionallyDown
            iv.translatesAutoresizingMaskIntoConstraints = false
            iv.heightAnchor.constraint(lessThanOrEqualToConstant: 320).isActive = true
            let box = FlippedView(); box.translatesAutoresizingMaskIntoConstraints = false
            box.addSubview(iv)
            iv.pin(to: box, insets: NSEdgeInsets(top: 12, left: 12, bottom: 12, right: 12))
            content = box
        } else if body.kind == .form && !body.form.isEmpty {
            let rows = body.form.map { ($0.name, $0.value, true) }
            let c = infoCard(rows)
            let box = FlippedView(); box.translatesAutoresizingMaskIntoConstraints = false
            box.addSubview(c); c.pin(to: box, insets: NSEdgeInsets(top: 10, left: 10, bottom: 10, right: 10))
            content = box
        } else {
            let code = CodeView()
            let (text, nums) = codeText(body, key: key, tree: isJSON && !raw.contains(key))
            code.set(text, numbers: nums)
            code.showGutter = body.kind != .binary
            code.onLink = { [weak self] link in
                guard link.hasPrefix("fold:"), let i = Int(link.dropFirst(5)), let self else { return }
                var f = self.folds[key] ?? []
                if f.contains(i) { f.remove(i) } else { f.insert(i) }
                self.folds[key] = f
                self.rebuild(keepScroll: true)
            }
            content = code
        }
        var parts: [NSView] = [barBox, line, content]
        if body.captureTruncated || body.previewTruncated || !body.note.isEmpty {
            let n = label(body.note.isEmpty ? "Truncated: the body is larger than what is stored or shown." : body.note, .callout, Theme.orange, lines: 3)
            let nb = FlippedView(); nb.translatesAutoresizingMaskIntoConstraints = false
            nb.addSubview(n); n.pin(to: nb, insets: NSEdgeInsets(top: 8, left: 12, bottom: 10, right: 12))
            parts.append(Divider()); parts.append(nb)
        }
        let v = NSStackView.vertical(parts, spacing: 0)
        v.alignment = .width
        card.addSubview(v)
        v.pin(to: card, insets: NSEdgeInsets(top: 1, left: 1, bottom: 1, right: 1))
        return card
    }

    private static let maxLines = 3000

    private func codeText(_ body: Proxy_Api_V1_BodyView, key: String, tree: Bool) -> (NSAttributedString, [Int]) {
        let out = NSMutableAttributedString()
        var nums: [Int] = []
        let base = Theme.attrs(.code, Theme.Syn.text)
        func color(_ k: Proxy_Api_V1_JsonTokenKind) -> NSColor {
            switch k {
            case .key: return Theme.Syn.key
            case .string: return Theme.Syn.string
            case .number: return Theme.Syn.number
            case .literal: return Theme.Syn.literal
            default: return Theme.Syn.punct
            }
        }
        let para = NSMutableParagraphStyle()
        para.minimumLineHeight = 20; para.maximumLineHeight = 20
        func add(_ s: String, _ c: NSColor, link: String? = nil) {
            var a = base; a[.foregroundColor] = c; a[.paragraphStyle] = para
            if let link { a[.link] = URL(string: link) }
            out.append(NSAttributedString(string: s, attributes: a))
        }
        if tree {
            let lines = body.json
            let folded = folds[key] ?? []
            var i = 0
            while i < lines.count && nums.count < Self.maxLines {
                let l = lines[i]
                if i > 0 { add("\n", Theme.Syn.text) }
                nums.append(i + 1)
                add(String(repeating: "  ", count: Int(l.depth)), Theme.Syn.text)
                let foldable = l.foldEnd >= 0
                if foldable { add(folded.contains(i) ? "›" : "⌄", Theme.text3, link: "fold:\(i)"); add(" ", Theme.Syn.text) }
                else { add("  ", Theme.Syn.text) }
                for t in l.tokens { add(t.text, color(t.kind)) }
                if foldable && folded.contains(i) {
                    let end = Int(l.foldEnd)
                    add(" … ", Theme.Syn.punct)
                    if end < lines.count { add(lines[end].tokens.map(\.text).joined().trimmingCharacters(in: .whitespaces), Theme.Syn.punct) }
                    i = end + 1
                } else {
                    i += 1
                }
            }
            if i < lines.count {
                add("\n… \(lines.count - i) more lines", Theme.text3)
                nums.append(i + 1)
            }
        } else if body.kind == .html && !body.source.isEmpty {
            var n = 0
            for (idx, l) in body.source.enumerated() where n < Self.maxLines {
                if idx > 0 { add("\n", Theme.Syn.text) }
                n += 1; nums.append(n)
                for t in l.tokens {
                    let c: NSColor
                    switch t.kind {
                    case .srcTag: c = Theme.Syn.tag
                    case .srcAttr: c = Theme.Syn.attr
                    case .srcValue: c = Theme.Syn.value
                    case .srcComment: c = Theme.Syn.comment
                    case .srcPunct: c = Theme.Syn.punct
                    case .srcEntity: c = Theme.Syn.entity
                    default: c = Theme.Syn.text
                    }
                    add(t.text, c)
                }
            }
        } else {
            let text = body.text.isEmpty ? body.note : body.text
            var n = 0
            for line in text.split(separator: "\n", omittingEmptySubsequences: false).prefix(Self.maxLines) {
                if n > 0 { add("\n", Theme.Syn.text) }
                n += 1; nums.append(n)
                add(String(line), body.kind == .binary ? Theme.text2 : Theme.Syn.text)
            }
        }
        return (out, nums)
    }
}
