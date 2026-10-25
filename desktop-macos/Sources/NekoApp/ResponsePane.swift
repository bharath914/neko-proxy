import AppKit
import NekoAPI

// MARK: - Section tabs

/// Text tabs with an underline on the selected one and an optional count badge.
final class SectionTabs: NSView {
    struct Item {
        var title: String
        var badge: String? = nil
        var tint: NSColor? = nil
    }

    var items: [Item] { didSet { invalidateIntrinsicContentSize(); needsDisplay = true } }
    var selected: Int { didSet { needsDisplay = true } }
    var onChange: ((Int) -> Void)?
    private var hover: Int?
    private let pad: CGFloat = 12
    override var isFlipped: Bool { true }

    init(_ items: [Item], selected: Int) {
        self.items = items
        self.selected = selected
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
    }
    required init?(coder: NSCoder) { fatalError() }

    private func titleAttrs(_ i: Int) -> [NSAttributedString.Key: Any] {
        Theme.attrs(.bodyMedium, i == selected ? Theme.text : i == hover ? Theme.text : Theme.text2)
    }

    private func badgeAttrs(_ it: Item) -> [NSAttributedString.Key: Any] {
        Theme.attrs(.caption, it.tint ?? Theme.text2)
    }

    /// The horizontal extent of each tab.
    private func spans() -> [(CGFloat, CGFloat)] {
        var x: CGFloat = 0
        var out: [(CGFloat, CGFloat)] = []
        for (i, it) in items.enumerated() {
            var w = (it.title as NSString).size(withAttributes: Theme.attrs(.bodyMedium)).width
            if let b = it.badge { w += 6 + (b as NSString).size(withAttributes: badgeAttrs(it)).width + 12 }
            let width = w + pad * 2
            out.append((x, width))
            x += width
            _ = i
        }
        return out
    }

    override var intrinsicContentSize: NSSize {
        NSSize(width: spans().last.map { $0.0 + $0.1 } ?? 0, height: 40)
    }

    override func draw(_ dirtyRect: NSRect) {
        for (i, (x, w)) in spans().enumerated() {
            let it = items[i]
            let a = titleAttrs(i)
            let s = it.title as NSString
            let z = s.size(withAttributes: a)
            var tx = x + pad
            s.draw(at: NSPoint(x: tx, y: bounds.midY - z.height / 2 - 1), withAttributes: a)
            tx += z.width
            if let b = it.badge {
                let ba = badgeAttrs(it)
                let bz = (b as NSString).size(withAttributes: ba)
                let r = NSRect(x: tx + 6, y: bounds.midY - 9 - 1, width: bz.width + 12, height: 18)
                (it.tint.map { Theme.tint($0, 0.18) } ?? Theme.fill).setFill()
                NSBezierPath(roundedRect: r, xRadius: 5, yRadius: 5).fill()
                (b as NSString).draw(at: NSPoint(x: r.midX - bz.width / 2, y: r.midY - bz.height / 2), withAttributes: ba)
            }
            if i == selected {
                Theme.accent.setFill()
                NSBezierPath(roundedRect: NSRect(x: x + pad - 2, y: bounds.maxY - 2, width: w - pad * 2 + 4, height: 2),
                             xRadius: 1, yRadius: 1).fill()
            }
        }
    }

    private func index(_ e: NSEvent) -> Int? {
        let p = convert(e.locationInWindow, from: nil)
        return spans().firstIndex { p.x >= $0.0 && p.x < $0.0 + $0.1 }
    }

    override func mouseDown(with event: NSEvent) {
        if let i = index(event), i != selected { selected = i; onChange?(i) }
    }
    override func mouseMoved(with event: NSEvent) {
        let i = index(event)
        if i != hover { hover = i; needsDisplay = true }
    }
    override func mouseExited(with event: NSEvent) { hover = nil; needsDisplay = true }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }
}

// MARK: - Progress line

/// A thin indeterminate bar that runs along the top while a request is in flight.
private final class ProgressLine: NSView {
    private let bar = CALayer()
    override var isFlipped: Bool { true }

    init() {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        wantsLayer = true
        layer?.masksToBounds = true
        layer?.addSublayer(bar)
        heightAnchor.constraint(equalToConstant: 2).isActive = true
    }
    required init?(coder: NSCoder) { fatalError() }

