import AppKit
import WebKit
import NekoAPI

/// What a body viewer remembers between rebuilds of the view that hosts it:
/// the chosen mode, folded JSON lines, wrap and the search.
@MainActor
final class BodyViewState {
    enum Mode: Int { case pretty, raw, preview }
    var mode: Mode?
    var folds = Set<Int>()
    var wrap = true
    var query = ""
    var current = 0
}

/// A small on/off text button for the toolbar.
private final class ToggleChip: NSView {
    var isOn: Bool { didSet { needsDisplay = true } }
    var onChange: ((Bool) -> Void)?
    private let title: String
    private var hover = false
    override var isFlipped: Bool { true }

    init(_ title: String, on: Bool) {
        self.title = title
        isOn = on
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        toolTip = "Wrap long lines"
    }
    required init?(coder: NSCoder) { fatalError() }

    override var intrinsicContentSize: NSSize {
        NSSize(width: (title as NSString).size(withAttributes: Theme.attrs(.bodyMedium)).width + 20, height: 30)
    }

    override func draw(_ dirtyRect: NSRect) {
        if isOn || hover {
            let p = NSBezierPath(roundedRect: bounds, xRadius: Theme.Radius.control, yRadius: Theme.Radius.control)
            (isOn ? Theme.accentSoft : Theme.fillHover).setFill()
            p.fill()
        }
        let a = Theme.attrs(.bodyMedium, isOn ? Theme.accent : Theme.text2)
        let s = (title as NSString).size(withAttributes: a)
        (title as NSString).draw(at: NSPoint(x: bounds.midX - s.width / 2, y: bounds.midY - s.height / 2), withAttributes: a)
    }

    override func mouseUp(with event: NSEvent) {
        guard bounds.contains(convert(event.locationInWindow, from: nil)) else { return }
        isOn.toggle()
        onChange?(isOn)
    }
    override func mouseEntered(with event: NSEvent) { hover = true; needsDisplay = true }
    override func mouseExited(with event: NSEvent) { hover = false; needsDisplay = true }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }
}

/// Keeps a rendered page from going anywhere: no links, no redirects.
private final class NavBlocker: NSObject, WKNavigationDelegate {
    func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction,
                 decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        let url = navigationAction.request.url
        let local = url == nil || url?.scheme == "about" || url?.absoluteString == "about:blank"
        decisionHandler(navigationAction.navigationType == .other && local ? .allow : .cancel)
    }
}

/// One response or request body: kind badge, size, view modes, folding, copy.
/// `.card` is the Inspector's rounded card that sizes itself to the body;
/// `.pane` fills the space it is given, scrolls inside and adds Pretty/Raw/Preview,
/// wrap, search and save for the Composer.
@MainActor
final class BodyViewer: Surface {
    enum Style { case card, pane }
    typealias Mode = BodyViewState.Mode
    private enum Content { case code, form, image, web }

    static let maxLines = 3000

    let body: Neko_Api_V1_BodyView
    let state: BodyViewState
    let style: Style

    private let image: NSImage?
    private let host = FlippedView()
    private var content: Content?
    private var codeView: CodeView?
    private var clip: NSClipView?
    private var wrapConstraints: [NSLayoutConstraint] = []
    private var shown = NSAttributedString()
    private var matches: [NSRange] = []
    private var segmented: Segmented?
    private var wrapToggle: ToggleChip?
    private var searchGroup: NSStackView?
    private var searchField: Field?
    private let count = label("", .caption, Theme.text3)
    private let blocker = NavBlocker()

