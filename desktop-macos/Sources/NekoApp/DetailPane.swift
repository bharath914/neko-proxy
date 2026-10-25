import AppKit
import NekoAPI

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

/// One waterfall: the total, a summary bar, and a bar per phase placed where it
/// starts. Phases the source did not measure (negative) show as dashes.
final class TimingView: Surface {
    private let rows: [(String, Double, NSColor)]
    private let total: Double
    private let note: String
    private let heading: String?
    private var headH: CGFloat { heading == nil ? 0 : 22 }

    init(_ t: Neko_Api_V1_Timings, heading: String? = nil, pending: Bool = false) {
        self.heading = heading
        rows = [("DNS", t.dnsMs, Theme.purple), ("Connect", t.connectMs, Theme.orange), ("TLS", t.tlsMs, Theme.pink),
                ("Send", t.sendMs, Theme.teal), ("Wait", t.waitMs, Theme.accent), ("Receive", t.receiveMs, Theme.green)]
        total = max(t.totalMs, rows.reduce(0) { $0 + max($1.1, 0) }, 0.001)
        note = pending ? "in progress" : t.reusedConnection ? "connection reused" : "new connection"
        super.init(frame: .zero)
        fill = Theme.card; border = Theme.separator; radius = Theme.Radius.card
        translatesAutoresizingMaskIntoConstraints = false
        heightAnchor.constraint(equalToConstant: CGFloat(rows.count) * 30 + 80 + headH).isActive = true
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        var y: CGFloat = 14
        if let heading {
            (heading.uppercased() as NSString).draw(at: NSPoint(x: 14, y: y), withAttributes: Theme.attrs(.overline, Theme.text3))
            y += headH
        }
        // Total and note.
        let big = Fmt.duration(total > 0.001 ? total : -1) as NSString
        let bigA = Theme.attrs(.title, Theme.text)
        big.draw(at: NSPoint(x: 14, y: y), withAttributes: bigA)
        let bw = big.size(withAttributes: bigA).width
        (note as NSString).draw(at: NSPoint(x: 14 + bw + 8, y: y + 8), withAttributes: Theme.attrs(.callout, Theme.text2))
        y += 40
        // Summary bar.
        let barW = bounds.width - 28
        Theme.fill.setFill()
        NSBezierPath(roundedRect: NSRect(x: 14, y: y, width: barW, height: 10), xRadius: 5, yRadius: 5).fill()
        var x: CGFloat = 14
        for (_, ms, color) in rows where ms > 0 {
            let w = barW * CGFloat(ms / total)
            if w > 0.5 {
                color.setFill()
                NSBezierPath(roundedRect: NSRect(x: x + 0.5, y: y, width: max(w - 1, 1), height: 10), xRadius: 3, yRadius: 3).fill()
                x += w
            }
        }
        y += 24
        // Waterfall.
        let labelW: CGFloat = 70, valueW: CGFloat = 62
        let trackX = 14 + labelW, trackW = bounds.width - trackX - valueW - 14
        var offset = 0.0
        for (name, ms, color) in rows {
            let la = Theme.attrs(.body, Theme.text2)
            let sz = (name as NSString).size(withAttributes: la)
            (name as NSString).draw(at: NSPoint(x: 14, y: y + 15 - sz.height / 2), withAttributes: la)
            Theme.fill.setFill()
            NSBezierPath(roundedRect: NSRect(x: trackX, y: y + 10, width: trackW, height: 10), xRadius: 5, yRadius: 5).fill()
            if ms > 0 {
                let bx = trackX + trackW * CGFloat(offset / total)
                let w = max(trackW * CGFloat(ms / total), 4)
                color.setFill()
                NSBezierPath(roundedRect: NSRect(x: bx, y: y + 10, width: w, height: 10), xRadius: 5, yRadius: 5).fill()
                offset += ms
            }
            let va = Theme.attrs(.mono, ms > 0 ? Theme.text : Theme.text3)
            let vs = (ms > 0 ? Fmt.duration(ms) : "—") as NSString
            let vz = vs.size(withAttributes: va)
            vs.draw(at: NSPoint(x: bounds.width - 14 - vz.width, y: y + 15 - vz.height / 2), withAttributes: va)
            y += 30
        }
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
    var onCompose: (() -> Void)?
    var onCopyCurl: (() -> Void)?
    var onCopyURL: (() -> Void)?
    var onReplay: (() -> Void)?
    var onCertSettings: (() -> Void)?
    var onMock: (() -> Void)?
    var onBreak: (() -> Void)?

    private let header = FlippedView()
    enum Kind { case request, response }
    let kind: Kind
    private let tabs: Segmented
    private let scroll = NSScrollView()
    private let doc = FlippedView()
    private let stack = NSStackView()
    private let placeholder = NSStackView()
    private var headerStack: NSStackView?
    private var headerBottom: NSLayoutConstraint?
    private var open: Set<String> = []
    private var bodyStates: [String: BodyViewState] = [:]
    private var sep = Divider()
    override var isFlipped: Bool { true }

    init(kind: Kind) {
        self.kind = kind
        tabs = Segmented(kind == .request ? ["Request", "Overview"] : ["Response", "Timing"])
        super.init(frame: .zero)
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
        if resetTab || !same { open = []; bodyStates = [:] }
        rebuild(keepScroll: same)
    }

    /// Takes the old four-tab numbering (0 overview, 1 request, 2 response, 3 timing); ignored if it belongs to the other pane.
    func selectTab(_ i: Int) {
        let mine: [Int: Int] = kind == .request ? [1: 0, 0: 1] : [2: 0, 3: 1]
        guard let t = mine[i] else { return }
        tabs.selected = t
        rebuild(keepScroll: false)
    }

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
        switch (kind, tabs.selected) {
        case (.request, 0): buildMessage(d, key: "req", headers: d.requestHeaders, body: d.requestBody, name: "Request")
        case (.request, _): buildOverview(d)
        case (.response, 0):
            if d.summary.isTunnel { stack.addArrangedSubview(label("A tunnel has no response to show.", .callout, Theme.text3)) }
            else { buildMessage(d, key: "res", headers: d.responseHeaders, body: d.responseBody, name: "Response") }
        case (.response, _): buildTiming(d)
        }
        layoutSubtreeIfNeeded()
        scroll.contentView.scroll(to: NSPoint(x: 0, y: keepScroll ? y : 0))
        scroll.reflectScrolledClipView(scroll.contentView)
    }