    private func colors() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = Theme.accentSoft.cgColor
            bar.backgroundColor = Theme.accent.cgColor
        }
    }

    override func viewDidChangeEffectiveAppearance() { super.viewDidChangeEffectiveAppearance(); colors() }
    override func viewDidMoveToWindow() { super.viewDidMoveToWindow(); colors(); needsLayout = true }

    override func layout() {
        super.layout()
        colors()
        let w = bounds.width
        guard w > 0 else { return }
        bar.frame = NSRect(x: 0, y: 0, width: w * 0.3, height: bounds.height)
        bar.removeAllAnimations()
        let a = CABasicAnimation(keyPath: "position.x")
        a.fromValue = -w * 0.15
        a.toValue = w * 1.15
        a.duration = 1.3
        a.repeatCount = .infinity
        a.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        bar.add(a, forKey: "run")
    }
}

// MARK: - Response pane

/// The response half: status, time and size, and the Body, Cookies, Headers,
/// Tests and Timing sections. Everything shown is read from `model.tab`.
@MainActor
final class ResponsePane: NSView {
    let model: ComposerModel
    var onInspect: ((UInt64) -> Void)?
    override var isFlipped: Bool { true }

    private let header = FlippedView()
    private let rule = Divider()
    private let host = FlippedView()
    private var tabs: SectionTabs?
    private var bodyStates: [String: BodyViewState] = [:]
    private var showingNormal = false

    static let sections = ["Body", "Cookies", "Headers", "Tests", "Timing"]

