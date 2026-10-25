import AppKit
import NekoAPI

// MARK: - Checkbox

/// A small rounded checkbox drawn in the accent color.
final class CheckBox: NSView {
    var isOn: Bool { didSet { needsDisplay = true } }
    var onToggle: ((Bool) -> Void)?
    override var isFlipped: Bool { true }

    init(on: Bool) {
        isOn = on
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        widthAnchor.constraint(equalToConstant: 16).isActive = true
        heightAnchor.constraint(equalToConstant: 16).isActive = true
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        let r = bounds.insetBy(dx: 1, dy: 1)
        let p = NSBezierPath(roundedRect: r, xRadius: 4, yRadius: 4)
        if isOn {
            Theme.accent.setFill(); p.fill()
            drawSymbol("checkmark", in: r, color: Theme.onAccent, size: 8.5, weight: .heavy)
        } else {
            Theme.code.setFill(); p.fill()
            Theme.text3.setStroke(); p.lineWidth = 1; p.stroke()
        }
    }
    override func mouseUp(with event: NSEvent) {
        guard bounds.contains(convert(event.locationInWindow, from: nil)) else { return }
        isOn.toggle()
        onToggle?(isOn)
    }
}

// MARK: - Small controls

/// A compact text button for table cells and bars: hover fill, optional symbol and chevron.
final class MiniButton: NSView {
    var title: String { didSet { invalidateIntrinsicContentSize(); needsDisplay = true } }
    var symbolName: String? { didSet { invalidateIntrinsicContentSize(); needsDisplay = true } }
    var chevron = false { didSet { invalidateIntrinsicContentSize(); needsDisplay = true } }
    var color: NSColor = Theme.text3 { didSet { needsDisplay = true } }
    var onClick: (() -> Void)?
    private var hover = false
    override var isFlipped: Bool { true }

    init(_ title: String, symbol: String? = nil, chevron: Bool = false, color: NSColor = Theme.text3, tip: String? = nil) {
        self.title = title
        self.symbolName = symbol
        self.chevron = chevron
        self.color = color
        super.init(frame: .zero)
        toolTip = tip
        translatesAutoresizingMaskIntoConstraints = false
        heightAnchor.constraint(equalToConstant: 22).isActive = true
        setContentHuggingPriority(.required, for: .horizontal)
        setContentCompressionResistancePriority(.required, for: .horizontal)
    }
    required init?(coder: NSCoder) { fatalError() }

    override var intrinsicContentSize: NSSize {
        let tw = title.isEmpty ? 0 : (title as NSString).size(withAttributes: Theme.attrs(.caption)).width
        var w = tw + 12
        if symbolName != nil { w += title.isEmpty ? 10 : 16 }
        if chevron { w += 12 }
        return NSSize(width: max(w, 24), height: 22)
    }

    override func draw(_ dirtyRect: NSRect) {
        if hover {
            Theme.fillHover.setFill()
            NSBezierPath(roundedRect: bounds, xRadius: 5, yRadius: 5).fill()
        }
        let a = Theme.attrs(.caption, color)
        let sz = (title as NSString).size(withAttributes: a)
        var x: CGFloat = 6
        if title.isEmpty, symbolName != nil { x = bounds.midX - 7 }
        if let s = symbolName {
            drawSymbol(s, in: NSRect(x: x, y: 0, width: 14, height: bounds.height), color: color, size: 11.5, weight: .medium)
            x += title.isEmpty ? 14 : 16
        }
        if !title.isEmpty { (title as NSString).draw(at: NSPoint(x: x, y: bounds.midY - sz.height / 2), withAttributes: a) }
        if chevron {
            drawSymbol("chevron.down", in: NSRect(x: bounds.maxX - 16, y: 0, width: 12, height: bounds.height), color: color, size: 7.5, weight: .bold)
        }
    }

    override func mouseUp(with event: NSEvent) {
        if bounds.contains(convert(event.locationInWindow, from: nil)) { onClick?() }
    }
    override func mouseEntered(with event: NSEvent) { hover = true; needsDisplay = true }
    override func mouseExited(with event: NSEvent) { hover = false; needsDisplay = true }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }

    /// Pops a menu under the button.
    func pop(_ menu: NSMenu) {
        menu.popUp(positioning: nil, at: NSPoint(x: 0, y: bounds.height + 3), in: self)
    }
}

/// A text field for table cells: no chrome, reports focus.
final class CellField: NSTextField {
    var onFocus: ((Bool) -> Void)?
    override func becomeFirstResponder() -> Bool {
        let ok = super.becomeFirstResponder()
        if ok { onFocus?(true) }
        return ok
    }
    override func textDidEndEditing(_ notification: Notification) {
        super.textDidEndEditing(notification)
        onFocus?(false)
    }
}

