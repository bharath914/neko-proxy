import AppKit
import NekoAPI

/// Shared geometry for the Sequence header and rows.
private struct SeqGeo {
    let pad: CGFloat = 16, numW: CGFloat = 44
    let labelW: CGFloat, barL: CGFloat, barR: CGFloat
    init(width: CGFloat) {
        labelW = min(max(width * 0.34, 220), 420)
        barL = pad + numW + labelW
        barR = width - pad - 72
    }
}

/// The time axis shared by every row: when the first flow began and how long the run lasts.
private struct SeqAxis {
    var origin: Int64 = 0
    var span: Double = 1
}

private func drawText(_ s: String, _ a: [NSAttributedString.Key: Any], x: CGFloat, midY: CGFloat, maxW: CGFloat? = nil, right: Bool = false) {
    let ns = s as NSString
    let sz = ns.size(withAttributes: a)
    if let maxW, sz.width > maxW {
        let p = NSMutableParagraphStyle(); p.lineBreakMode = .byTruncatingTail
        var b = a; b[.paragraphStyle] = p
        ns.draw(in: NSRect(x: x, y: midY - sz.height / 2, width: max(maxW, 0), height: sz.height), withAttributes: b)
    } else {
        ns.draw(at: NSPoint(x: right ? x - sz.width : x, y: midY - sz.height / 2), withAttributes: a)
    }
}

private final class SeqRowView: NSTableRowView {
    var flow: Flow? { didSet { needsDisplay = true } }
    var axis = SeqAxis()
    var index = 0
    override var isFlipped: Bool { true }
    override var isEmphasized: Bool { get { true } set {} }
    override func drawSelection(in dirtyRect: NSRect) {}
    override func drawBackground(in dirtyRect: NSRect) {}

    override func draw(_ dirtyRect: NSRect) {
        guard let f = flow else { return }
        if isSelected { Theme.accentSoft.setFill(); bounds.fill() }
        else if index % 2 == 1 { Theme.rowAlt.setFill(); bounds.fill() }
        let g = SeqGeo(width: bounds.width), midY = bounds.midY
        drawText(String(f.id), Theme.attrs(.monoSmall, Theme.text3), x: g.pad, midY: midY)
        let ma = Theme.attrs(.caption, Theme.method(f.method), extra: [.kern: 0.3])
        drawText(f.method, ma, x: g.pad + g.numW, midY: midY)
        let tx = g.pad + g.numW + (f.method as NSString).size(withAttributes: ma).width + 8
        drawText(f.isTunnel ? f.host : f.host + f.path, Theme.attrs(.body, Theme.text), x: tx, midY: midY, maxW: g.barL - tx - 12)

        let color = Theme.status(f.status, failed: f.state == .failed)
        let width = g.barR - g.barL
        let x0 = g.barL + width * CGFloat(Double(f.startedAtMs - axis.origin) / axis.span)
        let w = max(width * CGFloat(max(f.durationMs, 0) / axis.span), 3)
        color.setFill()
        NSBezierPath(roundedRect: NSRect(x: x0, y: midY - 5, width: w, height: 10), xRadius: 5, yRadius: 5).fill()
        let pending = f.state == .pending || f.state == .receiving
        drawText(pending ? "…" : Fmt.duration(f.durationMs), Theme.attrs(.monoSmall, Theme.text2), x: bounds.width - g.pad, midY: midY, right: true)
    }
}

private final class SeqHeader: NSView {
    var axis = SeqAxis() { didSet { needsDisplay = true } }
    override var isFlipped: Bool { true }
    override func draw(_ dirtyRect: NSRect) {
        Theme.toolbar.setFill(); bounds.fill()
        Theme.separator.setFill(); NSRect(x: 0, y: bounds.height - 1, width: bounds.width, height: 1).fill()
        let g = SeqGeo(width: bounds.width), midY = bounds.midY
        let o = Theme.attrs(.overline, Theme.text3)
        drawText("#", o, x: g.pad, midY: midY)
        drawText("REQUEST", o, x: g.pad + g.numW, midY: midY)
        let m = Theme.attrs(.monoSmall, Theme.text3)
        for i in 0...4 {
            let x = g.barL + (g.barR - g.barL) * CGFloat(i) / 4
            let s = i == 0 ? "0" : Fmt.duration(axis.span * Double(i) / 4)
            let w = (s as NSString).size(withAttributes: m).width
            drawText(s, m, x: i == 0 ? x : i == 4 ? x - w : x - w / 2, midY: midY)
        }
    }
}

/// The inspector's flows on a time axis, in the order requests began.
@MainActor
final class SequenceView: NSView, NSTableViewDataSource, NSTableViewDelegate {
    let model: FlowModel
    var onOpen: ((Flow) -> Void)?
    private let header = SeqHeader()
    private let scroll = NSScrollView()
    private let table = NSTableView()
    private let empty = label("No flows yet", .headline, Theme.text3)
    private var axis = SeqAxis()
    override var isFlipped: Bool { true }

    init(model: FlowModel) {
        self.model = model
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        header.translatesAutoresizingMaskIntoConstraints = false
        scroll.translatesAutoresizingMaskIntoConstraints = false
        empty.translatesAutoresizingMaskIntoConstraints = false
        let col = NSTableColumn(identifier: .init("seq"))
        col.resizingMask = .autoresizingMask
        table.addTableColumn(col)
        table.headerView = nil
        table.dataSource = self
        table.delegate = self
        table.rowHeight = 30
        table.intercellSpacing = .zero
        table.style = .plain
        table.backgroundColor = .clear
        table.focusRingType = .none
        table.target = self
        table.action = #selector(clicked)
        scroll.documentView = table
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        for v in [header, scroll, empty] { addSubview(v) }
        NSLayoutConstraint.activate([
            header.topAnchor.constraint(equalTo: topAnchor),
            header.leadingAnchor.constraint(equalTo: leadingAnchor),
            header.trailingAnchor.constraint(equalTo: trailingAnchor),
            header.heightAnchor.constraint(equalToConstant: 30),
            scroll.topAnchor.constraint(equalTo: header.bottomAnchor),
            scroll.leadingAnchor.constraint(equalTo: leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: trailingAnchor),
            scroll.bottomAnchor.constraint(equalTo: bottomAnchor),
            empty.centerXAnchor.constraint(equalTo: centerXAnchor),
            empty.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) { Theme.content.setFill(); bounds.fill() }

    func reload() {
        let flows = model.flows
        var a = SeqAxis()
        if let first = flows.map(\.startedAtMs).min() {
            a.origin = first
            let end = flows.map { $0.startedAtMs + Int64(max($0.durationMs, 0)) }.max() ?? first
            a.span = Double(max(end - first, 1))
        }
        axis = a
        header.axis = a
        empty.isHidden = !flows.isEmpty
        scroll.isHidden = flows.isEmpty
        table.reloadData()
    }

    @objc private func clicked() {
        let r = table.clickedRow
        if r >= 0 && r < model.flows.count { onOpen?(model.flows[r]) }
    }

    func numberOfRows(in tableView: NSTableView) -> Int { model.flows.count }

    func tableView(_ tableView: NSTableView, rowViewForRow row: Int) -> NSTableRowView? {
        let id = NSUserInterfaceItemIdentifier("seqrow")
        let v = (tableView.makeView(withIdentifier: id, owner: nil) as? SeqRowView) ?? {
            let n = SeqRowView(); n.identifier = id; return n
        }()
        v.flow = model.flows[row]
        v.axis = axis
        v.index = row
        return v
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? { nil }
}