    init(model: ComposerModel) {
        self.model = model
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        for v in [header, rule, host] { v.translatesAutoresizingMaskIntoConstraints = false; addSubview(v) }
        NSLayoutConstraint.activate([
            header.topAnchor.constraint(equalTo: topAnchor),
            header.leadingAnchor.constraint(equalTo: leadingAnchor),
            header.trailingAnchor.constraint(equalTo: trailingAnchor),
            header.heightAnchor.constraint(equalToConstant: 40),
            rule.topAnchor.constraint(equalTo: header.bottomAnchor),
            rule.leadingAnchor.constraint(equalTo: leadingAnchor),
            rule.trailingAnchor.constraint(equalTo: trailingAnchor),
            host.topAnchor.constraint(equalTo: rule.bottomAnchor),
            host.leadingAnchor.constraint(equalTo: leadingAnchor),
            host.trailingAnchor.constraint(equalTo: trailingAnchor),
            host.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
        reload()
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) { Theme.content.setFill(); bounds.fill() }

    // MARK: state

    private enum Phase { case empty, sending, failed, normal }

    private var phase: Phase {
        let t = model.tab
        if t.sending { return .sending }
        if t.sendError != nil { return .failed }
        if t.sent == nil && t.response == nil { return .empty }
        return .normal
    }

    /// The active tab changed, or its sending flag or response did.
    func reload() {
        let t = model.tab
        t.responseSection = max(0, min(t.responseSection, Self.sections.count - 1))
        let p = phase
        showingNormal = p == .normal
        header.isHidden = p != .normal
        rule.isHidden = p != .normal
        host.subviews.forEach { $0.removeFromSuperview() }
        header.subviews.forEach { $0.removeFromSuperview() }
        tabs = nil
        switch p {
        case .empty: buildEmpty()
        case .sending: buildSending()
        case .failed: buildFailed()
        case .normal:
            buildHeader()
            buildSection()
        }
    }

    // MARK: pieces

    private func centered(_ views: [NSView], spacing: CGFloat = 6) {
        let s = NSStackView.vertical(views, spacing: spacing)
        s.alignment = .centerX
        host.addSubview(s)
        NSLayoutConstraint.activate([
            s.centerXAnchor.constraint(equalTo: host.centerXAnchor),
            s.centerYAnchor.constraint(equalTo: host.centerYAnchor, constant: -10),
            s.leadingAnchor.constraint(greaterThanOrEqualTo: host.leadingAnchor, constant: 24),
        ])
    }

    private func buildEmpty() {
        let t = label("Send a request to see the response", .headline, Theme.text2)
        let key = Badge("⌘↵", color: Theme.text2, role: .mono)
        let hint = NSStackView.horizontal([label("Press", .callout, Theme.text3), key, label("to send", .callout, Theme.text3)], spacing: 6)
        centered([t, hint], spacing: 8)
    }

    private func buildSending() {
        let line = ProgressLine()
        host.addSubview(line)
        NSLayoutConstraint.activate([
            line.topAnchor.constraint(equalTo: host.topAnchor),
            line.leadingAnchor.constraint(equalTo: host.leadingAnchor),
            line.trailingAnchor.constraint(equalTo: host.trailingAnchor),
        ])
        centered([label("Sending…", .headline, Theme.text2)])
    }

    private func buildFailed() {
        let t = model.tab
        let message = t.sendError ?? ""
        let card = Surface()
        card.fill = Theme.tint(Theme.red, 0.08)
        card.border = Theme.red
        card.radius = Theme.Radius.card
        card.translatesAutoresizingMaskIntoConstraints = false
        let head = NSStackView.horizontal([SymbolView("exclamationmark.triangle.fill", Theme.red, size: 13), label("The request failed", .headline, Theme.text)], spacing: 8)
        let msg = label(message, .mono, Theme.text, lines: 0)
        msg.isSelectable = true
        msg.lineBreakMode = .byCharWrapping
        msg.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        var rows: [NSView] = [head, msg]
        if let id = t.sent?.flowID, id != 0 {
            let c = Chip("Show in Inspector", symbol: "arrow.up.right.square", style: .text)
            c.onClick = { [weak self] in self?.onInspect?(id) }
            rows.append(c)
        }
        let st = NSStackView.vertical(rows, spacing: 10)
        card.addSubview(st)
        st.pin(to: card, insets: NSEdgeInsets(top: 14, left: 16, bottom: 14, right: 16))
        msg.widthAnchor.constraint(equalTo: st.widthAnchor).isActive = true
        host.addSubview(card)
        let w = card.widthAnchor.constraint(equalToConstant: 560)
        w.priority = .defaultHigh
        NSLayoutConstraint.activate([
            card.topAnchor.constraint(equalTo: host.topAnchor, constant: 28),
            card.centerXAnchor.constraint(equalTo: host.centerXAnchor),
            card.leadingAnchor.constraint(greaterThanOrEqualTo: host.leadingAnchor, constant: 24),
            card.trailingAnchor.constraint(lessThanOrEqualTo: host.trailingAnchor, constant: -24),
            w,
        ])
    }

    private var testSummary: TestSummary {
        let tests = model.tab.sent?.tests ?? []
        return TestSummary(passed: tests.filter(\.passed).count, total: tests.count)
    }

    private func buildHeader() {
        let t = model.tab
        let sent = t.sent
        let cookies = sent?.cookies.count ?? 0
        let headers = t.response?.responseHeaders.count ?? 0
        let ts = testSummary
        let items: [SectionTabs.Item] = [
            .init(title: "Body"),
            .init(title: "Cookies", badge: cookies > 0 ? "\(cookies)" : nil),
            .init(title: "Headers", badge: headers > 0 ? "\(headers)" : nil),
            .init(title: "Tests", badge: ts.total > 0 ? ts.badge : nil, tint: ts.total == 0 ? nil : ts.allPassed ? Theme.green : Theme.red),
            .init(title: "Timing"),
        ]
        let tabs = SectionTabs(items, selected: t.responseSection)
        tabs.onChange = { [weak self] i in
            self?.model.tab.responseSection = i
            self?.buildSection()
        }
        self.tabs = tabs

        var right: [NSView] = []
        let status = sent?.status ?? t.response?.summary.status ?? 0
        if status != 0 {
            right.append(Badge(Fmt.statusLine(status), color: Theme.status(status, failed: false), role: .bodyMedium))
        }
        var parts: [String] = []
        let ms = sent?.durationMs ?? t.response?.summary.durationMs ?? 0
        if ms > 0 { parts.append(Fmt.duration(ms)) }
        let bytes = (sent?.size ?? 0) > 0 ? sent!.size : (t.response.map { $0.responseBody.decodedSize > 0 ? $0.responseBody.decodedSize : $0.summary.resSize } ?? 0)
        if bytes > 0 { parts.append(Fmt.bytes(bytes)) }
        if !parts.isEmpty { right.append(label(parts.joined(separator: "  ·  "), .callout, Theme.text2)) }
        if let id = sent?.flowID, id != 0 {
            let c = Chip("Show in Inspector", style: .text)
            c.toolTip = "Open this request in the Inspector"
            c.onClick = { [weak self] in self?.onInspect?(id) }
            right.append(c)
        }
        let r = NSStackView.horizontal(right, spacing: 10)
        header.addSubview(tabs)
        header.addSubview(r)
        NSLayoutConstraint.activate([
            tabs.leadingAnchor.constraint(equalTo: header.leadingAnchor, constant: 4),
            tabs.topAnchor.constraint(equalTo: header.topAnchor),
            tabs.bottomAnchor.constraint(equalTo: header.bottomAnchor),
            r.trailingAnchor.constraint(equalTo: header.trailingAnchor, constant: -10),
            r.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            r.leadingAnchor.constraint(greaterThanOrEqualTo: tabs.trailingAnchor, constant: 12),
        ])
        tabs.setContentCompressionResistancePriority(.required, for: .horizontal)
    }

    private func buildSection() {
        host.subviews.forEach { $0.removeFromSuperview() }
        let t = model.tab
        switch t.responseSection {
        case 0: buildBody()
        case 1: buildCookies()
        case 2: buildHeaders()
        case 3: buildTests()
        default: buildTiming()
        }
    }

    private func fill(_ v: NSView) {
        host.addSubview(v)
        v.pin(to: host)
    }

    /// A scrolling column of full-width views.
    private func column(_ views: [NSView], maxWidth: CGFloat? = nil) -> NSView {
        let scroll = NSScrollView()
        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        let doc = FlippedView()
        doc.translatesAutoresizingMaskIntoConstraints = false
        scroll.documentView = doc
        let stack = NSStackView.vertical(views, spacing: 12)
        doc.addSubview(stack)
        let clip = scroll.contentView
        var cs = [
            doc.topAnchor.constraint(equalTo: clip.topAnchor),
            doc.leadingAnchor.constraint(equalTo: clip.leadingAnchor),
            doc.trailingAnchor.constraint(equalTo: clip.trailingAnchor),
            stack.topAnchor.constraint(equalTo: doc.topAnchor, constant: 14),
            stack.leadingAnchor.constraint(equalTo: doc.leadingAnchor, constant: 14),
            stack.trailingAnchor.constraint(equalTo: doc.trailingAnchor, constant: -14),
            stack.bottomAnchor.constraint(equalTo: doc.bottomAnchor, constant: -18),
        ]
        if let maxWidth { cs.append(stack.widthAnchor.constraint(lessThanOrEqualToConstant: maxWidth)) }
        NSLayoutConstraint.activate(cs)
        for v in views { v.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true }
        return scroll
    }

    private func note(_ s: String) {
        host.subviews.forEach { $0.removeFromSuperview() }
        let l = label(s, .body, Theme.text3, lines: 0)
        l.alignment = .center
        host.addSubview(l)
        NSLayoutConstraint.activate([
            l.centerXAnchor.constraint(equalTo: host.centerXAnchor),
            l.centerYAnchor.constraint(equalTo: host.centerYAnchor, constant: -8),
            l.leadingAnchor.constraint(greaterThanOrEqualTo: host.leadingAnchor, constant: 24),
        ])
    }

    // MARK: sections

    private func buildBody() {
        let t = model.tab
        guard let d = t.response else {
            note((t.sent?.flowID ?? 0) != 0 ? "Loading the response…" : "There is no response body.")
            return
        }
        let key = "\(t.uid)-\(d.summary.id)"
        let state = bodyStates[key] ?? BodyViewState()
        bodyStates[key] = state
        fill(BodyViewer(body: d.responseBody, state: state, style: .pane))
    }

    private func buildCookies() {
        let cookies = model.tab.sent?.cookies ?? []
        if cookies.isEmpty { note("No cookies were set by this response."); return }
        let widths: [CGFloat] = [140, 240, 170, 80, 150, 0]
        func row(_ cells: [NSView]) -> NSView {
            for (i, c) in cells.enumerated() {
                if widths[i] > 0 { c.widthAnchor.constraint(equalToConstant: widths[i]).isActive = true }
            }
            return NSStackView.horizontal(cells, spacing: 12, alignment: .top)
        }
        var rows: [NSView] = []
        rows.append(row(["Name", "Value", "Domain", "Path", "Expires", "Flags"].map { overline($0) }))
        for c in cookies {
            func mono(_ s: String, _ color: NSColor = Theme.text) -> NSTextField {
                let l = label(s.isEmpty ? "–" : s, .mono, s.isEmpty ? Theme.text3 : color, lines: 3)
                l.lineBreakMode = .byCharWrapping
                l.isSelectable = true
                return l
            }
            var flags: [NSView] = []
            if c.httpOnly { flags.append(Badge("HttpOnly", color: Theme.teal)) }
            if c.secure { flags.append(Badge("Secure", color: Theme.green)) }
            if !c.sameSite.isEmpty { flags.append(Badge("SameSite=\(c.sameSite)", color: Theme.text2)) }
            let fs = NSStackView.horizontal(flags + [NSView()], spacing: 4)
            let expires = !c.expires.isEmpty ? c.expires : !c.maxAge.isEmpty ? "Max-Age \(c.maxAge)" : "Session"
            rows.append(row([mono(c.name, Theme.Syn.key), mono(c.value), mono(c.domain), mono(c.path), mono(expires, Theme.text2), fs]))
        }
        let card = groupCard(rows, padding: NSEdgeInsets(top: 10, left: 14, bottom: 10, right: 14))
        fill(column([card]))
    }

    private func buildHeaders() {
        guard let d = model.tab.response else {
            note((model.tab.sent?.flowID ?? 0) != 0 ? "Loading the response…" : "There are no response headers.")
            return
        }
        let headers = d.responseHeaders
        if headers.isEmpty { note("The response had no headers."); return }
        let copy = Chip("Copy", symbol: "doc.on.doc")
        copy.onClick = {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(headers.map { "\($0.name): \($0.value)" }.joined(separator: "\n"), forType: .string)
        }
        let top = NSStackView.horizontal([overline("\(headers.count) headers"), NSView(), copy], spacing: 8)
        var rows: [NSView] = []
        for h in headers {
            let k = label(h.name, .mono, Theme.Syn.key, lines: 0)
            k.lineBreakMode = .byCharWrapping
            k.isSelectable = true
            k.widthAnchor.constraint(equalToConstant: 200).isActive = true
            let v = label(h.value, .mono, Theme.text, lines: 0)
            v.lineBreakMode = .byCharWrapping
            v.isSelectable = true
            v.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
            rows.append(NSStackView.horizontal([k, v], spacing: 14, alignment: .top))
        }
        fill(column([top, groupCard(rows, padding: NSEdgeInsets(top: 8, left: 14, bottom: 8, right: 14))]))
    }

    private func buildTests() {
        let tests = model.tab.sent?.tests ?? []
        if tests.isEmpty { note("No tests ran. Add tests in the request's Tests tab."); return }
        let sum = testSummary
        let summary = NSStackView.horizontal([
            SymbolView(sum.allPassed ? "checkmark.circle.fill" : "xmark.circle.fill", sum.allPassed ? Theme.green : Theme.red, size: 15),
            label(sum.line, .headline, sum.allPassed ? Theme.green : Theme.red),
        ], spacing: 8)
        var rows: [NSView] = []
        for r in tests {
            let icon = SymbolView(r.passed ? "checkmark.circle.fill" : "xmark.circle.fill", r.passed ? Theme.green : Theme.red, size: 14)
            let name = label(r.name, .body, Theme.text, lines: 0)
            name.isSelectable = true
            var lines: [NSView] = [name]
            if !r.passed && !r.message.isEmpty {
                let m = label(r.message, .callout, Theme.text3, lines: 0)
                m.isSelectable = true
                lines.append(m)
            }
            let col = NSStackView.vertical(lines, spacing: 2)
            col.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
            rows.append(NSStackView.horizontal([icon, col, NSView()], spacing: 10, alignment: .top))
        }
        fill(column([summary, groupCard(rows, padding: NSEdgeInsets(top: 10, left: 14, bottom: 10, right: 14))]))
    }

    private func buildTiming() {
        guard let d = model.tab.response else {
            note((model.tab.sent?.flowID ?? 0) != 0 ? "Loading the response…" : "There are no timings for this request.")
            return
        }
        fill(column([TimingView(d.timings, pending: d.summary.state == .pending)], maxWidth: 720))
    }
}
