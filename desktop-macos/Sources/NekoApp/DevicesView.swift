import AppKit
import NekoAPI

/// SDK pairing and paired devices.
@MainActor
final class DevicesView: Page {
    let model: FlowModel
    private var hintsOpen = false
    private var drawer: DeviceForm?
    private var drawerError: String?
    private var admin: AdminModel { model.admin }

    init(model: FlowModel) {
        self.model = model
        super.init(maxWidth: 980)
    }
    required init?(coder: NSCoder) { fatalError() }

    private static let cols: [(String, CGFloat)] = [("Device", 200), ("App", 190), ("SDK", 80), ("Connection", 100), ("Status", 110), ("Last seen", 90)]

    func reload() {
        if drawer != nil { return }  // keep typing undisturbed; Save and Close rebuild
        render()
    }

    private func render() {
        let sdk = admin.sdk
        var views: [NSView] = []

        let title = label("Devices", .largeTitle, Theme.text)
        let accept = label("Accept SDK connections", .bodyMedium, Theme.text)
        let master = Switch(on: sdk?.enabled ?? false)
        master.onToggle = { [weak self] on in Task { await self?.admin.setSdk(enabled: on) } }
        views.append(NSStackView.horizontal([title, NSView(), accept, master], spacing: 10))

        let auto = Switch(on: sdk?.autoAcceptLocal ?? true)
        auto.onToggle = { [weak self] on in Task { await self?.admin.setSdk(autoAccept: on) } }
        let wifi = Switch(on: sdk?.lan ?? false)
        wifi.onToggle = { [weak self] on in Task { await self?.admin.setSdk(lan: on) } }
        views.append(NSStackView.horizontal([auto, label("Accept emulators, simulators and USB phones automatically", .callout, Theme.text),
                                             wifi, label("Phones on Wi-Fi", .callout, Theme.text), NSView()], spacing: 10))

        if let sdk {
            var meta: [NSView] = []
            let listening = sdk.listening.first.map { "Listening on \($0)" } ?? (sdk.enabled ? "Starting…" : "Off")
            let fp = sdk.fingerprint.isEmpty ? "" : "  ·  certificate \(sdk.fingerprint.prefix(12))…"
            meta.append(label(listening + fp, .callout, Theme.text2))
            if !sdk.adbLabel.isEmpty { meta.append(Badge(sdk.adbLabel, color: sdk.adbState == "ready" ? Theme.green : Theme.text2)) }
            if sdk.discoveryState == "advertising" { meta.append(Badge("Wi-Fi discovery on", color: Theme.green)) }
            meta.append(NSView())
            views.append(NSStackView.horizontal(meta, spacing: 8))
        }

        for p in model.pending { views.append(pendingCard(p)) }

        if drawer != nil { views.append(drawerCard()) }
        views.append(label("Paired devices", .headline, Theme.text))
        views.append(devicesTable())
        views.append(hints(sdk))
        setContent(views)
        column.setCustomSpacing(10, after: views[1])
        column.setCustomSpacing(20, after: views[min(2, views.count - 1)])
    }

    // MARK: pairing

