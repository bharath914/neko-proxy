import AppKit
import NekoAPI

/// Column geometry shared by the header and the rows. Fixed columns hug the
/// edges; host and path share what is left, 40/60.
struct Columns {
    var id, source, method, host, path, status, size, time: NSRect

    init(width: CGFloat, height: CGFloat) {
        let pad: CGFloat = 14
        func r(_ x: CGFloat, _ w: CGFloat) -> NSRect { NSRect(x: x, y: 0, width: w, height: height) }
        id = r(pad, 36)
        source = r(58, 62)
        method = r(126, 66)
        time = r(width - pad - 62, 62)
        size = r(time.minX - 6 - 62, 62)
        status = r(size.minX - 70, 70)
        let flexStart = method.maxX + 8
        let flex = max(status.minX - 8 - flexStart, 120)
        host = r(flexStart, (flex * 0.46).rounded())
        path = r(host.maxX + 8, flex - host.width - 8)
    }
}

final class FlowRowView: NSTableRowView {
    var flow: Flow? { didSet { needsDisplay = true } }
    var index = 0
    override var isFlipped: Bool { true }
    override var isEmphasized: Bool { get { true } set {} }

    override func drawSelection(in dirtyRect: NSRect) {}
    override func drawBackground(in dirtyRect: NSRect) {}

    override func draw(_ dirtyRect: NSRect) {
        guard let f = flow else { return }
        if isSelected {
            Theme.accentSoft.setFill(); bounds.fill()
            Theme.accent.setFill()
            NSRect(x: 0, y: 4, width: 3, height: bounds.height - 8).fill()
        } else if index % 2 == 1 {
            Theme.rowAlt.setFill(); bounds.fill()
        }
        let c = Columns(width: bounds.width, height: bounds.height)
        let midY = bounds.midY

        func text(_ s: String, _ a: [NSAttributedString.Key: Any], in r: NSRect, align: NSTextAlignment = .left, dx: CGFloat = 0) {
            let ns = s as NSString
            let sz = ns.size(withAttributes: a)
            let w = min(sz.width, r.width)
            var x = r.minX + dx
            if align == .right { x = r.maxX - w }
            let clip = NSRect(x: r.minX, y: 0, width: r.width, height: bounds.height)
            NSGraphicsContext.saveGraphicsState()
            clip.clip()
            // Truncate with an ellipsis when it does not fit.
            if sz.width > r.width {
                let p = NSMutableParagraphStyle(); p.lineBreakMode = .byTruncatingTail
                var b = a; b[.paragraphStyle] = p
                ns.draw(in: NSRect(x: x, y: midY - sz.height / 2, width: r.width - dx, height: sz.height), withAttributes: b)
            } else {
                ns.draw(at: NSPoint(x: x, y: midY - sz.height / 2), withAttributes: a)
            }
            NSGraphicsContext.restoreGraphicsState()
        }

        text(String(f.id), Theme.attrs(.monoSmall, Theme.text3), in: c.id)

        // Source badge.
        let (srcText, srcColor): (String, NSColor) = {
            switch f.source {
            case .sdk: return ("SDK", Theme.purple)
            case .merged: return ("BOTH", Theme.teal)
            default: return ("PROXY", Theme.text2)
            }
        }()
        let sa = Theme.attrs(.caption, srcColor, extra: [.kern: 0.5])
        let ssz = (srcText as NSString).size(withAttributes: sa)
        let badge = NSRect(x: c.source.minX, y: midY - 9, width: ssz.width + 12, height: 18)
        let bp = NSBezierPath(roundedRect: badge, xRadius: 4, yRadius: 4)
        (f.source == .proxy ? Theme.fill : Theme.tint(srcColor, 0.2)).setFill()
        bp.fill()
        (srcText as NSString).draw(at: NSPoint(x: badge.minX + 6, y: midY - ssz.height / 2), withAttributes: sa)

        let method = f.isTunnel ? "CONNECT" : f.method
        text(method, Theme.attrs(.caption, Theme.method(method), extra: [.kern: 0.3]), in: c.method)

        var hostText = f.host
        if f.port != 0 && f.port != 80 && f.port != 443 { hostText += ":\(f.port)" }
        var hostDX: CGFloat = 0
        if f.isTunnel || f.isPinned {
            drawSymbol(f.isPinned ? "lock.fill" : "lock.open", in: NSRect(x: c.host.minX, y: 0, width: 14, height: bounds.height),
                       color: f.isPinned ? Theme.orange : Theme.text3, size: 10, weight: .semibold)
            hostDX = 16
        }
        text(hostText, Theme.attrs(.body, Theme.text), in: c.host, dx: hostDX)
        text(f.isTunnel ? "tunnel" : f.path, Theme.attrs(.body, f.isTunnel ? Theme.text3 : Theme.text2), in: c.path)

        // Status: dot and number.
        if f.status != 0 {
            let color = Theme.status(f.status, failed: f.state == .failed)
            color.setFill()
            NSBezierPath(ovalIn: NSRect(x: c.status.minX, y: midY - 3, width: 6, height: 6)).fill()
            text(String(f.status), Theme.attrs(.mono, Theme.text), in: c.status, dx: 12)
        } else {
            text(f.state == .failed ? "Error" : "—", Theme.attrs(.mono, f.state == .failed ? Theme.red : Theme.text3), in: c.status, dx: 12)
        }
        text(f.resSize == 0 ? "—" : Fmt.size(f.resSize), Theme.attrs(.monoSmall, f.resSize == 0 ? Theme.text3 : Theme.text2), in: c.size, align: .right)
        let pending = f.state == .pending || f.state == .receiving
        let slow = f.durationMs >= 1000
        text(f.isTunnel || (f.durationMs == 0 && !pending) ? "—" : pending ? "…" : Fmt.duration(f.durationMs).replacingOccurrences(of: " s", with: " s"),
             Theme.attrs(.monoSmall, slow ? Theme.orange : Theme.text2), in: c.time, align: .right)
    }
}

