import AppKit
import NekoAPI

// Small widgets shared by the Body and Tests editors.

// MARK: - Popup

/// A pill that shows the current choice and lists the others when clicked.
final class BodyPopup: NSView {
    var titles: [String] { didSet { invalidateIntrinsicContentSize(); needsDisplay = true } }
    var selected: Int { didSet { needsDisplay = true } }
    var onPick: ((Int) -> Void)?
    var isEnabled = true { didSet { needsDisplay = true } }
    private var hover = false
    private let fixedWidth: CGFloat?
    override var isFlipped: Bool { true }

    init(_ titles: [String], selected: Int, width: CGFloat? = nil) {
        self.titles = titles
        self.selected = selected
        fixedWidth = width
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        heightAnchor.constraint(equalToConstant: 28).isActive = true
        if let width { widthAnchor.constraint(equalToConstant: width).isActive = true }
    }
    required init?(coder: NSCoder) { fatalError() }

    override var intrinsicContentSize: NSSize {
        if let fixedWidth { return NSSize(width: fixedWidth, height: 28) }
        let w = titles.map { ($0 as NSString).size(withAttributes: Theme.attrs(.bodyMedium)).width }.max() ?? 40
        return NSSize(width: w + 36, height: 28)
    }

    override func draw(_ dirtyRect: NSRect) {
        let p = NSBezierPath(roundedRect: bounds, xRadius: Theme.Radius.control, yRadius: Theme.Radius.control)
        (hover && isEnabled ? Theme.fillHover : Theme.fill).setFill()
        p.fill()
        let a = Theme.attrs(.bodyMedium, isEnabled ? Theme.text : Theme.text3)
        let s = (selected < titles.count ? titles[selected] : "") as NSString
        let size = s.size(withAttributes: a)
        let pr = NSMutableParagraphStyle(); pr.lineBreakMode = .byTruncatingTail
        var b = a; b[.paragraphStyle] = pr
        s.draw(in: NSRect(x: 10, y: bounds.midY - size.height / 2, width: max(bounds.width - 32, 10), height: size.height), withAttributes: b)
        drawSymbol("chevron.up.chevron.down", in: NSRect(x: bounds.maxX - 22, y: 0, width: 14, height: bounds.height), color: Theme.text3, size: 8.5, weight: .bold)
    }

    override func mouseDown(with event: NSEvent) {
        guard isEnabled else { return }
        let m = ActionMenu()
        for (i, t) in titles.enumerated() {
            m.add(t, checked: i == selected) { [weak self] in
                guard let self else { return }
                self.selected = i
                self.onPick?(i)
            }
        }
        m.popUp(positioning: nil, at: NSPoint(x: 0, y: bounds.height + 4), in: self)
    }
    override func mouseEntered(with event: NSEvent) { hover = true; needsDisplay = true }
    override func mouseExited(with event: NSEvent) { hover = false; needsDisplay = true }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }
}

// MARK: - Radio row

/// Postman's body type row: round radios with labels in a line.
final class BodyRadioRow: NSView {
    var titles: [String]
    var selected: Int { didSet { needsDisplay = true } }
    var onPick: ((Int) -> Void)?
    private var hover: Int?
    override var isFlipped: Bool { true }
    private static let gap: CGFloat = 20, dot: CGFloat = 14, pad: CGFloat = 7

    init(_ titles: [String], selected: Int) {
        self.titles = titles
        self.selected = selected
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        heightAnchor.constraint(equalToConstant: 28).isActive = true
    }
    required init?(coder: NSCoder) { fatalError() }

    private func widths() -> [CGFloat] {
        titles.map { ($0 as NSString).size(withAttributes: Theme.attrs(.body)).width + Self.dot + Self.pad }
    }

    override var intrinsicContentSize: NSSize {
        NSSize(width: widths().reduce(0, +) + Self.gap * CGFloat(max(titles.count - 1, 0)), height: 28)
    }

