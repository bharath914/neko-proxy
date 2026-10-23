import AppKit
import NekoAPI

final class HostRowView: NSTableRowView {
    var title = "", count: UInt64 = 0
    var errors = false, pinned = false, decrypted = true, tunnelOnly = false, isAll = false
    override var isFlipped: Bool { true }
    override func drawSelection(in dirtyRect: NSRect) {}
    override func drawBackground(in dirtyRect: NSRect) {}

    override func draw(_ dirtyRect: NSRect) {
        let r = bounds.insetBy(dx: 8, dy: 1)
        if isSelected {
            let p = NSBezierPath(roundedRect: r, xRadius: 8, yRadius: 8)
            Theme.accentSoft.setFill(); p.fill()
            Theme.accent.setFill()
            NSBezierPath(roundedRect: NSRect(x: 0, y: 8, width: 3, height: bounds.height - 16), xRadius: 1.5, yRadius: 1.5).fill()
        }
        let icon: String, color: NSColor
        if isAll { icon = "list.bullet"; color = Theme.accent }
        else if pinned { icon = "exclamationmark.triangle"; color = Theme.orange }
        else if tunnelOnly || !decrypted { icon = "lock.open"; color = Theme.text3 }
        else { icon = "lock"; color = Theme.green }
        drawSymbol(icon, in: NSRect(x: 18, y: 0, width: 18, height: bounds.height), color: color, size: 12, weight: .medium)

        let a = Theme.attrs(isSelected ? .bodyMedium : .body, isSelected ? Theme.accent : Theme.text)
        let cnt = Theme.attrs(.mono, Theme.text3)
        let cs = (String(count) as NSString)
        let csz = cs.size(withAttributes: cnt)
        let rightEdge = bounds.width - 20
        cs.draw(at: NSPoint(x: rightEdge - csz.width, y: bounds.midY - csz.height / 2), withAttributes: cnt)
        var limit = rightEdge - csz.width - 8
        if errors {
            Theme.red.setFill()
            NSBezierPath(ovalIn: NSRect(x: limit - 7, y: bounds.midY - 3, width: 6, height: 6)).fill()
            limit -= 14
        }
        let p = NSMutableParagraphStyle(); p.lineBreakMode = .byTruncatingTail
        var b = a; b[.paragraphStyle] = p
        let sz = (title as NSString).size(withAttributes: a)
        (title as NSString).draw(in: NSRect(x: 44, y: bounds.midY - sz.height / 2, width: max(limit - 44, 20), height: sz.height), withAttributes: b)
    }
}

@MainActor
final class Sidebar: NSView, NSTableViewDataSource, NSTableViewDelegate {
    let model: FlowModel
    private let table = NSTableView()
    private let scroll = NSScrollView()
    private let hostsLabel = label("HOSTS", .overline, Theme.text3)
    private let hostsCount = label("", .mono, Theme.text3)
    let theme = Segmented(["Auto", "Light", "Dark"])
    private let recDot = Surface()
    private let recText = label("Recording", .callout, Theme.text)
    private let addrText = label("", .monoSmall, Theme.text2)
    private let certText = label("Certificate", .callout, Theme.text2)
    private let certIcon = SymbolView("shield", Theme.text3)
    var onSelectHost: ((String?) -> Void)?
    override var isFlipped: Bool { true }
    override var mouseDownCanMoveWindow: Bool { true }