final class CellSecureField: NSSecureTextField {
    var onFocus: ((Bool) -> Void)?
    override func becomeFirstResponder() -> Bool {
        let ok = super.becomeFirstResponder()
        if ok { onFocus?(true) }
        return ok
    }
    override func textDidEndEditing(_ notification: Notification) {
        super.textDidEndEditing(notification)
        onFocus?(false)
    }
}

/// One editable cell. Can show its text masked, and offers inline suggestions.
@MainActor
final class CellInput: NSView, NSTextFieldDelegate {
    private let plain = CellField()
    private let secure = CellSecureField()
    private let placeholder: String
    private let mono: Bool
    private var focused = false { didSet { needsDisplay = true } }
    private var lastCount = 0
    var onChange: ((String) -> Void)?
    /// Completions for the text typed so far; the first is shown selected after the caret.
    var suggestions: ((String) -> [String])?
    var secret = false {
        didSet {
            guard secret != oldValue else { return }
            let t = text
            plain.stringValue = t; secure.stringValue = t
            plain.isHidden = secret; secure.isHidden = !secret
        }
    }
    var isEditable = true {
        didSet { plain.isEditable = isEditable; secure.isEditable = isEditable; plain.isSelectable = true }
    }
    var textColor: NSColor = Theme.text { didSet { plain.textColor = textColor; secure.textColor = textColor } }
    override var isFlipped: Bool { true }
    override var intrinsicContentSize: NSSize { NSSize(width: NSView.noIntrinsicMetric, height: NSView.noIntrinsicMetric) }

    var text: String {
        get { secret ? secure.stringValue : plain.stringValue }
        set { plain.stringValue = newValue; secure.stringValue = newValue }
    }

    init(_ text: String = "", placeholder: String, mono: Bool = true) {
        self.placeholder = placeholder
        self.mono = mono
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        for f in [plain, secure] as [NSTextField] {
            f.isBordered = false
            f.drawsBackground = false
            f.focusRingType = .none
            f.font = Theme.font(mono ? .mono : .body)
            f.textColor = Theme.text
            f.placeholderAttributedString = NSAttributedString(string: placeholder, attributes: Theme.attrs(mono ? .mono : .body, Theme.text3))
            f.cell?.usesSingleLineMode = true
            f.cell?.isScrollable = true
            f.cell?.lineBreakMode = .byTruncatingTail
            f.delegate = self
            f.translatesAutoresizingMaskIntoConstraints = false
            addSubview(f)
            NSLayoutConstraint.activate([
                f.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 9),
                f.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -6),
                f.centerYAnchor.constraint(equalTo: centerYAnchor),
            ])
        }
        secure.isHidden = true
        plain.onFocus = { [weak self] in self?.focused = $0 }
        secure.onFocus = { [weak self] in self?.focused = $0 }
        self.text = text
        lastCount = text.count
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        guard focused else { return }
        let p = NSBezierPath(roundedRect: bounds.insetBy(dx: 1.5, dy: 1.5), xRadius: 5, yRadius: 5)
        Theme.accent.withAlphaComponent(0.7).setStroke()
        p.lineWidth = 1.2
        p.stroke()
    }

    func focus() { window?.makeFirstResponder(secret ? secure : plain) }

    func controlTextDidChange(_ obj: Notification) {
        guard let f = obj.object as? NSTextField else { return }
        let t = f.stringValue
        var out = t
        let inserted = t.count > lastCount
        if inserted, let suggest = suggestions, let ed = f.currentEditor() as? NSTextView,
           ed.selectedRange.location == (t as NSString).length,
           let first = suggest(t).first, first.count > t.count {
            ed.string = first
            ed.setSelectedRange(NSRange(location: (t as NSString).length, length: (first as NSString).length - (t as NSString).length))
            out = first
        }
        lastCount = out.count
        onChange?(out)
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy sel: Selector) -> Bool {
        // Return commits the cell and moves on, like Tab, instead of beating the field editor.
        if sel == #selector(NSResponder.insertNewline(_:)) {
            window?.makeFirstResponder(nil)
            return true
        }
        return false
    }
}

// MARK: - Key/value table

/// A row that draws hairlines: a top line and a line after each of its column views.
class GridRow: NSView {
    var dividers: [NSView] = []
    var topLine = true
    private(set) var hover = false { didSet { if hover != oldValue { hoverChanged() } } }
    override var isFlipped: Bool { true }

    func hoverChanged() {}

    override func draw(_ dirtyRect: NSRect) {
        Theme.separator.setFill()
        if topLine { NSRect(x: 0, y: 0, width: bounds.width, height: 1).fill() }
        for d in dividers where !d.isHidden {
            let x = convert(d.bounds, from: d).maxX
            NSRect(x: x - 0.5, y: 0, width: 1, height: bounds.height).fill()
        }
    }
    override func layout() { super.layout(); needsDisplay = true }
    override func mouseEntered(with event: NSEvent) { hover = true }
    override func mouseExited(with event: NSEvent) { hover = false }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }
}

