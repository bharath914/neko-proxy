import AppKit
import NekoAPI

private final class ThinSplit: NSSplitView {
    override var dividerColor: NSColor { Theme.separator }
    override var dividerThickness: CGFloat { 1 }
}

/// The whole window: top bar, host sidebar and the active screen.
@MainActor
final class RootViewController: NSViewController, NSSplitViewDelegate {
    let model: FlowModel
    let top = TopBar()
    let sidebar: Sidebar
    let list: FlowList
    let detail = DetailPane(kind: .request)
    let response = DetailPane(kind: .response)
    let chips = DeviceChips()
    let settings: SettingsView
    let breakpoints: BreakpointsView
    let rules: RulesView
    let sequence: SequenceView
    let devicesView: DevicesView
    let approval = ApprovalCard()
    private var inspector = FlippedView()
    private var screen = FlippedView()
    private var split = ThinSplit()
    private var bottom = ThinSplit()
    private let traffic = FlippedView()
    private var sidebarWidth: NSLayoutConstraint!
    private var chipsHeight: NSLayoutConstraint!
    private(set) var section: Section = .inspector
    private var detailTask: Task<Void, Never>?
    var onError: ((String) -> Void)?

    init(model: FlowModel) {
        self.model = model
        sidebar = Sidebar(model: model)
        list = FlowList(model: model)
        settings = SettingsView(model: model)
        breakpoints = BreakpointsView(admin: model.admin)
        rules = RulesView(admin: model.admin)
        sequence = SequenceView(model: model)
        devicesView = DevicesView(model: model)
        super.init(nibName: nil, bundle: nil)
    }
    required init?(coder: NSCoder) { fatalError() }