    init(model: FlowModel) {
        self.model = model
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false

        let col = NSTableColumn(identifier: .init("h"))
        col.resizingMask = .autoresizingMask
        table.addTableColumn(col)
        table.headerView = nil
        table.dataSource = self
        table.delegate = self
        table.rowHeight = 34
        table.intercellSpacing = .zero
        table.backgroundColor = .clear
        table.style = .plain
        table.focusRingType = .none
        scroll.documentView = table
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        scroll.translatesAutoresizingMaskIntoConstraints = false

        theme.equalWidths = true
        theme.role = .bodyMedium
        theme.padding = 8

        recDot.radius = 4
        recDot.translatesAutoresizingMaskIntoConstraints = false
        let recRow = NSStackView.horizontal([recDot, recText], spacing: 9)
        let addrIcon = SymbolView("globe", Theme.text3)
        let addrRow = NSStackView.horizontal([addrIcon, addrText], spacing: 9)
        let certRow = NSStackView.horizontal([certIcon, certText], spacing: 9)
        for v in [recDot] { v.widthAnchor.constraint(equalToConstant: 8).isActive = true; v.heightAnchor.constraint(equalToConstant: 8).isActive = true }
        let status = NSStackView.vertical([recRow, addrRow, certRow], spacing: 9)
        let sep = Divider()

        for v in [hostsLabel, hostsCount, scroll, theme, sep, status] { addSubview(v) }
        NSLayoutConstraint.activate([
            hostsLabel.topAnchor.constraint(equalTo: topAnchor, constant: 16),
            hostsLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 20),
            hostsCount.centerYAnchor.constraint(equalTo: hostsLabel.centerYAnchor),
            hostsCount.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -20),
            scroll.topAnchor.constraint(equalTo: hostsLabel.bottomAnchor, constant: 10),
            scroll.leadingAnchor.constraint(equalTo: leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: trailingAnchor),
            scroll.bottomAnchor.constraint(equalTo: theme.topAnchor, constant: -12),
            theme.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            theme.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            theme.heightAnchor.constraint(equalToConstant: 30),
            theme.bottomAnchor.constraint(equalTo: sep.topAnchor, constant: -12),
            sep.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 20),
            sep.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -20),
            sep.bottomAnchor.constraint(equalTo: status.topAnchor, constant: -12),
            status.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 20),
            status.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -12),
            status.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -16),
        ])
        refresh()
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) { Theme.sidebar.setFill(); bounds.fill() }

    func refresh() {
        let sel = model.host
        table.reloadData()
        let row = sel.flatMap { h in model.hosts.firstIndex { $0.host == h } }.map { $0 + 1 } ?? 0
        table.selectRowIndexes([row], byExtendingSelection: false)
        hostsCount.set(String(model.hosts.count), .mono, Theme.text3)
        recDot.fill = model.recording ? Theme.green : Theme.orange
        recText.set(model.recording ? "Recording" : "Paused", .callout, Theme.text)
        addrText.set(model.listen.isEmpty ? "—" : model.listen, .monoSmall, Theme.text2)
        switch model.caTrusted {
        case false?:
            certText.set("Certificate not trusted", .callout, Theme.text2)
            certIcon.set("exclamationmark.triangle", Theme.orange)
        default:
            certText.set("Certificate", .callout, Theme.text2)
            certIcon.set("shield", model.caTrusted == true ? Theme.green : Theme.text3)
        }
    }

    func numberOfRows(in tableView: NSTableView) -> Int { model.hosts.count + 1 }

    func tableView(_ tableView: NSTableView, rowViewForRow row: Int) -> NSTableRowView? {
        let v = HostRowView()
        if row == 0 {
            v.title = "All traffic"; v.isAll = true; v.count = model.total
        } else {
            let h = model.hosts[row - 1]
            v.title = h.host; v.count = h.flows; v.errors = h.errors > 0; v.pinned = h.pinned
            v.decrypted = h.decrypted; v.tunnelOnly = h.tunnelOnly
        }
        return v
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? { nil }

    func tableViewSelectionDidChange(_ notification: Notification) {
        let r = table.selectedRow
        guard r >= 0 else { return }
        let host = r == 0 ? nil : model.hosts[r - 1].host
        if host != model.host { onSelectHost?(host) }
    }
}

/// A tinted SF Symbol as a view, so its color follows the appearance.
final class SymbolView: NSView {
    private var name: String
    private var color: NSColor
    private let size: CGFloat
    override var isFlipped: Bool { true }

    init(_ name: String, _ color: NSColor, size: CGFloat = 12) {
        self.name = name; self.color = color; self.size = size
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        widthAnchor.constraint(equalToConstant: 16).isActive = true
        heightAnchor.constraint(equalToConstant: 16).isActive = true
    }
    required init?(coder: NSCoder) { fatalError() }

    func set(_ name: String, _ color: NSColor) { self.name = name; self.color = color; needsDisplay = true }
    override func draw(_ dirtyRect: NSRect) { drawSymbol(name, in: bounds, color: color, size: size, weight: .medium) }
}