struct KVStyle {
    var description = false
    var files = false
    var secrets = false
    /// Path variables: the name is fixed, nothing is added or removed, no enable box.
    var fixed = false
}

/// One row of a key/value table: enable box, key, value, optional description and a remove button.
@MainActor
final class KVRow: GridRow {
    let kvStyle: KVStyle
    let check: CheckBox
    let key: CellInput
    let value: CellInput
    let desc: CellInput?
    let trash = IconButton("trash", tip: "Remove row")
    private let fileIcon = NSImageView()
    private let fileLabel = label("", .mono, Theme.text)
    private let pick = MiniButton("Select file…", color: Theme.accent)
    private let fileBox: NSStackView
    private let kindButton = MiniButton("Text", chevron: true, tip: "Text or file")
    private let eye = MiniButton("", symbol: "eye", tip: "Mask the value")
    private(set) var isFile: Bool
    private(set) var isSecret: Bool
    var isBlank: Bool { didSet { restyle() } }
    /// Anything in the row changed; the table reads it back with `fill`.
    var onEdit: (() -> Void)?
    var onDelete: (() -> Void)?

    init(_ kv: KeyValue?, keyHint: String, valueHint: String, style: KVStyle) {
        kvStyle = style
        check = CheckBox(on: !(kv?.disabled ?? false))
        key = CellInput(kv?.name ?? "", placeholder: keyHint)
        value = CellInput(kv?.value ?? "", placeholder: valueHint)
        desc = style.description ? CellInput(kv?.description_p ?? "", placeholder: "Description", mono: false) : nil
        isBlank = kv == nil
        isFile = style.files && (kv?.file ?? false)
        isSecret = style.secrets && (kv?.secret ?? false)
        fileBox = NSStackView.horizontal([fileIcon, fileLabel, pick], spacing: 6)
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        heightAnchor.constraint(equalToConstant: 30).isActive = true

        fileIcon.image = symbol("doc", size: 11, weight: .medium)
        fileIcon.contentTintColor = Theme.text3
        fileIcon.setContentHuggingPriority(.required, for: .horizontal)
        fileLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        fileLabel.lineBreakMode = .byTruncatingMiddle

        // lead: enable box
        let lead = FlippedView(); lead.translatesAutoresizingMaskIntoConstraints = false
        lead.widthAnchor.constraint(equalToConstant: style.fixed ? 0 : 34).isActive = true
        lead.addSubview(check)
        check.centerXAnchor.constraint(equalTo: lead.centerXAnchor).isActive = true
        check.centerYAnchor.constraint(equalTo: lead.centerYAnchor).isActive = true
        lead.isHidden = style.fixed

        // value cell: input or file chooser, then the small controls
        let valueWrap = FlippedView(); valueWrap.translatesAutoresizingMaskIntoConstraints = false
        let accessories = NSStackView.horizontal([kindButton, eye], spacing: 2)
        accessories.setHuggingPriority(.required, for: .horizontal)
        kindButton.isHidden = !style.files
        eye.isHidden = !style.secrets
        for v in [value, fileBox, accessories] as [NSView] { valueWrap.addSubview(v) }
        NSLayoutConstraint.activate([
            accessories.trailingAnchor.constraint(equalTo: valueWrap.trailingAnchor, constant: -4),
            accessories.centerYAnchor.constraint(equalTo: valueWrap.centerYAnchor),
            value.leadingAnchor.constraint(equalTo: valueWrap.leadingAnchor),
            value.topAnchor.constraint(equalTo: valueWrap.topAnchor),
            value.bottomAnchor.constraint(equalTo: valueWrap.bottomAnchor),
            value.trailingAnchor.constraint(equalTo: accessories.leadingAnchor),
            fileBox.leadingAnchor.constraint(equalTo: valueWrap.leadingAnchor, constant: 9),
            fileBox.centerYAnchor.constraint(equalTo: valueWrap.centerYAnchor),
            fileBox.trailingAnchor.constraint(lessThanOrEqualTo: accessories.leadingAnchor, constant: -4),
        ])
        fileBox.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        let tail = FlippedView(); tail.translatesAutoresizingMaskIntoConstraints = false
        tail.widthAnchor.constraint(equalToConstant: style.fixed ? 8 : 30).isActive = true
        tail.addSubview(trash)
        trash.centerXAnchor.constraint(equalTo: tail.centerXAnchor).isActive = true
        trash.centerYAnchor.constraint(equalTo: tail.centerYAnchor).isActive = true
        trash.isHidden = style.fixed

        var cols: [NSView] = [lead, key, valueWrap]
        if let desc { cols.append(desc) }
        cols.append(tail)
        let stack = NSStackView.horizontal(cols, spacing: 0)
        stack.distribution = .fill
        addSubview(stack)
        stack.pin(to: self)
        key.widthAnchor.constraint(equalTo: valueWrap.widthAnchor, multiplier: style.description ? 0.7 : 0.62).isActive = true
        if let desc { desc.widthAnchor.constraint(equalTo: valueWrap.widthAnchor, multiplier: 0.9).isActive = true }
        dividers = style.fixed ? [key, valueWrap] : [lead, key, valueWrap]
        if desc == nil { dividers = style.fixed ? [key] : [lead, key] }
        if style.fixed { key.isEditable = false }

        key.onChange = { [weak self] _ in self?.onEdit?() }
        value.onChange = { [weak self] _ in self?.onEdit?() }
        desc?.onChange = { [weak self] _ in self?.onEdit?() }
        check.onToggle = { [weak self] _ in self?.restyle(); self?.onEdit?() }
        trash.onClick = { [weak self] in self?.onDelete?() }
        pick.onClick = { [weak self] in self?.chooseFile() }
        eye.onClick = { [weak self] in
            guard let self else { return }
            self.isSecret.toggle()
            self.restyle(); self.onEdit?()
        }
        kindButton.onClick = { [weak self] in self?.popKind() }
        if isFile { fileLabel.set(ComposerEdit.fileName(value.text), .mono, Theme.text) }
        restyle()
    }
    required init?(coder: NSCoder) { fatalError() }

