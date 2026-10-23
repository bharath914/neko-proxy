import AppKit

/// The green on/off switch used in Rules and Devices.
final class Switch: NSView {
    var isOn: Bool { didSet { needsDisplay = true } }
    var onToggle: ((Bool) -> Void)?
    var isEnabled = true { didSet { needsDisplay = true } }
    override var isFlipped: Bool { true }

    init(on: Bool) {
        isOn = on
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        widthAnchor.constraint(equalToConstant: 42).isActive = true
        heightAnchor.constraint(equalToConstant: 24).isActive = true
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        let track = NSBezierPath(roundedRect: bounds, xRadius: 12, yRadius: 12)
        (isOn ? NSColor(srgbRed: 0.204, green: 0.78, blue: 0.349, alpha: 1) : Theme.fillPress).setFill()
        track.fill()
        let d: CGFloat = 20
        let x = isOn ? bounds.width - d - 2 : 2
        NSGraphicsContext.saveGraphicsState()
        let sh = NSShadow()
        sh.shadowColor = NSColor.black.withAlphaComponent(0.25); sh.shadowBlurRadius = 2; sh.shadowOffset = NSSize(width: 0, height: -1)
        sh.set()
        NSColor.white.setFill()
        NSBezierPath(ovalIn: NSRect(x: x, y: 2, width: d, height: d)).fill()
        NSGraphicsContext.restoreGraphicsState()
        if !isEnabled { Theme.content.withAlphaComponent(0.5).setFill(); track.fill() }
    }

    override func mouseUp(with event: NSEvent) {
        guard isEnabled, bounds.contains(convert(event.locationInWindow, from: nil)) else { return }
        isOn.toggle()
        onToggle?(isOn)
    }
}

/// A single-line text input in the code-field style.
final class Field: Surface, NSTextFieldDelegate {
    let input = NSTextField()
    var onChange: ((String) -> Void)?
    var text: String {
        get { input.stringValue }
        set { input.stringValue = newValue }
    }

    init(_ text: String = "", placeholder: String = "", width: CGFloat? = nil, mono: Bool = true) {
        super.init(frame: .zero)
        fill = Theme.code
        border = Theme.separator
        radius = 6
        translatesAutoresizingMaskIntoConstraints = false
        input.isBordered = false
        input.drawsBackground = false
        input.focusRingType = .none
        input.font = Theme.font(mono ? .mono : .body)
        input.textColor = Theme.text
        input.stringValue = text
        input.placeholderAttributedString = NSAttributedString(string: placeholder, attributes: Theme.attrs(mono ? .mono : .body, Theme.text3))
        input.cell?.usesSingleLineMode = true
        input.cell?.isScrollable = true
        input.delegate = self
        input.translatesAutoresizingMaskIntoConstraints = false
        addSubview(input)
        NSLayoutConstraint.activate([
            input.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            input.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            input.centerYAnchor.constraint(equalTo: centerYAnchor),
            heightAnchor.constraint(equalToConstant: 28),
        ])
        if let width { widthAnchor.constraint(equalToConstant: width).isActive = true }
    }
    required init?(coder: NSCoder) { fatalError() }

    func controlTextDidChange(_ obj: Notification) { onChange?(input.stringValue) }
}

/// A multi-line text input in the code-field style.
final class Area: Surface, NSTextViewDelegate {
    private let scroll = NSScrollView()
    let textView = NSTextView()
    var onChange: ((String) -> Void)?
    var text: String {
        get { textView.string }
        set { textView.string = newValue }
    }
    var isEditable: Bool {
        get { textView.isEditable }
        set { textView.isEditable = newValue; alphaValue = newValue ? 1 : 0.6 }
    }

    init(_ text: String = "", rows: Int, placeholder: String = "") {
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
        textView.textColor = Theme.text
        textView.insertionPointColor = Theme.accent
        textView.isRichText = false
        textView.allowsUndo = true
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        textView.isAutomaticSpellingCorrectionEnabled = false
        textView.textContainerInset = NSSize(width: 4, height: 6)
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.textContainer?.widthTracksTextView = true
        textView.string = text
        textView.delegate = self
        scroll.documentView = textView
        addSubview(scroll)
        scroll.pin(to: self, insets: NSEdgeInsets(top: 1, left: 1, bottom: 1, right: 1))
        heightAnchor.constraint(equalToConstant: CGFloat(rows) * 17 + 14).isActive = true
        if !placeholder.isEmpty { self.placeholder = placeholder }
    }
    required init?(coder: NSCoder) { fatalError() }

    private var placeholder = ""
    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        if textView.string.isEmpty && !placeholder.isEmpty {
            (placeholder as NSString).draw(in: NSRect(x: 9, y: 7, width: bounds.width - 18, height: bounds.height - 10),
                                           withAttributes: Theme.attrs(.mono, Theme.text3))
        }
    }

    func textDidChange(_ notification: Notification) { needsDisplay = true; onChange?(textView.string) }
}

/// A small caption above a form field.
func caption(_ s: String) -> NSTextField { label(s, .caption, Theme.text2) }

/// A labelled field, stacked.
func labelled(_ title: String, _ field: NSView, fill: Bool = true) -> NSStackView {
    let s = NSStackView.vertical([caption(title), field], spacing: 5)
    s.alignment = .leading
    if fill {
        field.leadingAnchor.constraint(equalTo: s.leadingAnchor).isActive = true
        field.trailingAnchor.constraint(equalTo: s.trailingAnchor).isActive = true
    }
    return s
}