    init(body: Neko_Api_V1_BodyView, state: BodyViewState? = nil, style: Style = .card) {
        self.body = body
        self.state = state ?? BodyViewState()
        self.style = style
        image = body.kind == .image && !body.image.isEmpty ? NSImage(data: body.image) : nil
        super.init(frame: .zero)
        fill = Theme.code
        if style == .card { border = Theme.separator; radius = Theme.Radius.card }
        translatesAutoresizingMaskIntoConstraints = false
        host.translatesAutoresizingMaskIntoConstraints = false

        if body.kind == .empty && body.note.isEmpty {
            let l = label("No body", style == .card ? .callout : .body, Theme.text3)
            addSubview(l)
            if style == .card {
                l.pin(to: self, insets: NSEdgeInsets(top: 14, left: 14, bottom: 14, right: 14))
            } else {
                l.centerXAnchor.constraint(equalTo: centerXAnchor).isActive = true
                l.centerYAnchor.constraint(equalTo: centerYAnchor).isActive = true
            }
            return
        }

        var parts: [NSView] = [buildBar(), Divider(), host]
        if body.captureTruncated || body.previewTruncated || !body.note.isEmpty {
            let n = label(body.note.isEmpty ? "Truncated: the body is larger than what is stored or shown." : body.note, .callout, Theme.orange, lines: 3)
            let nb = FlippedView(); nb.translatesAutoresizingMaskIntoConstraints = false
            nb.addSubview(n); n.pin(to: nb, insets: NSEdgeInsets(top: 8, left: 12, bottom: 10, right: 12))
            parts.append(Divider()); parts.append(nb)
        }
        let v = NSStackView.vertical(parts, spacing: 0)
        v.alignment = .width
        v.distribution = .fill
        host.setContentHuggingPriority(.init(10), for: .vertical)
        host.setContentCompressionResistancePriority(.init(10), for: .vertical)
        addSubview(v)
        let inset: CGFloat = style == .card ? 1 : 0
        v.pin(to: self, insets: NSEdgeInsets(top: inset, left: inset, bottom: inset, right: inset))
        update()
    }
    required init?(coder: NSCoder) { fatalError() }

    // MARK: modes

    private var modes: [Mode] {
        switch style {
        case .card:
            return body.kind == .json && !body.json.isEmpty ? [.pretty, .raw] : []
        case .pane:
            switch body.kind {
            case .json: return body.json.isEmpty ? [.raw] : [.pretty, .raw]
            case .html: return [.pretty, .raw, .preview]
            case .image: return image != nil ? [.preview] : [.raw]
            case .form: return body.form.isEmpty ? [.raw] : [.pretty, .raw]
            default: return [.raw]
            }
        }
    }

    private var mode: Mode {
        let all = modes
        if let m = state.mode, all.contains(m) { return m }
        return all.first ?? .pretty
    }

    private func contentKind(_ m: Mode) -> Content {
        if image != nil && (style == .card || m == .preview) { return .image }
        if style == .pane && body.kind == .html && m == .preview { return .web }
        if !body.form.isEmpty && body.kind == .form && m != .raw { return .form }
        return .code
    }

    // MARK: bar

    private var kindName: (String, NSColor) {
        switch body.kind {
        case .json: return ("JSON", Theme.accent)
        case .html: return ("HTML", Theme.orange)
        case .form: return ("FORM", Theme.teal)
        case .image: return ("IMAGE", Theme.purple)
        case .text: return ("TEXT", Theme.text2)
        default: return ("BINARY", Theme.text2)
        }
    }

