import AppKit

/// Draws line numbers beside a text view, one per paragraph, aligned to the
/// first line fragment of each so wrapped lines stay tidy.
private final class Gutter: NSView {
    weak var textView: NSTextView?
    var numbers: [Int] = []
    override var isFlipped: Bool { true }

    override func draw(_ dirtyRect: NSRect) {
        guard let tv = textView, let lm = tv.layoutManager, !numbers.isEmpty else { return }
        let ns = tv.string as NSString
        let a = Theme.attrs(.monoSmall, Theme.Syn.gutter)
        var para = 0
        var loc = 0
        while loc <= ns.length && para < numbers.count {
            let glyph = lm.glyphIndexForCharacter(at: min(loc, max(ns.length - 1, 0)))
            let rect = lm.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
            let y = rect.minY + tv.textContainerOrigin.y
            if y + rect.height >= dirtyRect.minY && y <= dirtyRect.maxY {
                let s = String(numbers[para]) as NSString
                let sz = s.size(withAttributes: a)
                s.draw(at: NSPoint(x: bounds.width - 10 - sz.width, y: y + (rect.height - sz.height) / 2 + 1), withAttributes: a)
            }
            let r = ns.paragraphRange(for: NSRange(location: min(loc, ns.length), length: 0))
            if r.upperBound >= ns.length { break }
            loc = r.upperBound
            para += 1
        }
    }
}

/// A read-only, selectable code block that sizes itself to its text.
final class CodeView: NSView, NSTextViewDelegate {
    private let gutter = Gutter()
    private let text = NSTextView()
    private var heightC: NSLayoutConstraint!
    var onLink: ((String) -> Void)?
    var showGutter = true
    override var isFlipped: Bool { true }

    override init(frame: NSRect) {
        super.init(frame: frame)
        translatesAutoresizingMaskIntoConstraints = false
        text.isEditable = false
        text.isSelectable = true
        text.drawsBackground = false
        text.isRichText = true
        text.textContainerInset = NSSize(width: 4, height: 10)
        text.textContainer?.lineFragmentPadding = 6
        text.textContainer?.widthTracksTextView = false
        text.isHorizontallyResizable = false
        text.isVerticallyResizable = false
        text.linkTextAttributes = [:]
        text.delegate = self
        text.insertionPointColor = .clear
        text.selectedTextAttributes = [.backgroundColor: Theme.accentSoft]
        gutter.textView = text
        addSubview(gutter)
        addSubview(text)
        heightC = heightAnchor.constraint(equalToConstant: 40)
        heightC.isActive = true
    }
    required init?(coder: NSCoder) { fatalError() }

    func set(_ s: NSAttributedString, numbers: [Int]) {
        text.textStorage?.setAttributedString(s)
        gutter.numbers = numbers
        needsLayout = true
        gutter.needsDisplay = true
    }

    override func layout() {
        super.layout()
        let gw: CGFloat = showGutter ? 50 : 0
        gutter.isHidden = !showGutter
        gutter.frame = NSRect(x: 0, y: 0, width: gw, height: bounds.height)
        let w = max(bounds.width - gw, 50)
        text.textContainer?.containerSize = NSSize(width: w, height: .greatestFiniteMagnitude)
        text.frame = NSRect(x: gw, y: 0, width: w, height: text.frame.height)
        guard let lm = text.layoutManager, let tc = text.textContainer else { return }
        lm.ensureLayout(for: tc)
        let h = ceil(lm.usedRect(for: tc).height) + text.textContainerInset.height * 2
        text.frame = NSRect(x: gw, y: 0, width: w, height: h)
        if abs(heightC.constant - h) > 0.5 { heightC.constant = h }
        gutter.frame.size.height = h
        gutter.needsDisplay = true
    }

    func textView(_ textView: NSTextView, clickedOnLink link: Any, at charIndex: Int) -> Bool {
        if let u = link as? URL { onLink?(u.absoluteString) } else if let s = link as? String { onLink?(s) }
        return true
    }
}