    private func popKind() {
        let m = ActionMenu()
        m.add("Text", checked: !isFile) { [weak self] in self?.setFile(false) }
        m.add("File", checked: isFile) { [weak self] in self?.setFile(true) }
        kindButton.pop(m)
    }

    private func setFile(_ on: Bool) {
        guard on != isFile else { return }
        isFile = on
        value.text = ""
        fileLabel.set("No file chosen", .mono, Theme.text3)
        if on { isSecret = false }
        restyle(); onEdit?()
        if on { chooseFile() }
    }

    private func chooseFile() {
        let p = NSOpenPanel()
        p.canChooseFiles = true
        p.canChooseDirectories = false
        p.allowsMultipleSelection = false
        let done: (NSApplication.ModalResponse) -> Void = { [weak self] r in
            guard r == .OK, let url = p.url, let self else { return }
            self.value.text = url.path
            self.restyle(); self.onEdit?()
        }
        if let w = window { p.beginSheetModal(for: w, completionHandler: done) } else { done(p.runModal()) }
    }

    override func hoverChanged() { restyle() }

    func restyle() {
        let live = !isBlank
        check.alphaValue = live ? 1 : 0
        trash.alphaValue = live && hover ? 1 : 0
        trash.isEnabled = live
        let on = check.isOn || isBlank || kvStyle.fixed
        let c = on ? Theme.text : Theme.text3
        key.textColor = kvStyle.fixed ? Theme.text2 : c
        value.textColor = c
        desc?.textColor = on ? Theme.text2 : Theme.text3
        value.secret = isSecret && !isFile
        value.isHidden = isFile
        fileBox.isHidden = !isFile
        if isFile {
            let p = value.text
            if p.isEmpty { fileLabel.set("No file chosen", .mono, Theme.text3) }
            else { fileLabel.set(ComposerEdit.fileName(p), .mono, c) }
            pick.title = p.isEmpty ? "Select file…" : "Change"
            fileLabel.toolTip = p
        }
        kindButton.title = isFile ? "File" : "Text"
        eye.isHidden = !kvStyle.secrets || isFile
        eye.symbolName = isSecret ? "eye.slash" : "eye"
        eye.color = isSecret ? Theme.accent : Theme.text3
        eye.toolTip = isSecret ? "Masked; click to show" : "Mask the value"
        value.suggestions = nil
        needsDisplay = true
    }

    /// Copies what the row shows into a model value.
    func fill(_ kv: inout KeyValue) {
        kv.name = key.text
        kv.value = value.text
        kv.disabled = !check.isOn && !kvStyle.fixed
        if let desc { kv.description_p = desc.text }
        if kvStyle.secrets { kv.secret = isSecret }
        if kvStyle.files { kv.file = isFile }
    }
}