    private func buildBar() -> NSView {
        let (kn, kc) = kindName
        var meta = Fmt.bytes(body.decodedSize > 0 ? body.decodedSize : body.wireSize)
        if !body.contentEncoding.isEmpty { meta += " · \(body.contentEncoding)" }
        let metaL = label(meta, .callout, Theme.text2)
        let badge = Badge(kn, color: kc, role: .caption)

        var items: [NSView] = []
        let all = modes
        if style == .pane {
            if all.count > 1 {
                let titles = all.map { $0 == .pretty ? "Pretty" : $0 == .raw ? "Raw" : "Preview" }
                let seg = Segmented(titles, selected: all.firstIndex(of: mode) ?? 0)
                seg.padding = 12
                seg.onChange = { [weak self] i in
                    guard let self else { return }
                    self.state.mode = all[i]
                    self.update()
                }
                segmented = seg
                items.append(seg)
            }
            items += [badge, metaL, NSView()]
            let wrap = ToggleChip("Wrap", on: state.wrap)
            wrap.onChange = { [weak self] on in
                self?.state.wrap = on
                self?.applyWrap()
            }
            wrapToggle = wrap
            items.append(wrap)
            items.append(buildSearch())
        } else {
            items += [badge, metaL, NSView()]
            if all.count > 1 {
                let seg = Segmented(["Tree", "Raw"], selected: mode == .raw ? 1 : 0)
                seg.padding = 12
                seg.onChange = { [weak self] i in
                    self?.state.mode = i == 1 ? .raw : .pretty
                    self?.update()
                }
                segmented = seg
                items.append(seg)
            }
        }
        let copy = IconButton("doc.on.doc", tip: "Copy body")
        copy.isEnabled = style == .card || image == nil
        copy.onClick = { [body] in NSPasteboard.general.clearContents(); NSPasteboard.general.setString(body.text, forType: .string) }
        items.append(copy)
        if style == .pane {
            let save = IconButton("square.and.arrow.down", tip: "Save to file")
            save.isEnabled = image != nil || !body.text.isEmpty
            save.onClick = { [weak self] in self?.save() }
            items.append(save)
        }
        let barStack = NSStackView.horizontal(items, spacing: 8)
        barStack.translatesAutoresizingMaskIntoConstraints = false
        let barBox = Surface()
        barBox.fill = Theme.card
        barBox.translatesAutoresizingMaskIntoConstraints = false
        barBox.addSubview(barStack)
        barStack.pin(to: barBox, insets: NSEdgeInsets(top: 6, left: 10, bottom: 6, right: 8))
        return barBox
    }

    private func buildSearch() -> NSView {
        let field = Field(state.query, placeholder: "Search body", width: 150, mono: false)
        field.onChange = { [weak self] q in
            guard let self else { return }
            self.state.query = q
            self.state.current = 0
            self.refreshMatches(reveal: true)
        }
        field.onSubmit = { [weak self] in self?.step(1) }
        searchField = field
        count.widthAnchor.constraint(greaterThanOrEqualToConstant: 52).isActive = true
        count.alignment = .right
        let up = IconButton("chevron.up", tip: "Previous match")
        up.onClick = { [weak self] in self?.step(-1) }
        let down = IconButton("chevron.down", tip: "Next match")
        down.onClick = { [weak self] in self?.step(1) }
        let g = NSStackView.horizontal([field, count, up, down], spacing: 4)
        searchGroup = g
        return g
    }

    func focusSearch() { window?.makeFirstResponder(searchField?.input) }

    // MARK: content

    private func update() {
        let m = mode
        let c = contentKind(m)
        segmented?.selected = modes.firstIndex(of: m) ?? 0
        searchGroup?.isHidden = c != .code
        wrapToggle?.isHidden = c != .code
        if c == .code, content == .code, let cv = codeView {
            applyCode(cv)
            return
        }
        host.subviews.forEach { $0.removeFromSuperview() }
        codeView = nil; clip = nil; wrapConstraints = []; matches = []
        content = c
        switch c {
        case .code: buildCode()
        case .form: buildForm()
        case .image: buildImage()
        case .web: buildWeb()
        }
    }

    private func scroller(horizontal: Bool = false) -> (NSScrollView, FlippedView) {
        let scroll = NSScrollView()
        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = horizontal
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        let doc = FlippedView()
        doc.translatesAutoresizingMaskIntoConstraints = false
        scroll.documentView = doc
        host.addSubview(scroll)
        scroll.pin(to: host)
        NSLayoutConstraint.activate([
            doc.topAnchor.constraint(equalTo: scroll.contentView.topAnchor),
            doc.leadingAnchor.constraint(equalTo: scroll.contentView.leadingAnchor),
        ])
        clip = scroll.contentView
        return (scroll, doc)
    }

