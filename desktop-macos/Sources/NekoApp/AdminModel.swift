import Foundation
import NekoAPI

typealias Approval = Neko_Api_V1_Approval
typealias Device = Neko_Api_V1_Device
typealias PendingPrompt = Neko_Api_V1_PendingPrompt
typealias SdkStatus = Neko_Api_V1_SdkStatus
typealias McpInfo = Neko_Api_V1_McpInfo

/// Rules, held messages, AI approvals and SDK state: everything the Breakpoints,
/// Rules, Devices and Settings screens read and change. Like the flow model it
/// mirrors what the core says and sends the person's choices back.
@MainActor
final class AdminModel {
    private(set) var rules = Neko_Api_V1_RulesResponse()
    private(set) var paused: [PausedFlow] = []
    private(set) var approvals: [Approval] = []
    private(set) var mcp: McpInfo?
    private(set) var sdk: SdkStatus?
    private var client: APIClient?

    var onChange: (() -> Void)?
    var onError: ((String) -> Void)?
    /// A rule form for the Rules screen to open (set by "New breakpoint" and friends).
    var pendingForm: RuleForm?

    func attach(_ c: APIClient) async {
        client = c
        _ = try? await c.request { $0.watch = .init() }
        await refreshAll()
    }

    func refreshAll() async {
        await refreshRules()
        await refreshPaused()
        await refreshApprovals()
        await refreshMcp()
        await refreshSdk()
    }

    /// The core's message for the last request that failed.
    private(set) var lastError: String?

    private func call(_ build: (inout ClientMessage) -> Void) async -> ServerMessage? {
        guard let c = client else { return nil }
        do {
            let r = try await c.request(build)
            lastError = nil
            return r
        } catch {
            lastError = error.localizedDescription
            return nil
        }
    }

    func refreshRules() async {
        if case .rules(let r)? = await call({ $0.rules = .init() })?.body { rules = r; onChange?() }
    }

    func refreshPaused() async {
        if case .paused(let p)? = await call({ $0.paused = .init() })?.body { paused = p.items; onChange?() }
    }

    func refreshApprovals() async {
        if case .approvals(let a)? = await call({ $0.approvals = .init() })?.body { approvals = a.pending; onChange?() }
    }

    func refreshMcp() async {
        if case .mcpInfo(let m)? = await call({ $0.mcpInfo = .init() })?.body { mcp = m; onChange?() }
    }

    func refreshSdk() async {
        if case .sdkStatus(let s)? = await call({ $0.sdkStatus = .init() })?.body { sdk = s; onChange?() }
    }

    // MARK: rules

    /// Nil on success, otherwise the reason the core gave.
    func put(_ rule: Rule) async -> String? {
        let r = await call { $0.putRule = .with { $0.rule = rule } }
        let why = r == nil ? (lastError ?? "The core rejected this rule.") : nil
        await refreshRules()
        return why
    }

    /// Nil on success, otherwise the reason the core gave.
    func putMapLocal(_ rule: Neko_Api_V1_MapLocalRule) async -> String? {
        let r = await call { $0.putMapLocal = .with { $0.rule = rule } }
        let why = r == nil ? (lastError ?? "The core rejected this rule.") : nil
        await refreshRules()
        return why
    }

    func setSsl(_ ssl: Neko_Api_V1_SslSettings) async -> String? {
        let r = await call { $0.setSsl = .with { $0.ssl = ssl } }
        let why = r == nil ? (lastError ?? "The core rejected these settings.") : nil
        await refreshRules()
        return why
    }

    func setEnabled(_ id: String, _ on: Bool) async {
        _ = await call { $0.setRuleEnabled = .with { $0.id = id; $0.enabled = on } }
        await refreshRules()
    }

    func remove(_ id: String) async {
        _ = await call { $0.removeRule = .with { $0.id = id } }
        await refreshRules()
    }

    // MARK: held messages

    func resume(_ req: Neko_Api_V1_ResumeRequest) async {
        _ = await call { $0.resume = req }
        paused.removeAll { $0.flowID == req.flowID }
        onChange?()
    }

    func abort(_ flowID: UInt64) async {
        await resume(.with { $0.flowID = flowID; $0.abort = true })
    }

    // MARK: AI approvals

    func decide(_ id: String, allow: Bool) async {
        _ = await call { $0.decideApproval = .with { $0.id = id; $0.allow = allow } }
        approvals.removeAll { $0.id == id }
        onChange?()
        await refreshRules()
    }

    func setMcpWrites(_ w: Neko_Api_V1_McpWrites) async {
        _ = await call { $0.setMcpWrites = .with { $0.writes = w } }
        await refreshMcp()
        await refreshRules()
    }

    // MARK: SDK and devices

    func setSdk(enabled: Bool? = nil, lan: Bool? = nil, autoAccept: Bool? = nil) async {
        let cur = sdk
        _ = await call {
            $0.sdkSetSettings = .with {
                $0.enabled = enabled ?? cur?.enabled ?? true
                $0.lan = lan ?? cur?.lan ?? false
                $0.autoAcceptLocal = autoAccept ?? cur?.autoAcceptLocal ?? true
            }
        }
        await refreshSdk()
    }

    func setDeviceConfig(_ f: DeviceForm) async -> String? {
        let r = await call { $0.setDeviceConfig = .with { $0.deviceID = f.deviceID; $0.appID = f.appID; $0.config = f.config } }
        return r == nil ? (lastError ?? "The core rejected these settings.") : nil
    }

    func deviceAction(_ kind: Neko_Api_V1_DeviceActionRequest.Kind, pendingID: UInt64 = 0, deviceID: String = "", appID: String = "") async {
        _ = await call {
            $0.deviceAction = .with { $0.kind = kind; $0.pendingID = pendingID; $0.deviceID = deviceID; $0.appID = appID }
        }
        await refreshSdk()
    }

    // MARK: pushes

    func handlePush(_ m: ServerMessage) {
        switch m.body {
        case .pauseNotice(let n)?:
            if n.hasPaused {
                if let i = paused.firstIndex(where: { $0.flowID == n.paused.flowID }) { paused[i] = n.paused } else { paused.append(n.paused) }
            }
            if n.releasedFlowID != 0 { paused.removeAll { $0.flowID == n.releasedFlowID } }
            onChange?()
        case .approvalNotice(let n)?:
            if n.hasRequested, !approvals.contains(where: { $0.id == n.requested.id }) { approvals.append(n.requested) }
            if !n.decidedID.isEmpty {
                approvals.removeAll { $0.id == n.decidedID }
                Task { await refreshRules() }
            }
            onChange?()
        default:
            break
        }
    }

    /// Fixtures for screenshots.
    func loadFixture(rules: Neko_Api_V1_RulesResponse, paused: [PausedFlow], approvals: [Approval], sdk: SdkStatus, mcp: McpInfo) {
        self.rules = rules; self.paused = paused; self.approvals = approvals; self.sdk = sdk; self.mcp = mcp
    }
}