/// Column titles above the rows.
private final class KVHeader: GridRow {
    init(_ titles: [String], style: KVStyle) {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        topLine = false
        heightAnchor.constraint(equalToConstant: 26).isActive = true
        func box(_ s: String) -> FlippedView {
            let b = FlippedView(); b.translatesAutoresizingMaskIntoConstraints = false
            let l = label(s, .caption, Theme.text3)
            b.addSubview(l)
            NSLayoutConstraint.activate([
                l.leadingAnchor.constraint(equalTo: b.leadingAnchor, constant: 9),
                l.centerYAnchor.constraint(equalTo: b.centerYAnchor),
                l.trailingAnchor.constraint(lessThanOrEqualTo: b.trailingAnchor, constant: -4),
            ])
            return b
        }
        let lead = FlippedView(); lead.translatesAutoresizingMaskIntoConstraints = false
        lead.widthAnchor.constraint(equalToConstant: style.fixed ? 0 : 34).isActive = true
        let k = box(titles[0]), v = box(titles[1])
        let tail = FlippedView(); tail.translatesAutoresizingMaskIntoConstraints = false
        tail.widthAnchor.constraint(equalToConstant: style.fixed ? 8 : 30).isActive = true
        var cols: [NSView] = [lead, k, v]
        let d = style.description ? box(titles[2]) : nil
        if let d { cols.append(d) }
        cols.append(tail)
        let stack = NSStackView.horizontal(cols, spacing: 0)
        stack.distribution = .fill
        addSubview(stack)
        stack.pin(to: self)
        k.widthAnchor.constraint(equalTo: v.widthAnchor, multiplier: style.description ? 0.7 : 0.62).isActive = true
        if let d { d.widthAnchor.constraint(equalTo: v.widthAnchor, multiplier: 0.9).isActive = true }
        dividers = style.fixed ? [k] : [lead, k]
        if d != nil { dividers.append(v) }
    }
    required init?(coder: NSCoder) { fatalError() }
}

/// Headers, query parameters, form fields and variables. A blank row at the end
/// becomes a real row as soon as it is typed into, without rebuilding, so focus stays.
@MainActor
final class KeyValueTable: NSView {
    private(set) var rows: [KeyValue]
    var onChange: (([KeyValue]) -> Void)?
    /// Completions for the key column (header names).
    var keySuggestions: ((String) -> [String])? {
        didSet { for r in rowViews { r.key.suggestions = keySuggestions } }
    }
    private let kvStyle: KVStyle
    private let keyHint: String, valueHint: String
    private let titles: [String]
    private let grid = Surface()
    private let gridStack = NSStackView.vertical([], spacing: 0)
    private let root = NSStackView.vertical([], spacing: 8)
    private let bar = NSStackView()
    private let bulkButton = MiniButton("Bulk Edit", color: Theme.accent)
    private let hint = label("One per line as name: value. Start a line with // to turn it off.", .caption, Theme.text3)
    private var area: Area?
    private var bulkFormat: (([KeyValue]) async -> String)?
    private var bulkParse: ((String) async -> [KeyValue])?
    private var bulkOn = false
    private var parseSeq = 0
    override var isFlipped: Bool { true }

    /// - description: show a Description column.
    /// - files: each row can switch between text and a file (form-data); `KeyValue.file`, value is the path.
    /// - secrets: each row can be masked (`KeyValue.secret`), for variables.
    /// - title: a heading above the table, with the Bulk Edit switch at its right.
    /// - fixed: the names are given: no blank row, no remove, no enable box (path variables).
    init(_ rows: [KeyValue], key: String = "Key", value: String = "Value", description: Bool = false, files: Bool = false,
         secrets: Bool = false, title: String? = nil, fixed: Bool = false) {
        self.rows = rows
        keyHint = key; valueHint = value
        kvStyle = KVStyle(description: description, files: files, secrets: secrets, fixed: fixed)
        titles = [key, value, "Description"]
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false

        grid.fill = Theme.card
        grid.border = Theme.separator
        grid.radius = Theme.Radius.control
        grid.translatesAutoresizingMaskIntoConstraints = false
        grid.addSubview(gridStack)
        gridStack.pin(to: grid, insets: NSEdgeInsets(top: 1, left: 1, bottom: 1, right: 1))

        bar.orientation = .horizontal
        bar.alignment = .centerY
        bar.translatesAutoresizingMaskIntoConstraints = false
        if let title { bar.addArrangedSubview(label(title, .bodyMedium, Theme.text)) }
        bar.addArrangedSubview(NSView())
        bar.addArrangedSubview(bulkButton)
        bulkButton.isHidden = true
        bar.isHidden = title == nil

        hint.isHidden = true
        for v in [bar, grid, hint] as [NSView] {
            root.addArrangedSubview(v)
            v.leadingAnchor.constraint(equalTo: root.leadingAnchor).isActive = true
            v.trailingAnchor.constraint(equalTo: root.trailingAnchor).isActive = true
        }
        addSubview(root)
        root.pin(to: self)
        bulkButton.onClick = { [weak self] in self?.toggleBulk() }
        buildRows()
    }
    required init?(coder: NSCoder) { fatalError() }

    /// True while the person is typing in the table.
    var hasFocus: Bool {
        guard let r = window?.firstResponder as? NSView else { return false }
        return r.isDescendant(of: self)
    }

    /// Adds the Bulk Edit switch: `format` turns rows into "name: value" lines, `parse` back.
    func enableBulkEdit(format: @escaping ([KeyValue]) async -> String, parse: @escaping (String) async -> [KeyValue]) {
        bulkFormat = format
        bulkParse = parse
        bulkButton.isHidden = false
        bar.isHidden = false
    }