    private func buildCode() {
        let cv = CodeView()
        cv.showGutter = body.kind != .binary
        cv.onLink = { [weak self] link in
            guard link.hasPrefix("fold:"), let i = Int(link.dropFirst(5)), let self else { return }
            if self.state.folds.contains(i) { self.state.folds.remove(i) } else { self.state.folds.insert(i) }
            self.update()
        }
        codeView = cv
        if style == .card {
            host.addSubview(cv)
            cv.pin(to: host)
        } else {
            let (_, doc) = scroller(horizontal: true)
            doc.addSubview(cv)
            cv.pin(to: doc)
        }
        applyCode(cv)
    }

    private func applyCode(_ cv: CodeView) {
        let (text, nums) = displayText()
        cv.set(text, numbers: nums)
        shown = text
        applyWrap()
        refreshMatches(reveal: false)
    }

    private func applyWrap() {
        guard style == .pane, let cv = codeView, let clip else { return }
        NSLayoutConstraint.deactivate(wrapConstraints)
        let fit = cv.widthAnchor.constraint(equalTo: clip.widthAnchor)
        if state.wrap {
            wrapConstraints = [fit]
        } else {
            fit.priority = .init(250)
            wrapConstraints = [cv.widthAnchor.constraint(greaterThanOrEqualToConstant: noWrapWidth()),
                               cv.widthAnchor.constraint(greaterThanOrEqualTo: clip.widthAnchor), fit]
        }
        NSLayoutConstraint.activate(wrapConstraints)
        cv.needsLayout = true
    }

    /// The width of the longest displayed line, gutter and margins included.
    private func noWrapWidth() -> CGFloat {
        let font = Theme.font(.code)
        var longest = Substring()
        for line in shown.string.split(separator: "\n", omittingEmptySubsequences: true) where line.utf16.count > longest.utf16.count {
            longest = line
        }
        let w = (String(longest) as NSString).size(withAttributes: [.font: font]).width
        return ceil(w) + 50 + 28
    }

    private func buildForm() {
        let c = infoCard(body.form.map { ($0.name, $0.value, true) })
        let box = FlippedView(); box.translatesAutoresizingMaskIntoConstraints = false
        box.addSubview(c); c.pin(to: box, insets: NSEdgeInsets(top: 10, left: 10, bottom: 10, right: 10))
        if style == .card {
            host.addSubview(box)
            box.pin(to: host)
        } else {
            let (_, doc) = scroller()
            doc.addSubview(box)
            box.pin(to: doc)
            box.widthAnchor.constraint(equalTo: clip!.widthAnchor).isActive = true
        }
    }

    private func buildImage() {
        guard let img = image else { return }
        let iv = NSImageView(image: img)
        iv.imageScaling = .scaleProportionallyDown
        iv.translatesAutoresizingMaskIntoConstraints = false
        let box = FlippedView(); box.translatesAutoresizingMaskIntoConstraints = false
        box.addSubview(iv)
        if style == .card {
            iv.heightAnchor.constraint(lessThanOrEqualToConstant: 320).isActive = true
            iv.pin(to: box, insets: NSEdgeInsets(top: 12, left: 12, bottom: 12, right: 12))
            host.addSubview(box)
            box.pin(to: host)
        } else {
            let (_, doc) = scroller()
            doc.addSubview(box)
            box.pin(to: doc)
            let size = img.size
            let natural = iv.widthAnchor.constraint(equalToConstant: max(size.width, 1))
            natural.priority = .init(750)
            NSLayoutConstraint.activate([
                box.widthAnchor.constraint(equalTo: clip!.widthAnchor),
                iv.topAnchor.constraint(equalTo: box.topAnchor, constant: 12),
                iv.leadingAnchor.constraint(equalTo: box.leadingAnchor, constant: 12),
                iv.bottomAnchor.constraint(equalTo: box.bottomAnchor, constant: -12),
                iv.widthAnchor.constraint(lessThanOrEqualTo: box.widthAnchor, constant: -24),
                iv.heightAnchor.constraint(equalTo: iv.widthAnchor, multiplier: size.height / max(size.width, 1)),
                natural,
            ])
        }
    }