    override func loadView() {
        let root = Surface()
        root.fill = Theme.content
        view = root

        let bodyDivider = Divider(vertical: true)
        screen.translatesAutoresizingMaskIntoConstraints = false
        for v in [top, sidebar, bodyDivider, screen] { root.addSubview(v) }
        sidebarWidth = sidebar.widthAnchor.constraint(equalToConstant: 236)
        NSLayoutConstraint.activate([
            top.topAnchor.constraint(equalTo: root.topAnchor),
            top.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            top.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            sidebar.topAnchor.constraint(equalTo: top.bottomAnchor),
            sidebar.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            sidebar.bottomAnchor.constraint(equalTo: root.bottomAnchor),
            sidebarWidth,
            bodyDivider.leadingAnchor.constraint(equalTo: sidebar.trailingAnchor),
            bodyDivider.topAnchor.constraint(equalTo: top.bottomAnchor),
            bodyDivider.bottomAnchor.constraint(equalTo: root.bottomAnchor),
            screen.topAnchor.constraint(equalTo: top.bottomAnchor),
            screen.leadingAnchor.constraint(equalTo: bodyDivider.trailingAnchor),
            screen.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            screen.bottomAnchor.constraint(equalTo: root.bottomAnchor),
        ])

        // Inspector: device chips over the list and detail split.
        inspector.translatesAutoresizingMaskIntoConstraints = false
        chips.translatesAutoresizingMaskIntoConstraints = false
        // Traffic on top (list or sequence); request and response side by side below.
        split.isVertical = false
        split.dividerStyle = .thin
        split.delegate = self
        split.translatesAutoresizingMaskIntoConstraints = false
        bottom.isVertical = true
        bottom.dividerStyle = .thin
        bottom.delegate = self
        bottom.translatesAutoresizingMaskIntoConstraints = true
        traffic.translatesAutoresizingMaskIntoConstraints = true
        for v in [list, sequence] {
            v.translatesAutoresizingMaskIntoConstraints = false
            traffic.addSubview(v)
            v.pin(to: traffic)
        }
        sequence.isHidden = true
        detail.translatesAutoresizingMaskIntoConstraints = true
        response.translatesAutoresizingMaskIntoConstraints = true
        bottom.addArrangedSubview(detail)
        bottom.addArrangedSubview(response)
        split.addArrangedSubview(traffic)
        split.addArrangedSubview(bottom)
        inspector.addSubview(chips); inspector.addSubview(split)
        chipsHeight = chips.heightAnchor.constraint(equalToConstant: 0)
        NSLayoutConstraint.activate([
            chips.topAnchor.constraint(equalTo: inspector.topAnchor),
            chips.leadingAnchor.constraint(equalTo: inspector.leadingAnchor),
            chips.trailingAnchor.constraint(equalTo: inspector.trailingAnchor),
            chipsHeight,
            split.topAnchor.constraint(equalTo: chips.bottomAnchor),
            split.leadingAnchor.constraint(equalTo: inspector.leadingAnchor),
            split.trailingAnchor.constraint(equalTo: inspector.trailingAnchor),
            split.bottomAnchor.constraint(equalTo: inspector.bottomAnchor),
        ])
        root.addSubview(approval)
        NSLayoutConstraint.activate([
            approval.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -16),
            approval.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -16),
            approval.widthAnchor.constraint(equalToConstant: 440),
        ])
        wire()
        setSection(.inspector)
    }

    override func viewDidLayout() {
        super.viewDidLayout()
        // Below about 1040 pt the host list could become a rail; keep it fixed and let the split flex.
        sidebarWidth.constant = view.bounds.width < 1000 ? 200 : 236
    }

    func splitView(_ splitView: NSSplitView, constrainMinCoordinate p: CGFloat, ofSubviewAt i: Int) -> CGFloat {
        splitView === split ? 150 : 300
    }
    func splitView(_ splitView: NSSplitView, constrainMaxCoordinate p: CGFloat, ofSubviewAt i: Int) -> CGFloat {
        splitView === split ? splitView.bounds.height - 200 : splitView.bounds.width - 300
    }
    func splitView(_ splitView: NSSplitView, shouldAdjustSizeOfSubview view: NSView) -> Bool {
        splitView === split ? view === bottom : true
    }

    func setInitialSplit() {
        split.setPosition(max(split.bounds.height * 0.42, 150), ofDividerAt: 0)
        bottom.setPosition(bottom.bounds.width / 2, ofDividerAt: 0)
    }

    func selectTab(_ i: Int) { detail.selectTab(i); response.selectTab(i) }

    /// Traffic list or sequence in the top section.
    func showTraffic(sequence on: Bool) {
        list.isHidden = on
        sequence.isHidden = !on
        top.view.selected = on ? 1 : 0
        if on { sequence.reload() }
    }

    private func wire() {
        top.nav.onChange = { [weak self] i in self?.setSection(Section(rawValue: i) ?? .inspector) }
        top.record.onClick = { [weak self] in self?.toggleRecording() }
        top.clear.onClick = { [weak self] in self?.confirmClear() }
        top.filter.onChange = { [weak self] i in
            guard let self else { return }
            let kinds: [Neko_Api_V1_Filter.Kind] = [.all, .errors, .slow, .pinned]
            self.model.kind = kinds[i]
            self.model.filterChanged()
        }
        top.search.onChange = { [weak self] t in self?.model.text = t; self?.model.filterChanged() }
        top.export.onClick = { [weak self] in self?.exportHAR() }
        top.phone.onClick = { [weak self] in self?.setSection(.devices) }
        top.proxy.onClick = { [weak self] in self?.setSection(.settings) }
        sidebar.onSelectHost = { [weak self] h in
            self?.model.host = h
            self?.model.filterChanged()
        }
        chips.onPick = { [weak self] ids in
            self?.model.deviceIDs = ids
            self?.model.filterChanged()
            self?.refreshChips()
        }
        list.onSelect = { [weak self] f in self?.select(f) }
        detail.onCopyCurl = { [weak self] in self?.copy(self?.detail.detail?.curl) }
        detail.onCopyURL = { [weak self] in self?.copy(self?.detail.detail?.url) }
        detail.onReplay = { [weak self] in
            guard let id = self?.detail.detail?.summary.id else { return }
            Task { await self?.model.replay(id) }
        }
        detail.onCertSettings = { [weak self] in self?.setSection(.settings) }
        settings.onInstall = { [weak self] in self?.installCA() }
        settings.onShow = { NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: $0)]) }
        detail.onMock = { [weak self] in self?.newRule(.mock) }
        detail.onBreak = { [weak self] in self?.newRule(.breakpoint) }
        breakpoints.onNewBreakpoint = { [weak self] in self?.setSection(.rules); self?.rules.startNew(.breakpoint) }
        approval.onDecide = { [weak self] id, allow in Task { await self?.model.admin.decide(id, allow: allow) } }
        model.admin.onChange = { [weak self] in self?.adminChanged() }
        top.view.onChange = { [weak self] i in self?.showTraffic(sequence: i == 1) }
        sequence.onOpen = { [weak self] f in
            self?.list.select(id: f.id)
            self?.showTraffic(sequence: false)
        }
        model.onChange = { [weak self] in self?.list.reload(); self?.sequence.reload() }
        model.onStatus = { [weak self] in self?.statusChanged() }
        model.onSidebar = { [weak self] in self?.sidebarChanged() }
        model.onError = { [weak self] m in self?.onError?(m) }
    }

    func setSection(_ s: Section) {
        section = s
        top.setSection(s)
        screen.subviews.forEach { $0.removeFromSuperview() }
        let v: NSView
        switch s {
        case .inspector: v = inspector
        case .breakpoints: v = breakpoints; breakpoints.reload()
        case .rules: v = rules; rules.reload()
        case .devices:
            v = devicesView
            devicesView.reload()
            Task { await model.admin.refreshSdk(); devicesView.reload() }
        case .settings: v = settings
        }
        v.translatesAutoresizingMaskIntoConstraints = false
        screen.addSubview(v)
        v.pin(to: screen)
        if s == .inspector { DispatchQueue.main.async { self.setInitialSplit() } }
        if s == .settings { settings.rebuild(); Task { await model.loadCA(); await model.admin.refreshMcp() } }
    }

    private func select(_ f: Flow?) {
        detailTask?.cancel()
        guard let f, let provider = model.detailProvider else { detail.show(nil); response.show(nil); return }
        detailTask = Task {
            do {
                let d = try await provider(f.id)
                if !Task.isCancelled { detail.show(d); response.show(d) }
            } catch {
                if !Task.isCancelled { detail.show(nil); response.show(nil) }
            }
        }
    }

    private func statusChanged() {
        top.record.recording = model.recording
        top.clear.isEnabled = model.total > 0
        top.export.isEnabled = model.total > 0
        sidebar.refresh()
    }

    /// Opens the rule editor on the Rules screen, pre-filled from the selected request.
    private func newRule(_ kind: RuleKind) {
        let flow = detail.detail?.summary
        setSection(.rules)
        rules.startNew(kind, flow: flow)
    }

    private func adminChanged() {
        approval.update(model.admin.approvals)
        switch section {
        case .breakpoints: breakpoints.reload()
        case .rules: rules.reload()
        case .devices: devicesView.reload()
        case .settings: settings.rebuild()
        default: break
        }
        sidebarChanged()
    }

    private func sidebarChanged() {
        sidebar.refresh()
        if section == .devices { devicesView.reload() }
        top.phone.dot = model.pending.isEmpty ? nil : Theme.orange
        refreshChips()
        if section == .settings { settings.rebuild() }
        top.nav.titles = ["Inspector", model.paused > 0 ? "Breakpoints (\(model.paused))" : "Breakpoints", "Rules",
                          model.pending.isEmpty ? "Devices" : "Devices (\(model.pending.count))", "Settings"]
    }

    private func refreshChips() {
        chips.update(devices: model.devices, selected: model.deviceIDs)
        chipsHeight.constant = model.devices.isEmpty ? 0 : 42
        chips.isHidden = model.devices.isEmpty
    }

    // MARK: actions

    func toggleRecording() { Task { await model.setRecording(!model.recording) } }

    func confirmClear() {
        guard let w = view.window else { return }
        let a = NSAlert()
        a.messageText = "Clear all requests?"
        a.informativeText = "This removes every captured request and body from this computer. It can't be undone."
        a.addButton(withTitle: "Clear")
        a.addButton(withTitle: "Cancel")
        a.beginSheetModal(for: w) { [weak self] r in
            if r == .alertFirstButtonReturn { Task { await self?.model.clear() } }
        }
    }

    func exportHAR() {
        Task {
            guard let (data, name) = await model.exportHAR() else { return }
            let p = NSSavePanel()
            p.nameFieldStringValue = name
            p.allowedContentTypes = [.json]
            if await p.beginSheetModal(for: view.window!) == .OK, let url = p.url { try? data.write(to: url) }
        }
    }

    func copy(_ s: String?) {
        guard let s else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(s, forType: .string)
    }

    func copyCurl() { copy(detail.detail?.curl) }
    func copyURL() { copy(detail.detail?.url) }

    func focusSearch() { view.window?.makeFirstResponder(top.search.field) }

    /// Shows the exact command first and only runs it after the person confirms.
    func installCA() {
        guard let w = view.window else { return }
        let ca = model.ca?.certPath ?? CorePaths.caPath
        let args = ["add-trusted-cert", "-r", "trustRoot", "-k", CorePaths.home + "/Library/Keychains/login.keychain-db", ca]
        let a = NSAlert()
        a.messageText = "Trust the Neko CA certificate?"
        a.informativeText = "This lets the proxy decrypt HTTPS for apps that use the system trust store (Safari, Chrome, curl). It runs:\n\nsecurity \(args.map { $0.contains(" ") ? "\"\($0)\"" : $0 }.joined(separator: " "))\n\nmacOS will ask for your password."
        a.addButton(withTitle: "Install")
        a.addButton(withTitle: "Cancel")
        a.beginSheetModal(for: w) { [weak self] resp in
            guard resp == .alertFirstButtonReturn else { return }
            Task {
                let (ok, msg) = await Task.detached { Self.runSecurity(args) }.value
                await self?.model.loadCA()
                let r = NSAlert()
                r.messageText = ok ? "CA certificate trusted" : "Could not trust the certificate"
                r.informativeText = ok ? "HTTPS from system apps can now be inspected." : msg
                _ = await r.beginSheetModal(for: w)
            }
        }
    }

    nonisolated private static func runSecurity(_ args: [String]) -> (Bool, String) {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/security")
        p.arguments = args
        let err = Pipe()
        p.standardError = err
        do {
            try p.run()
            p.waitUntilExit()
            return (p.terminationStatus == 0, String(decoding: err.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self))
        } catch {
            return (false, error.localizedDescription)
        }
    }
}

enum Appearance {
    static var current: Int { UserDefaults.standard.integer(forKey: "appearance") }
    @MainActor static func set(_ i: Int) {
        UserDefaults.standard.set(i, forKey: "appearance")
        apply()
    }
    @MainActor static func apply() {
        switch current {
        case 1: NSApp.appearance = NSAppearance(named: .aqua)
        case 2: NSApp.appearance = NSAppearance(named: .darkAqua)
        default: NSApp.appearance = nil
        }
    }
}