    /// Replaces the rows without rebuilding the table's place in its parent.
    func setRows(_ rows: [KeyValue]) {
        self.rows = rows
        if bulkOn {
            Task { [weak self] in
                guard let self, let f = self.bulkFormat else { return }
                let t = await f(rows)
                if self.bulkOn, !self.hasFocus { self.area?.text = t }
            }
        } else {
            buildRows()
        }
    }

    private var rowViews: [KVRow] { gridStack.arrangedSubviews.compactMap { $0 as? KVRow } }

    private func add(_ v: NSView) {
        gridStack.addArrangedSubview(v)
        v.leadingAnchor.constraint(equalTo: gridStack.leadingAnchor).isActive = true
        v.trailingAnchor.constraint(equalTo: gridStack.trailingAnchor).isActive = true
    }

    private func buildRows() {
        gridStack.arrangedSubviews.forEach { gridStack.removeArrangedSubview($0); $0.removeFromSuperview() }
        add(KVHeader(titles, style: kvStyle))
        for r in rows { add(makeRow(r)) }
        if !kvStyle.fixed { add(makeRow(nil)) }
    }

    private func makeRow(_ kv: KeyValue?) -> KVRow {
        let r = KVRow(kv, keyHint: keyHint, valueHint: valueHint, style: kvStyle)
        r.key.suggestions = keySuggestions
        r.onEdit = { [weak self, weak r] in
            guard let self, let r else { return }
            self.changed(r)
        }
        r.onDelete = { [weak self, weak r] in
            guard let self, let r, !r.isBlank, let i = self.rowViews.firstIndex(of: r), i < self.rows.count else { return }
            self.rows.remove(at: i)
            self.gridStack.removeArrangedSubview(r)
            r.removeFromSuperview()
            self.onChange?(self.rows)
        }
        return r
    }

    private func changed(_ r: KVRow) {
        if r.isBlank {
            r.isBlank = false
            var kv = KeyValue()
            r.fill(&kv)
            rows.append(kv)
            add(makeRow(nil))
        } else if let i = rowViews.firstIndex(of: r), i < rows.count {
            r.fill(&rows[i])
        }
        onChange?(rows)
    }

    // MARK: bulk edit

    private func toggleBulk() {
        bulkOn.toggle()
        bulkButton.title = bulkOn ? "Key-Value Edit" : "Bulk Edit"
        grid.isHidden = bulkOn
        hint.isHidden = !bulkOn
        if bulkOn {
            let a = Area("", rows: min(max(rows.count + 2, 6), 16), placeholder: "name: value")
            a.onChange = { [weak self] text in self?.bulkEdited(text) }
            root.insertArrangedSubview(a, at: 1)
            a.leadingAnchor.constraint(equalTo: root.leadingAnchor).isActive = true
            a.trailingAnchor.constraint(equalTo: root.trailingAnchor).isActive = true
            area = a
            Task { [weak self] in
                guard let self, let f = self.bulkFormat else { return }
                a.text = await f(self.rows)
            }
        } else {
            area?.removeFromSuperview()
            area = nil
            buildRows()
        }
    }

    private func bulkEdited(_ text: String) {
        parseSeq += 1
        let seq = parseSeq
        Task { [weak self] in
            guard let self, let p = self.bulkParse else { return }
            let r = await p(text)
            guard seq == self.parseSeq else { return }
            self.rows = r
            self.onChange?(r)
        }
    }
}

// MARK: - Method picker

/// The colored HTTP method; a click lists the others, and "Other…" takes any.
final class MethodPicker: NSView {
    var method: String { didSet { needsDisplay = true } }
    var onPick: ((String) -> Void)?
    private var hover = false
    override var isFlipped: Bool { true }

    init(_ method: String, height: CGFloat = 30) {
        self.method = method
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        widthAnchor.constraint(equalToConstant: 104).isActive = true
        heightAnchor.constraint(equalToConstant: height).isActive = true
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        let p = NSBezierPath(roundedRect: bounds, xRadius: Theme.Radius.control, yRadius: Theme.Radius.control)
        (hover ? Theme.fillHover : Theme.fill).setFill(); p.fill()
        let a: [NSAttributedString.Key: Any] = [.font: Theme.monoMedium(12.5), .foregroundColor: Theme.method(method)]
        let s = method as NSString
        let sz = s.size(withAttributes: a)
        s.draw(at: NSPoint(x: 12, y: bounds.midY - sz.height / 2), withAttributes: a)
        drawSymbol("chevron.down", in: NSRect(x: bounds.maxX - 24, y: 0, width: 16, height: bounds.height), color: Theme.text3, size: 9, weight: .bold)
    }

