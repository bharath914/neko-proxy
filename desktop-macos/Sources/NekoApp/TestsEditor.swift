import AppKit
import NekoAPI

/// The Tests section: declarative assertions run after every send. There is no
/// scripting; each row is a check the core evaluates against the response.
@MainActor
final class TestsEditor: NSView {
    let model: ComposerModel
    private var rows: [Assertion]
    private let column = NSStackView.vertical([], spacing: 12)
    private var card: NSView?
    private var targetFields: [Int: Field] = [:]
    private var expectedFields: [Int: Field] = [:]
    private var pendingFocus: (row: Int, field: String)?
    override var isFlipped: Bool { true }

    init(model: ComposerModel) {
        self.model = model
        rows = model.draft.tests
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false

        let note = label("Tests run after every send and report in the response's Tests tab.", .callout, Theme.text2)
        add(note)
        rebuildCard()
        add(actions())

        let scroll = bodyScroll(column)
        addSubview(scroll)
        scroll.pin(to: self)
    }
    required init?(coder: NSCoder) { fatalError() }

    private func add(_ v: NSView, at index: Int? = nil) {
        if let index { column.insertArrangedSubview(v, at: index) } else { column.addArrangedSubview(v) }
        v.widthAnchor.constraint(equalTo: column.widthAnchor).isActive = true
    }

    private func commit() { let r = rows; model.edit { $0.tests = r } }

    private func update(_ i: Int, _ change: (inout Assertion) -> Void) {
        guard rows.indices.contains(i) else { return }
        change(&rows[i])
        commit()
    }

    // MARK: list

    private func rebuildCard() {
        if let card { column.removeArrangedSubview(card); card.removeFromSuperview() }
        targetFields = [:]; expectedFields = [:]
        let c: NSView
        if rows.isEmpty {
            let t = label("No tests yet", .bodyMedium, Theme.text)
            let s = label("Add a check below, or start from a common one.", .callout, Theme.text2)
            c = groupCard([NSStackView.vertical([t, s], spacing: 4)], padding: NSEdgeInsets(top: 16, left: 16, bottom: 16, right: 16))
        } else {
            c = groupCard(rows.indices.map(row), padding: NSEdgeInsets(top: 8, left: 12, bottom: 8, right: 8))
        }
        card = c
        add(c, at: 1)
        if let f = pendingFocus {
            pendingFocus = nil
            let field = f.field == "target" ? targetFields[f.row] : expectedFields[f.row]
            DispatchQueue.main.async { [weak self] in
                if let field { self?.window?.makeFirstResponder(field.input) }
            }
        }
    }

    private func row(_ i: Int) -> NSView {
        let a = rows[i]
        let check = CheckBox(on: !a.disabled)
        let kinds = AssertionSchema.kinds
        let kind = BodyPopup(kinds.map(AssertionSchema.title), selected: kinds.firstIndex(of: a.kind) ?? 0, width: 132)
        kind.onPick = { [weak self] j in
            self?.update(i) { $0 = AssertionSchema.changing($0, to: kinds[j]) }
            self?.rebuildCard()
        }
        var dim: [NSView] = [kind]
        var views: [NSView] = [check, kind]

        var target: Field?
        if AssertionSchema.needsTarget(a.kind) {
            let f = Field(a.target, placeholder: AssertionSchema.targetHint(a.kind))
            f.onChange = { [weak self] t in self?.update(i) { $0.target = t } }
            targetFields[i] = f
            target = f
            dim.append(f); views.append(f)
        }

        let ops = AssertionSchema.operators(for: a.kind)
        let op = BodyPopup(ops.map(AssertionSchema.opTitle), selected: ops.firstIndex(of: a.op) ?? 0, width: 136)
        op.onPick = { [weak self] j in
            self?.update(i) { $0 = AssertionSchema.changing($0, to: ops[j]) }
            self?.rebuildCard()
        }
        dim.append(op); views.append(op)

        var expected: Field?
        if AssertionSchema.needsExpected(a.op) {
            let f = Field(a.expected, placeholder: AssertionSchema.expectedHint(a.kind, a.op))
            f.onChange = { [weak self] t in self?.update(i) { $0.expected = t } }
            expectedFields[i] = f
            expected = f
            dim.append(f); views.append(f)
        } else {
            let spacer = NSView()
            spacer.translatesAutoresizingMaskIntoConstraints = false
            spacer.setContentHuggingPriority(.init(1), for: .horizontal)
            views.append(spacer)
        }

        let trash = IconButton("minus.circle", tip: "Remove test")
        trash.onClick = { [weak self] in
            guard let self, self.rows.indices.contains(i) else { return }
            self.rows.remove(at: i)
            self.commit()
            self.rebuildCard()
        }
        views.append(trash)

        let stack = NSStackView.horizontal(views, spacing: 8)
        stack.distribution = .fill
        stack.setCustomSpacing(10, after: check)
        for f in [target, expected].compactMap({ $0 }) { f.setContentHuggingPriority(.init(200), for: .horizontal) }
        if let target, let expected {
            target.widthAnchor.constraint(equalTo: expected.widthAnchor, multiplier: 0.85).isActive = true
        }
        let setDim = { (on: Bool) in dim.forEach { $0.alphaValue = on ? 1 : 0.5 } }
        setDim(!a.disabled)
        check.onToggle = { [weak self] on in
            self?.update(i) { $0.disabled = !on }
            setDim(on)
        }
        stack.toolTip = AssertionSchema.describe(a)
        return stack
    }

    // MARK: adding

    private func actions() -> NSView {
        let wrap = WrapView()
        wrap.spacing = 8
        let addChip = Chip("Add test", symbol: "plus")
        addChip.onClick = { [weak self] in self?.append(AssertionSchema.presets[0].assertion, focus: "expected") }
        wrap.addSubview(addChip)
        for p in AssertionSchema.presets {
            let c = Chip(p.title, style: .text)
            c.onClick = { [weak self] in self?.append(p.assertion, focus: p.focus) }
            wrap.addSubview(c)
        }
        return wrap
    }

    private func append(_ a: Assertion, focus: String?) {
        rows.append(a)
        commit()
        if let focus { pendingFocus = (rows.count - 1, focus) }
        rebuildCard()
    }
}
