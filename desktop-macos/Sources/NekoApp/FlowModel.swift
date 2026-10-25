import Foundation
import NekoAPI

/// View state for the flow list. All parsing, decoding and search happens in the
/// core; this only mirrors what the core sends and applies its batches.
@MainActor
final class FlowModel {
    let composer = ComposerModel()
    private(set) var flows: [Flow] = []
    private var rowByID: [UInt64: Int] = [:]
    private(set) var total: UInt64 = 0
    private(set) var recording = true
    private(set) var listen = ""

    var kind: Neko_Api_V1_Filter.Kind = .all
    var text = ""
    /// Exact host the sidebar is focused on; nil means all traffic.
    var host: String?
    /// SDK device ids picked in the chip row; empty means all devices.
    var deviceIDs: [String] = []

    private(set) var hosts: [Neko_Api_V1_HostSummary] = []
    private(set) var devices: [Neko_Api_V1_Device] = []
    private(set) var pending: [Neko_Api_V1_PendingPrompt] = []
    private(set) var ca: Neko_Api_V1_CaInfo?
    private(set) var caTrusted: Bool?
    let admin = AdminModel()
    var paused: Int { admin.paused.count }
    /// Where detail comes from. Screenshots swap this for fixtures.
    var detailProvider: ((UInt64) async throws -> FlowDetail)?
    private var lastHostsFetch = Date.distantPast

    var onChange: (() -> Void)?
    var onSidebar: (() -> Void)?
    var onStatus: (() -> Void)?
    var onError: ((String) -> Void)?

    private(set) var client: APIClient?
    private let core = CoreProcess()
    private var reloading = false
    private var held: [Neko_Api_V1_FlowBatch] = []
    private var generation = 0
    private var notifyScheduled = false
    private var searchTask: Task<Void, Never>?

    /// How many of the newest matching flows to keep in memory.
    static let window: UInt32 = 5000

    func start() async {
        do {
            let c = try await core.connect()
            c.onPush = { [weak self] m in Task { @MainActor in self?.handlePush(m) } }
            c.onClose = { [weak self] in
                Task { @MainActor in self?.onError?("The connection to the core closed.") }
            }
            client = c
            let st = try await c.request { $0.status = .init() }
            if case .status(let s)? = st.body {
                listen = s.listen
                recording = s.recording
            }
            detailProvider = { [weak c] id in
                guard let c else { throw APIError.closed }
                let r = try await c.request { $0.getFlow = .with { $0.id = id; $0.parts = 7 } }
                guard case .flow(let d)? = r.body else { throw APIError.unexpected }
                return d
            }
            await admin.attach(c)
            await composer.attach(c)
            try await resubscribe()
            await refreshHosts(force: true)
            await loadCA()
            _ = try? await c.request { $0.devicesSubscribe = .init() }.body.map(handleDevices)
        } catch {
            onError?(error.localizedDescription)
        }
    }

    private func handleDevices(_ b: ServerMessage.OneOf_Body) {
        if case .devices(let d) = b {
            devices = d.devices
            pending = d.pending
            onSidebar?()
        }
    }

    func refreshHosts(force: Bool = false) async {
        guard let c = client, force || Date().timeIntervalSince(lastHostsFetch) > 1 else { return }
        lastHostsFetch = Date()
        if let r = try? await c.request({ $0.hosts = .init() }), case .hosts(let h)? = r.body {
            hosts = h.hosts
            onSidebar?()
        }
    }

    func loadCA() async {
        guard let c = client else { return }
        if let r = try? await c.request({ $0.caInfo = .init() }), case .caInfo(let i)? = r.body {
            ca = i
            onSidebar?()
            let path = i.certPath
            let ok = await Task.detached { Self.verifyTrusted(path) }.value
            caTrusted = ok
            onSidebar?()
        }
    }

    /// True when the system trust settings accept the CA for SSL.
    nonisolated static func verifyTrusted(_ path: String) -> Bool {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/security")
        p.arguments = ["verify-cert", "-c", path, "-p", "ssl"]
        p.standardOutput = FileHandle.nullDevice
        p.standardError = FileHandle.nullDevice
        do { try p.run() } catch { return false }
        p.waitUntilExit()
        return p.terminationStatus == 0
    }

    func exportHAR() async -> (Data, String)? {
        guard let c = client else { return nil }
        let f = filter
        guard let r = try? await c.request({ $0.export = .with { $0.kind = "har"; $0.filter = f; $0.redact = true } }),
              case .export(let e)? = r.body else { return nil }
        return (e.data, e.filename.isEmpty ? "flows.har" : e.filename)
    }

    func replay(_ id: UInt64) async {
        _ = try? await client?.request { $0.replay = .with { $0.id = id } }
    }

    func stop() {
        client?.close()
        core.stop()
    }

    private var filter: Neko_Api_V1_Filter {
        .with {
            $0.kind = kind
            $0.text = text
            if let host { $0.hosts = [host] }
            $0.devices = deviceIDs
        }
    }