    override func mouseDown(with event: NSEvent) {
        let m = NSMenu()
        for name in ComposerModel.methods {
            let i = NSMenuItem(title: name, action: #selector(picked(_:)), keyEquivalent: "")
            i.target = self
            i.attributedTitle = NSAttributedString(string: name, attributes: [.font: Theme.monoMedium(12.5), .foregroundColor: Theme.method(name)])
            i.state = name == method ? .on : .off
            m.addItem(i)
        }
        m.addItem(.separator())
        let other = NSMenuItem(title: "Other…", action: #selector(askOther), keyEquivalent: "")
        other.target = self
        other.state = ComposerModel.methods.contains(method) ? .off : .on
        m.addItem(other)
        m.popUp(positioning: nil, at: NSPoint(x: 0, y: bounds.height + 4), in: self)
    }
    @objc private func picked(_ i: NSMenuItem) { method = i.title; onPick?(i.title) }
    @objc private func askOther() {
        askText(window, title: "Custom method", info: "Letters, digits and dashes, such as PROPFIND.", initial: ComposerModel.methods.contains(method) ? "" : method,
                placeholder: "PROPFIND", button: "Use") { [weak self] text in
            let m = text.uppercased().filter { $0.isLetter || $0.isNumber || $0 == "-" }
            guard !m.isEmpty else { return }
            self?.method = m
            self?.onPick?(m)
        }
    }
    override func mouseEntered(with event: NSEvent) { hover = true; needsDisplay = true }
    override func mouseExited(with event: NSEvent) { hover = false; needsDisplay = true }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }
}

/// A pill that opens a menu of titles, in the same style as the method picker.
final class PillMenu: NSView {
    var items: [String] { didSet { invalidateIntrinsicContentSize(); needsDisplay = true } }
    var selected: Int { didSet { needsDisplay = true } }
    var onPick: ((Int) -> Void)?
    private var hover = false
    private let height: CGFloat
    override var isFlipped: Bool { true }

    init(_ items: [String], selected: Int, height: CGFloat = 30) {
        self.items = items
        self.selected = selected
        self.height = height
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
    }
    required init?(coder: NSCoder) { fatalError() }

    override var intrinsicContentSize: NSSize {
        let w = items.map { ($0 as NSString).size(withAttributes: Theme.attrs(.bodyMedium)).width }.max() ?? 40
        return NSSize(width: w + 42, height: height)
    }

    override func draw(_ dirtyRect: NSRect) {
        let p = NSBezierPath(roundedRect: bounds, xRadius: Theme.Radius.control, yRadius: Theme.Radius.control)
        (hover ? Theme.fillHover : Theme.fill).setFill(); p.fill()
        let a = Theme.attrs(.bodyMedium)
        let t = (items.indices.contains(selected) ? items[selected] : "") as NSString
        let sz = t.size(withAttributes: a)
        t.draw(at: NSPoint(x: 12, y: bounds.midY - sz.height / 2), withAttributes: a)
        drawSymbol("chevron.down", in: NSRect(x: bounds.maxX - 24, y: 0, width: 16, height: bounds.height), color: Theme.text3, size: 9, weight: .bold)
    }

    override func mouseDown(with event: NSEvent) {
        let m = NSMenu()
        for (i, name) in items.enumerated() {
            let item = NSMenuItem(title: name, action: #selector(picked(_:)), keyEquivalent: "")
            item.target = self
            item.tag = i
            item.state = i == selected ? .on : .off
            m.addItem(item)
        }
        m.popUp(positioning: nil, at: NSPoint(x: 0, y: bounds.height + 4), in: self)
    }
    @objc private func picked(_ i: NSMenuItem) { selected = i.tag; onPick?(i.tag) }
    override func mouseEntered(with event: NSEvent) { hover = true; needsDisplay = true }
    override func mouseExited(with event: NSEvent) { hover = false; needsDisplay = true }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }
}

/// A single-line input in the code-field style whose text can be hidden, for passwords.
@MainActor
final class SecretField: Surface, NSTextFieldDelegate {
    private let plain = NSTextField()
    private let masked = NSSecureTextField()
    private let eye = MiniButton("", symbol: "eye", tip: "Show or hide")
    private var shown = false
    var onChange: ((String) -> Void)?
    var text: String {
        get { shown ? plain.stringValue : masked.stringValue }
        set { plain.stringValue = newValue; masked.stringValue = newValue }
    }