    private func buildWeb() {
        let cfg = WKWebViewConfiguration()
        let prefs = WKWebpagePreferences()
        prefs.allowsContentJavaScript = false
        cfg.defaultWebpagePreferences = prefs
        let web = WKWebView(frame: .zero, configuration: cfg)
        web.navigationDelegate = blocker
        web.translatesAutoresizingMaskIntoConstraints = false
        host.addSubview(web)
        web.pin(to: host)
        // Nothing the page names is fetched: no scripts, no remote images, fonts or frames.
        let csp = "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; font-src data:; style-src 'unsafe-inline'\">"
        web.loadHTMLString(csp + body.text, baseURL: nil)
    }

    // MARK: text

    private func displayText() -> (NSAttributedString, [Int]) {
        let m = mode
        let out = NSMutableAttributedString()
        var nums: [Int] = []
        let base = Theme.attrs(.code, Theme.Syn.text)
        func color(_ k: Neko_Api_V1_JsonTokenKind) -> NSColor {
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
        let tree = m == .pretty && body.kind == .json && !body.json.isEmpty
        if tree {
            let lines = body.json
            let folded = state.folds
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
        } else if m == .pretty && body.kind == .html && !body.source.isEmpty {
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
            if body.source.count > n {
                add("\n… \(body.source.count - n) more lines", Theme.text3)
                nums.append(n + 1)
            }
        } else {
            let text = body.text.isEmpty ? body.note : body.text
            let (lines, more) = BodyText.lines(text, maxLines: Self.maxLines)
            var n = 0
            for line in lines {
                if n > 0 { add("\n", Theme.Syn.text) }
                n += 1; nums.append(n)
                add(String(line), body.kind == .binary ? Theme.text2 : Theme.Syn.text)
            }
            if more > 0 {
                add("\n… \(more) more lines", Theme.text3)
                nums.append(n + 1)
            }
        }
        return (out, nums)
    }

    // MARK: search

    private var textView: NSTextView? { codeView?.subviews.compactMap { $0 as? NSTextView }.first }

    private func refreshMatches(reveal: Bool) {
        guard style == .pane else { return }
        matches = BodySearch.ranges(in: shown.string, query: state.query)
        if state.current >= matches.count { state.current = 0 }
        count.set(BodySearch.summary(current: state.current, count: matches.count, query: state.query), .caption,
                  matches.isEmpty && !state.query.isEmpty ? Theme.orange : Theme.text3)
        paintMatches()
        if reveal { revealCurrent() }
    }

    private func paintMatches() {
        guard let lm = textView?.layoutManager else { return }
        lm.removeTemporaryAttribute(.backgroundColor, forCharacterRange: NSRange(location: 0, length: shown.length))
        let all = Theme.tint(Theme.yellow, 0.3), cur = Theme.tint(Theme.orange, 0.55)
        for (i, r) in matches.enumerated() where NSMaxRange(r) <= shown.length {
            lm.addTemporaryAttribute(.backgroundColor, value: i == state.current ? cur : all, forCharacterRange: r)
        }
    }

    private func revealCurrent() {
        guard let tv = textView, matches.indices.contains(state.current) else { return }
        layoutSubtreeIfNeeded()
        tv.scrollRangeToVisible(matches[state.current])
    }

    private func step(_ delta: Int) {
        guard !matches.isEmpty else { return }
        state.current = BodySearch.step(state.current, by: delta, count: matches.count)
        count.set(BodySearch.summary(current: state.current, count: matches.count, query: state.query), .caption, Theme.text3)
        paintMatches()
        revealCurrent()
    }

    // MARK: save

    private func save() {
        let data = image != nil ? body.image : Data(body.text.utf8)
        let panel = NSSavePanel()
        panel.nameFieldStringValue = bodyFileName(kind: kindName.0.lowercased(), contentType: body.contentType)
        let write: (NSApplication.ModalResponse) -> Void = { [weak self] r in
            guard r == .OK, let url = panel.url else { return }
            do { try data.write(to: url) } catch { tell(self?.window, "Save", error.localizedDescription) }
        }
        if let w = window { panel.beginSheetModal(for: w, completionHandler: write) } else { write(panel.runModal()) }
    }
}