    private func pendingCard(_ p: PendingPrompt) -> NSView {
        let who = [p.deviceName, p.deviceModel].filter { !$0.isEmpty }.joined(separator: "  ·  ")
        let head = NSStackView.horizontal([Badge(p.repair ? "Re-pair request" : "Pairing request", color: Theme.accent),
                                           label(who, .bodyMedium, Theme.text)], spacing: 10)
        let detail = [p.appName, "\(p.sdkName) \(p.sdkVersion)", p.connection, p.osVersion].filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }.joined(separator: "  ·  ")
        let code = String(p.code.prefix(3)) + "  " + String(p.code.dropFirst(3))
        let codeLabel = NSTextField(labelWithAttributedString: NSAttributedString(string: code, attributes: [
            .font: Theme.monoMedium(30), .foregroundColor: Theme.text, .kern: 1.0,
        ]))
        codeLabel.translatesAutoresizingMaskIntoConstraints = false
        let allow = Chip("Allow", symbol: "checkmark", style: .accent)
        allow.onClick = { [weak self] in Task { await self?.admin.deviceAction(.allow, pendingID: p.id) } }
        let deny = Chip("Deny")
        deny.onClick = { [weak self] in Task { await self?.admin.deviceAction(.deny, pendingID: p.id) } }
        let left = NSStackView.vertical([head, label(detail, .callout, Theme.text2),
                                         NSStackView.horizontal([allow, deny, label(expiryText(p), .callout, Theme.text2)], spacing: 10)], spacing: 8)
        let row = NSStackView.horizontal([left, NSView(), codeLabel], spacing: 12)
        return groupCard([row], border: Theme.accent, padding: NSEdgeInsets(top: 16, left: 16, bottom: 16, right: 16))
    }

    private func expiryText(_ p: PendingPrompt) -> String {
        let left = max(0, Int((Double(p.expiresMs) - Date().timeIntervalSince1970 * 1000) / 1000))
        return "Confirm the same code appears in the app's log or banner. Expires in \(left) s."
    }

    // MARK: table

    private func cell(_ v: NSView, _ w: CGFloat) -> NSView {
        v.widthAnchor.constraint(equalToConstant: w).isActive = true
        return v
    }

    private func devicesTable() -> NSView {
        let hdr = NSStackView.horizontal(Self.cols.map { cell(label($0.0, .caption, Theme.text3), $0.1) } + [NSView(), cell({ let l = label("Flows", .caption, Theme.text3); l.alignment = .right; return l }(), 64)], spacing: 10)
        var rows: [NSView] = [hdr]
        if model.devices.isEmpty {
            rows.append(label("No devices have paired yet. Add the SDK to a debug build and run it.", .callout, Theme.text2))
        }
        for d in model.devices { rows.append(deviceRow(d)) }
        return groupCard(rows, padding: NSEdgeInsets(top: 12, left: 16, bottom: 12, right: 16))
    }

    private func deviceRow(_ d: Device) -> NSView {
        func two(_ a: String, _ b: String) -> NSView {
            NSStackView.vertical([label(a, .bodyMedium, Theme.text), label(b, .caption, Theme.text3)], spacing: 3)
        }
        let model_ = [d.deviceModel, d.osVersion].filter { !$0.isEmpty }.joined(separator: " · ")
        let app = [d.appID, d.appVersion].filter { !$0.isEmpty }.joined(separator: " · ")
        let sdk = two(d.sdkName, d.sdkVersion)
        let (stText, stColor): (String, NSColor) = {
            switch d.status {
            case .deviceConnected: return ("Connected", Theme.green)
            case .deviceDenied: return ("Denied", Theme.red)
            case .deviceRevoked: return ("Revoked", Theme.orange)
            default: return ("Idle", Theme.text3)
            }
        }()
        let dot = Surface(); dot.fill = stColor; dot.radius = 4
        dot.translatesAutoresizingMaskIntoConstraints = false
        dot.widthAnchor.constraint(equalToConstant: 8).isActive = true
        dot.heightAnchor.constraint(equalToConstant: 8).isActive = true
        let status = NSStackView.horizontal([dot, label(stText, .body, Theme.text)], spacing: 8)
        let conn = ["wifi": "Wi-Fi", "usb": "USB", "emulator": "Emulator", "simulator": "Simulator"][d.connection] ?? d.connection
        let flows = label(NumberFormatter.localizedString(from: NSNumber(value: d.flows), number: .decimal), .mono, Theme.text)
        flows.alignment = .right
        var action: NSView
        if d.status == .deviceDenied || d.status == .deviceRevoked {
            let t = IconButton("trash", tip: "Forget this device")
            t.onClick = { [weak self] in Task { await self?.admin.deviceAction(.forget, deviceID: d.deviceID) } }
            action = t
        } else {
            let r = Chip("Revoke", style: .danger)
            r.onClick = { [weak self] in Task { await self?.admin.deviceAction(.revoke, deviceID: d.deviceID, appID: d.appID) } }
            let cfg = IconButton("gearshape", tip: "Settings for this app")
            cfg.onClick = { [weak self] in self?.drawer = DeviceForm(d); self?.drawerError = nil; self?.render() }
            action = NSStackView.horizontal([cfg, r], spacing: 6)
        }
        return NSStackView.horizontal([
            cell(two(d.deviceName.isEmpty ? d.deviceID : d.deviceName, model_), Self.cols[0].1),
            cell(two(d.appName, app), Self.cols[1].1),
            cell(sdk, Self.cols[2].1),
            cell(label(conn, .body, Theme.text), Self.cols[3].1),
            cell(status, Self.cols[4].1),
            cell(label(lastSeen(d), .callout, Theme.text2), Self.cols[5].1),
            NSView(), cell(flows, 64), action,
        ], spacing: 10)
    }

    private func lastSeen(_ d: Device) -> String {
        if d.status == .deviceConnected { return "now" }
        let s = Int(Date().timeIntervalSince1970 - Double(d.lastSeenMs) / 1000)
        if d.lastSeenMs == 0 || s < 0 { return "—" }
        if s < 90 { return "now" }
        if s < 3600 { return "\(s / 60) min ago" }
        if s < 86400 { return "\(s / 3600) h ago" }
        return "\(s / 86400) d ago"
    }

    // MARK: hints

    private func hints(_ sdk: SdkStatus?) -> NSView {
        let dh = DisclosureHeader("Connection hints", count: sdk?.checks.count ?? 0, open: hintsOpen)
        dh.onToggle = { [weak self] in self?.hintsOpen.toggle(); self?.render() }
        guard hintsOpen, let sdk, !sdk.checks.isEmpty else { return dh }
        var rows: [NSView] = []
        for c in sdk.checks {
            let color: NSColor = c.level == "ok" ? Theme.green : c.level == "error" ? Theme.red : c.level == "warn" ? Theme.orange : Theme.text3
            let dot = Surface(); dot.fill = color; dot.radius = 4
            dot.translatesAutoresizingMaskIntoConstraints = false
            dot.widthAnchor.constraint(equalToConstant: 8).isActive = true
            dot.heightAnchor.constraint(equalToConstant: 8).isActive = true
            var lines: [NSView] = [label(c.title, .bodyMedium, Theme.text)]
            if !c.detail.isEmpty { lines.append(label(c.detail, .callout, Theme.text2, lines: 0)) }
            if !c.fix.isEmpty { lines.append(label(c.fix, .mono, Theme.text2, lines: 0)) }
            let text = NSStackView.vertical(lines, spacing: 3)
            rows.append(NSStackView.horizontal([dot, text], spacing: 10, alignment: .top))
        }
        let box = NSStackView.vertical([dh, groupCard(rows)], spacing: 8)
        return box
    }

    // MARK: settings drawer

    private func drawerCard() -> NSView {
        guard let f = drawer else { return NSView() }
        func bound(_ kp: WritableKeyPath<DeviceForm, String>, _ ph: String, width: CGFloat) -> Field {
            let fld = Field(drawer?[keyPath: kp] ?? "", placeholder: ph, width: width)
            fld.onChange = { [weak self] v in self?.drawer?[keyPath: kp] = v }
            return fld
        }
        func area(_ kp: WritableKeyPath<DeviceForm, String>, _ rows: Int, _ ph: String) -> Area {
            let a = Area(drawer?[keyPath: kp] ?? "", rows: rows, placeholder: ph)
            a.onChange = { [weak self] v in self?.drawer?[keyPath: kp] = v }
            return a
        }
        var rows: [NSView] = [label("Settings for \(f.title)", .headline, Theme.text)]
        let sw = Switch(on: f.capture)
        sw.onToggle = { [weak self] on in self?.drawer?.capture = on }
        rows.append(NSStackView.horizontal([sw, label("Capture this app's calls", .body, Theme.text), NSView()], spacing: 10))
        let lists = NSStackView.horizontal([
            labelled("Only these hosts (empty = all)", area(\.allow, 3, "api.example.com\n*.cdn.example.com")),
            labelled("Never capture these hosts", area(\.deny, 3, "ads.example.com")),
        ], spacing: 12)
        lists.distribution = .fillEqually
        rows.append(lists)
        rows.append(NSStackView.horizontal([
            label("Body limit", .body, Theme.text), bound(\.bodyCapKB, "1024", width: 84), label("KB", .callout, Theme.text2),
            label("Send bodies up to", .body, Theme.text), bound(\.eagerKB, "64", width: 84),
            label("KB right away; larger ones load when opened", .callout, Theme.text2), NSView(),
        ], spacing: 8))
        let sel: Int = { switch f.redaction { case .redactStrict: return 1; case .redactNone: return 2; default: return 0 } }()
        let seg = Segmented(["Default", "Strict", "None"], selected: sel)
        seg.onChange = { [weak self] i in
            self?.drawer?.redaction = [.redactDefault, .redactStrict, .redactNone][min(i, 2)]
            self?.render()
        }
        rows.append(labelled("Redaction", seg, fill: false))
        let note: String = {
            switch f.redaction {
            case .redactStrict: return "Strict: only a short list of headers is sent, and no bodies."
            case .redactNone: return "None: nothing is hidden."
            default: return "Default: Authorization, cookies, API keys, and passwords or tokens in JSON are replaced on the device before they are sent."
            }
        }()
        rows.append(label(note, .callout, Theme.text2, lines: 0))
        if f.revealsSecrets {
            rows.append(label("Tokens, passwords and cookies from this app will appear in the inspector, in exports and in anything an AI client reads.", .callout, Theme.orange, lines: 0))
        }
        rows.append(labelled("Also hide these keys (one per line)", area(\.extraKeys, 2, "ssn\nnational_id")))
        if let drawerError { rows.append(label(drawerError, .callout, Theme.red, lines: 2)) }
        let save = Chip("Save", symbol: "checkmark", style: .accent)
        save.onClick = { [weak self] in
            guard let self, let f = self.drawer else { return }
            Task {
                if let why = await self.admin.setDeviceConfig(f) { self.drawerError = why } else { self.drawer = nil; self.drawerError = nil }
                self.render()
            }
        }
        let close = Chip(f.dirty ? "Cancel" : "Close", style: .text)
        close.onClick = { [weak self] in self?.drawer = nil; self?.drawerError = nil; self?.render() }
        rows.append(NSStackView.horizontal([save, close, NSView()], spacing: 8))

        let card = Surface()
        card.fill = Theme.card; card.border = Theme.accent; card.radius = Theme.Radius.card
        card.translatesAutoresizingMaskIntoConstraints = false
        let st = NSStackView.vertical(rows, spacing: 10)
        card.addSubview(st)
        st.pin(to: card, insets: NSEdgeInsets(top: 16, left: 18, bottom: 16, right: 18))
        for r in rows where !(r is Segmented) { r.widthAnchor.constraint(equalTo: st.widthAnchor).isActive = true }
        return card
    }
}