    init(_ text: String = "", placeholder: String = "") {
        super.init(frame: .zero)
        fill = Theme.code
        border = Theme.separator
        radius = 6
        translatesAutoresizingMaskIntoConstraints = false
        for f in [plain, masked] as [NSTextField] {
            f.isBordered = false
            f.drawsBackground = false
            f.focusRingType = .none
            f.font = Theme.font(.mono)
            f.textColor = Theme.text
            f.placeholderAttributedString = NSAttributedString(string: placeholder, attributes: Theme.attrs(.mono, Theme.text3))
            f.cell?.usesSingleLineMode = true
            f.cell?.isScrollable = true
            f.delegate = self
            f.translatesAutoresizingMaskIntoConstraints = false
            addSubview(f)
            NSLayoutConstraint.activate([
                f.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
                f.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -32),
                f.centerYAnchor.constraint(equalTo: centerYAnchor),
            ])
        }
        plain.isHidden = true
        self.text = text
        addSubview(eye)
        NSLayoutConstraint.activate([
            eye.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -4),
            eye.centerYAnchor.constraint(equalTo: centerYAnchor),
            heightAnchor.constraint(equalToConstant: 28),
        ])
        eye.onClick = { [weak self] in
            guard let self else { return }
            let t = self.text
            self.shown.toggle()
            self.plain.stringValue = t; self.masked.stringValue = t
            self.plain.isHidden = !self.shown; self.masked.isHidden = self.shown
            self.eye.symbolName = self.shown ? "eye.slash" : "eye"
            self.window?.makeFirstResponder(self.shown ? self.plain : self.masked)
        }
    }
    required init?(coder: NSCoder) { fatalError() }

    func controlTextDidChange(_ obj: Notification) {
        guard let f = obj.object as? NSTextField else { return }
        onChange?(f.stringValue)
    }
}

// MARK: - List rows

/// A custom-drawn row in the composer's sidebar, with hover, click and a context menu.
final class ListRow: NSView {
    var selected = false { didSet { needsDisplay = true } }
    var onClick: (() -> Void)?
    var menuProvider: (() -> NSMenu?)?
    var paint: ((ListRow, Bool) -> Void)?
    private(set) var hover = false
    override var isFlipped: Bool { true }

    init(height: CGFloat) {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        heightAnchor.constraint(equalToConstant: height).isActive = true
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        let r = bounds.insetBy(dx: 8, dy: 1)
        if selected {
            Theme.accentSoft.setFill()
            NSBezierPath(roundedRect: r, xRadius: 7, yRadius: 7).fill()
        } else if hover {
            Theme.fill.setFill()
            NSBezierPath(roundedRect: r, xRadius: 7, yRadius: 7).fill()
        }
        paint?(self, selected)
    }

    /// Draws a string clipped to a width, vertically centered on `y`.
    func text(_ s: String, _ a: [NSAttributedString.Key: Any], x: CGFloat, midY: CGFloat, maxX: CGFloat) {
        let p = NSMutableParagraphStyle(); p.lineBreakMode = .byTruncatingTail
        var b = a; b[.paragraphStyle] = p
        let sz = (s as NSString).size(withAttributes: a)
        (s as NSString).draw(in: NSRect(x: x, y: midY - sz.height / 2, width: max(maxX - x, 10), height: sz.height), withAttributes: b)
    }

    override func mouseUp(with event: NSEvent) {
        if bounds.contains(convert(event.locationInWindow, from: nil)) { onClick?() }
    }
    override func menu(for event: NSEvent) -> NSMenu? { menuProvider?() }
    override func mouseEntered(with event: NSEvent) { hover = true; needsDisplay = true }
    override func mouseExited(with event: NSEvent) { hover = false; needsDisplay = true }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }
}

/// An NSMenu whose items run closures.
final class ActionMenu: NSMenu {
    private var actions: [() -> Void] = []

    @discardableResult
    func add(_ title: String, checked: Bool = false, enabled: Bool = true, _ run: @escaping () -> Void) -> NSMenuItem {
        let i = NSMenuItem(title: title, action: enabled ? #selector(fire(_:)) : nil, keyEquivalent: "")
        i.target = self
        i.tag = actions.count
        i.state = checked ? .on : .off
        actions.append(run)
        addItem(i)
        return i
    }
    func separator() { addItem(.separator()) }
    @objc private func fire(_ i: NSMenuItem) { actions[i.tag]() }
}

// MARK: - Alerts

/// Asks for one line of text in a sheet. Nil means cancelled.
@MainActor
func askText(_ window: NSWindow?, title: String, info: String = "", initial: String = "", placeholder: String = "",
             button: String, done: @escaping (String) -> Void) {
    let a = NSAlert()
    a.messageText = title
    a.informativeText = info
    a.addButton(withTitle: button)
    a.addButton(withTitle: "Cancel")
    let f = NSTextField(frame: NSRect(x: 0, y: 0, width: 280, height: 24))
    f.stringValue = initial
    f.placeholderString = placeholder
    a.accessoryView = f
    a.window.initialFirstResponder = f
    let handle: (NSApplication.ModalResponse) -> Void = { r in
        let t = f.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        if r == .alertFirstButtonReturn && !t.isEmpty { done(t) }
    }
    if let window { a.beginSheetModal(for: window, completionHandler: handle) } else { handle(a.runModal()) }
}

/// Shows a short message in a sheet.
@MainActor
func tell(_ window: NSWindow?, _ title: String, _ info: String) {
    let a = NSAlert()
    a.alertStyle = .warning
    a.messageText = title
    a.informativeText = info
    if let window { a.beginSheetModal(for: window) } else { a.runModal() }
}