    private func frames() -> [NSRect] {
        var x: CGFloat = 0
        return widths().map { w in
            defer { x += w + Self.gap }
            return NSRect(x: x, y: 0, width: w, height: bounds.height)
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        for (i, f) in frames().enumerated() {
            let on = i == selected
            let d = NSRect(x: f.minX, y: bounds.midY - Self.dot / 2, width: Self.dot, height: Self.dot)
            if on {
                Theme.accent.setFill()
                NSBezierPath(ovalIn: d).fill()
                Theme.onAccent.setFill()
                NSBezierPath(ovalIn: d.insetBy(dx: 4.5, dy: 4.5)).fill()
            } else {
                (hover == i ? Theme.text2 : Theme.text3).setStroke()
                let p = NSBezierPath(ovalIn: d.insetBy(dx: 0.5, dy: 0.5))
                p.lineWidth = 1
                p.stroke()
            }
            let a = Theme.attrs(on ? .bodyMedium : .body, on || hover == i ? Theme.text : Theme.text2)
            let s = titles[i] as NSString
            let size = s.size(withAttributes: a)
            s.draw(at: NSPoint(x: d.maxX + Self.pad, y: bounds.midY - size.height / 2), withAttributes: a)
        }
    }

    private func index(_ e: NSEvent) -> Int? {
        let p = convert(e.locationInWindow, from: nil)
        let gap = Self.gap / 2
        return frames().firstIndex { $0.insetBy(dx: -gap, dy: 0).contains(p) }
    }

    override func mouseDown(with event: NSEvent) {
        if let i = index(event), i != selected { selected = i; onPick?(i) }
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

// MARK: - Scrolling host

/// Puts a view that has its own height in a vertical scroll view, as wide as the scroll view.
@MainActor
func bodyScroll(_ content: NSView, insets: NSEdgeInsets = NSEdgeInsets(top: 2, left: 0, bottom: 14, right: 0)) -> NSScrollView {
    let scroll = NSScrollView()
    scroll.translatesAutoresizingMaskIntoConstraints = false
    scroll.drawsBackground = false
    scroll.hasVerticalScroller = true
    scroll.autohidesScrollers = true
    scroll.scrollerStyle = .overlay
    let doc = FlippedView()
    doc.translatesAutoresizingMaskIntoConstraints = false
    scroll.documentView = doc
    doc.addSubview(content)
    content.translatesAutoresizingMaskIntoConstraints = false
    let clip = scroll.contentView
    NSLayoutConstraint.activate([
        doc.topAnchor.constraint(equalTo: clip.topAnchor),
        doc.leadingAnchor.constraint(equalTo: clip.leadingAnchor),
        doc.trailingAnchor.constraint(equalTo: clip.trailingAnchor),
        content.topAnchor.constraint(equalTo: doc.topAnchor, constant: insets.top),
        content.bottomAnchor.constraint(equalTo: doc.bottomAnchor, constant: -insets.bottom),
        content.leadingAnchor.constraint(equalTo: doc.leadingAnchor, constant: insets.left),
        content.trailingAnchor.constraint(equalTo: doc.trailingAnchor, constant: -insets.right),
    ])
    return scroll
}

// MARK: - Code editor

/// A multi-line monospaced editor in the code-field style that fills the height
/// it is given, with light syntax tinting.
@MainActor
final class BodyCodeEditor: Surface, NSTextViewDelegate {
    private let scroll = NSScrollView()
    let textView = NSTextView()
    var onChange: ((String) -> Void)?
    var language: BodySyntax.Language { didSet { if oldValue != language { highlight() } } }
    var placeholder: String { didSet { needsDisplay = true } }
    private var pending: DispatchWorkItem?

    var text: String { textView.string }

    init(_ text: String, language: BodySyntax.Language, placeholder: String = "", minHeight: CGFloat = 120) {
        self.language = language
        self.placeholder = placeholder
        super.init(frame: .zero)
        fill = Theme.code
        border = Theme.separator
        radius = 6
        translatesAutoresizingMaskIntoConstraints = false
        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        textView.drawsBackground = false
        textView.font = Theme.font(.mono)
        textView.textColor = Theme.Syn.text
        textView.insertionPointColor = Theme.accent
        textView.isRichText = false
        textView.allowsUndo = true
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        textView.isAutomaticSpellingCorrectionEnabled = false
        textView.isAutomaticLinkDetectionEnabled = false
        textView.isAutomaticDataDetectionEnabled = false
        textView.smartInsertDeleteEnabled = false
        textView.isContinuousSpellCheckingEnabled = false
        textView.textContainerInset = NSSize(width: 6, height: 8)
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        textView.textContainer?.widthTracksTextView = true
        textView.typingAttributes = baseAttributes
        textView.string = text
        textView.delegate = self
        scroll.documentView = textView
        addSubview(scroll)
        scroll.pin(to: self, insets: NSEdgeInsets(top: 1, left: 1, bottom: 1, right: 1))
        let h = heightAnchor.constraint(greaterThanOrEqualToConstant: minHeight)
        h.priority = .defaultHigh
        h.isActive = true
        highlight()
    }
    required init?(coder: NSCoder) { fatalError() }

    private var baseAttributes: [NSAttributedString.Key: Any] {
        [.font: Theme.font(.mono), .foregroundColor: Theme.Syn.text]
    }

    private static func color(_ k: BodySyntax.Kind) -> NSColor {
        switch k {
        case .key: return Theme.Syn.key
        case .string: return Theme.Syn.string
        case .number: return Theme.Syn.number
        case .literal: return Theme.Syn.literal
        case .punct: return Theme.Syn.punct
        case .tag: return Theme.Syn.tag
        case .attr: return Theme.Syn.attr
        case .value: return Theme.Syn.value
        case .comment: return Theme.Syn.comment
        case .entity: return Theme.Syn.entity
        case .variable: return Theme.orange
        }
    }

    func highlight() {
        guard let ts = textView.textStorage, !textView.hasMarkedText() else { return }
        let s = textView.string
        let full = NSRange(location: 0, length: (s as NSString).length)
        ts.beginEditing()
        ts.setAttributes(baseAttributes, range: full)
        if full.length <= BodySyntax.limit {
            for t in BodySyntax.tokens(s, language) where t.location + t.length <= full.length {
                ts.addAttribute(.foregroundColor, value: Self.color(t.kind), range: NSRange(location: t.location, length: t.length))
            }
        }
        ts.endEditing()
    }

    /// Replaces the whole text as one undoable edit.
    func replaceAll(_ s: String) {
        let full = NSRange(location: 0, length: (textView.string as NSString).length)
        guard textView.shouldChangeText(in: full, replacementString: s) else { return }
        textView.textStorage?.replaceCharacters(in: full, with: s)
        textView.didChangeText()
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        if textView.string.isEmpty && !placeholder.isEmpty {
            (placeholder as NSString).draw(in: NSRect(x: 12, y: 9, width: max(bounds.width - 24, 10), height: max(bounds.height - 14, 10)),
                                           withAttributes: Theme.attrs(.mono, Theme.text3))
        }
    }

    override func layout() {
        super.layout()
        // Clicking below the last line still lands in the text.
        let h = scroll.contentSize.height
        textView.minSize = NSSize(width: 0, height: h)
        if textView.frame.height < h { textView.setFrameSize(NSSize(width: textView.frame.width, height: h)) }
    }

    func textDidChange(_ notification: Notification) {
        needsDisplay = true
        pending?.cancel()
        if (textView.string as NSString).length > 20_000 {
            let w = DispatchWorkItem { [weak self] in self?.highlight() }
            pending = w
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.15, execute: w)
        } else {
            highlight()
        }
        onChange?(textView.string)
    }
}