final class ListHeader: NSView {
    override var isFlipped: Bool { true }
    override func draw(_ dirtyRect: NSRect) {
        Theme.content.setFill(); bounds.fill()
        let c = Columns(width: bounds.width, height: bounds.height)
        let a = Theme.attrs(.caption, Theme.text3, extra: [.kern: 0.7])
        func t(_ s: String, _ r: NSRect, right: Bool = false) {
            let sz = (s as NSString).size(withAttributes: a)
            (s as NSString).draw(at: NSPoint(x: right ? r.maxX - sz.width : r.minX, y: bounds.midY - sz.height / 2), withAttributes: a)
        }
        t("#", c.id); t("SOURCE", c.source); t("METHOD", c.method); t("HOST", c.host); t("PATH", c.path)
        t("STATUS", c.status, right: false); t("SIZE", c.size, right: true); t("TIME", c.time, right: true)
        Theme.separator.setFill()
        NSRect(x: 0, y: bounds.height - 1, width: bounds.width, height: 1).fill()
    }
}

@MainActor
final class FlowList: NSView, NSTableViewDataSource, NSTableViewDelegate {
    let model: FlowModel
    private let header = ListHeader()
    private let scroll = NSScrollView()
    private let table = NSTableView()
    private let empty = FlippedView()
    private var emptyTitle = label("No requests yet", .headline, Theme.text2)
    private var emptySub = label("", .callout, Theme.text3)
    var onSelect: ((Flow?) -> Void)?
    private var selectedID: UInt64?
    private var shown: Flow?
    override var isFlipped: Bool { true }