    func filterChanged() {
        searchTask?.cancel()
        searchTask = Task {
            try? await Task.sleep(nanoseconds: 150_000_000)
            if Task.isCancelled { return }
            do { try await resubscribe() } catch { onError?(error.localizedDescription) }
        }
    }

    private func resubscribe() async throws {
        guard let c = client else { return }
        let f = filter
        // Subscribe first so nothing slips between the subscription and the reload.
        let r = try await c.request { $0.subscribe = .with { $0.filter = f } }
        if case .subscribed(let s)? = r.body { total = s.total }
        try await reload()
    }

    private func reload() async throws {
        guard let c = client else { return }
        generation += 1
        let mine = generation
        reloading = true
        held = []
        defer { if mine == generation { reloading = false } }
        let f = filter
        let head = try await c.request { $0.search = .with { $0.filter = f; $0.limit = 1 } }
        guard case .search(let s)? = head.body else { return }
        let matching = s.total
        var offset = matching > UInt64(Self.window) ? UInt32(matching) - Self.window : 0
        var items: [Flow] = []
        while UInt64(offset) < matching {
            let o = offset
            let r = try await c.request { $0.search = .with { $0.filter = f; $0.offset = o; $0.limit = 1000 } }
            guard case .search(let page)? = r.body, !page.items.isEmpty else { break }
            items += page.items
            offset += UInt32(page.items.count)
        }
        guard mine == generation else { return }  // a newer reload superseded this one
        flows = items
        reindex()
        for b in held { apply(b) }
        held = []
        reloading = false
        scheduleNotify()
    }

    func setRecording(_ on: Bool) async {
        guard let c = client else { return }
        _ = try? await c.request { $0.capture = .with { $0.action = on ? .start : .stop } }
        recording = on
        onStatus?()
    }

    func clear() async {
        guard let c = client else { return }
        _ = try? await c.request { $0.clear = .init() }
    }

    // MARK: pushes

    private func handlePush(_ m: ServerMessage) {
        switch m.body {
        case .batch(let b)?:
            if reloading { held.append(b) } else { apply(b) }
            scheduleNotify()
        case .resync?:
            Task { try? await reload() }
        case .devices(let d)?:
            devices = d.devices
            pending = d.pending
            onSidebar?()
        case .pauseNotice?, .approvalNotice?:
            admin.handlePush(m)
        default:
            break
        }
    }

    private func apply(_ b: Neko_Api_V1_FlowBatch) {
        if b.cleared {
            flows = []
            rowByID = [:]
        }
        var needsReindex = false
        for u in b.upserts {
            if let i = rowByID[u.id] {
                flows[i] = u
            } else if let last = flows.last, u.id < last.id {
                let at = flows.firstIndex { $0.id > u.id } ?? flows.count
                flows.insert(u, at: at)
                needsReindex = true
            } else {
                rowByID[u.id] = flows.count
                flows.append(u)
            }
        }
        if !b.removed.isEmpty {
            let gone = Set(b.removed)
            flows.removeAll { gone.contains($0.id) }
            needsReindex = true
        }
        if flows.count > Int(Self.window) * 2 {
            flows.removeFirst(flows.count - Int(Self.window))
            needsReindex = true
        }
        if needsReindex { reindex() }
        total = b.total
        Task { await refreshHosts() }
    }

    private func reindex() {
        rowByID = Dictionary(uniqueKeysWithValues: flows.enumerated().map { ($1.id, $0) })
    }

    /// Batches arrive every 16 ms; redraw at most about 20 times a second.
    private func scheduleNotify() {
        guard !notifyScheduled else { return }
        notifyScheduled = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.notifyScheduled = false
                self.onChange?()
                self.onStatus?()
            }
        }
    }

    func row(forID id: UInt64) -> Int? { rowByID[id] }

    /// Replaces the device list, for screenshots.
    func setDevices(_ d: [Device], pending p: [PendingPrompt]) { devices = d; pending = p }

    /// Fills the model without a core, for screenshots.
    func loadFixture(flows: [Flow], hosts: [Neko_Api_V1_HostSummary], devices: [Neko_Api_V1_Device],
                     pending: Int, detail: @escaping (UInt64) async throws -> FlowDetail) {
        self.flows = flows
        reindex()
        total = UInt64(flows.count)
        self.hosts = hosts
        self.devices = devices
        self.pending = (0..<pending).map { _ in Neko_Api_V1_PendingPrompt() }
        listen = "127.0.0.1:8899"
        recording = true
        caTrusted = false
        ca = .with {
            $0.subject = "Neko Local CA (workstation)"
            $0.notAfter = "2036-10-10"
            $0.sha256 = "9f2c1b7e5a044d1e8a526e1f0c0a77d1aa"
            $0.certPath = "/Users/you/Library/Application Support/neko/ca.pem"
            $0.keyStorage = "keyring"
        }
        detailProvider = detail
    }
}