    private func buildHeader(_ d: FlowDetail) {
        let s = d.summary
        let method = s.isTunnel ? "CONNECT" : s.method
        var status: NSView?
        if s.isPinned { status = Badge("Pinned tunnel", color: Theme.orange) }
        else if s.status != 0 { status = Badge(Fmt.statusLine(s.status), color: Theme.status(s.status, failed: false)) }
        else if s.state == .failed { status = Badge("Failed", color: Theme.red) }
        if kind == .response {
            var badges: [NSView] = status.map { [$0] } ?? []
            var parts: [String] = []
            if s.resSize > 0 { parts.append(Fmt.bytes(s.resSize)) }
            if s.durationMs > 0 && !s.isTunnel { parts.append(Fmt.duration(s.durationMs)) }
            if !parts.isEmpty { badges.append(label(parts.joined(separator: "  ·  "), .callout, Theme.text2)) }
            let hs = NSStackView.vertical([NSStackView.horizontal(badges + [NSView()], spacing: 8), tabs], spacing: 10)
            header.addSubview(hs)
            hs.pin(to: header, insets: NSEdgeInsets(top: 12, left: 16, bottom: 10, right: 16))
            tabs.widthAnchor.constraint(equalTo: hs.widthAnchor).isActive = true
            tabs.heightAnchor.constraint(equalToConstant: 30).isActive = true
            return
        }
        var badges: [NSView] = [Badge(method, color: Theme.method(method), role: .overline)]
        if s.isPinned, let status { badges.append(status) }
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
        let mock = Chip("Mock"); mock.isEnabled = !s.isTunnel; mock.onClick = { [weak self] in self?.onMock?() }
        mock.toolTip = "Answer requests like this with a fixed response"
        let brk = Chip("Break"); brk.isEnabled = !s.isTunnel; brk.onClick = { [weak self] in self?.onBreak?() }
        brk.toolTip = "Hold requests like this until you resume them"
        let compose = Chip("Edit in Composer"); compose.isEnabled = !s.isTunnel
        compose.onClick = { [weak self] in self?.onCompose?() }
        for c in [curl, url, compose, replay, mock, brk] { wrap.addSubview(c) }
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

    private func buildMessage(_ d: FlowDetail, key: String, headers: [Neko_Api_V1_Header], body: Neko_Api_V1_BodyView, name: String) {
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
        let state = bodyStates[key] ?? BodyViewState()
        bodyStates[key] = state
        addFull(BodyViewer(body: body, state: state))
    }

    private func headersCard(_ headers: [Neko_Api_V1_Header]) -> NSView {
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
        let pending = d.summary.state == .pending
        stack.addArrangedSubview(overline("Timing"))
        // A merged flow shows both: the client stack's own timings and the proxy's.
        let app = d.hasSdk && d.sdk.hasTimings ? d.sdk.timings : d.timings
        if d.hasWire {
            addFull(TimingView(app, heading: "As the app saw it", pending: pending))
            stack.setCustomSpacing(10, after: stack.arrangedSubviews.last!)
            addFull(TimingView(d.wire.timings, heading: "On the wire (proxy)", pending: pending))
        } else {
            addFull(TimingView(app, pending: pending))
        }
        if app.reusedConnection {
            stack.setCustomSpacing(10, after: stack.arrangedSubviews.last!)
            addFull(label("DNS, connect and TLS are zero because this request reused an open connection.", .callout, Theme.text3, lines: 0))
        }
    }
}