    init(model: FlowModel) {
        self.model = model
        super.init(frame: .zero)
        header.translatesAutoresizingMaskIntoConstraints = false
        scroll.translatesAutoresizingMaskIntoConstraints = false

        let col = NSTableColumn(identifier: .init("row"))
        col.resizingMask = .autoresizingMask
        table.addTableColumn(col)
        table.headerView = nil
        table.dataSource = self
        table.delegate = self
        table.rowHeight = 30
        table.intercellSpacing = .zero
        table.style = .plain
        table.backgroundColor = .clear
        table.selectionHighlightStyle = .regular
        table.columnAutoresizingStyle = .uniformColumnAutoresizingStyle
        table.focusRingType = .none
        scroll.documentView = table
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay

        emptyTitle.alignment = .center
        emptySub.alignment = .center
        empty.translatesAutoresizingMaskIntoConstraints = false
        let es = NSStackView.vertical([emptyTitle, emptySub], spacing: 4)
        es.alignment = .centerX
        empty.addSubview(es)
        NSLayoutConstraint.activate([
            es.centerXAnchor.constraint(equalTo: empty.centerXAnchor),
            es.centerYAnchor.constraint(equalTo: empty.centerYAnchor, constant: -20),
        ])

        addSubview(header); addSubview(scroll); addSubview(empty)
        NSLayoutConstraint.activate([
            header.topAnchor.constraint(equalTo: topAnchor),
            header.leadingAnchor.constraint(equalTo: leadingAnchor),
            header.trailingAnchor.constraint(equalTo: trailingAnchor),
            header.heightAnchor.constraint(equalToConstant: 30),
            scroll.topAnchor.constraint(equalTo: header.bottomAnchor),
            scroll.leadingAnchor.constraint(equalTo: leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: trailingAnchor),
            scroll.bottomAnchor.constraint(equalTo: bottomAnchor),
            empty.topAnchor.constraint(equalTo: header.bottomAnchor),
            empty.leadingAnchor.constraint(equalTo: leadingAnchor),
            empty.trailingAnchor.constraint(equalTo: trailingAnchor),
            empty.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) { Theme.content.setFill(); bounds.fill() }

    func selected() -> Flow? {
        let r = table.selectedRow
        return r >= 0 && r < model.flows.count ? model.flows[r] : nil
    }

    func reload() {
        let atBottom = scroll.contentView.bounds.maxY >= table.bounds.maxY - 8
        table.reloadData()
        let noRows = model.flows.isEmpty
        empty.isHidden = !noRows
        scroll.isHidden = noRows
        if noRows {
            let filtered = model.kind != .all || !model.text.isEmpty || model.host != nil || !model.deviceIDs.isEmpty
            emptyTitle.set(filtered ? "No matching requests" : "No requests yet", .headline, Theme.text2)
            emptySub.set(filtered ? "Try another filter." : "Point an app or browser at \(model.listen.isEmpty ? "127.0.0.1:8899" : model.listen).",
                         .callout, Theme.text3)
        }
        if let id = selectedID, let row = model.row(forID: id) {
            if table.selectedRow != row { table.selectRowIndexes([row], byExtendingSelection: false) }
            let now = model.flows[row]
            if let prev = shown, prev.state != now.state || prev.status != now.status || prev.resSize != now.resSize {
                shown = now
                onSelect?(now)
            }
        } else if selectedID != nil {
            selectedID = nil; shown = nil
            onSelect?(nil)
        }
        if atBottom && selectedID == nil && !model.flows.isEmpty { table.scrollRowToVisible(model.flows.count - 1) }
    }

    func select(id: UInt64) {
        if let r = model.row(forID: id) {
            table.selectRowIndexes([r], byExtendingSelection: false)
            table.scrollRowToVisible(r)
        }
    }

    func numberOfRows(in tableView: NSTableView) -> Int { model.flows.count }

    func tableView(_ tableView: NSTableView, rowViewForRow row: Int) -> NSTableRowView? {
        let id = NSUserInterfaceItemIdentifier("flowrow")
        let v = (tableView.makeView(withIdentifier: id, owner: nil) as? FlowRowView) ?? {
            let n = FlowRowView(); n.identifier = id; return n
        }()
        v.flow = model.flows[row]
        v.index = row
        return v
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? { nil }

    func tableViewSelectionDidChange(_ notification: Notification) {
        let f = selected()
        selectedID = f?.id
        shown = f
        onSelect?(f)
    }
}
