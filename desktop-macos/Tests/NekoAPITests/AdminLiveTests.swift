import Foundation
import Testing
@testable import NekoAPI

/// A running `nekod` in a temp dir, plus a local origin, for the rules, breakpoint
/// and approval flows the app's Rules, Breakpoints and Settings screens use.
final class Harness {
    let core: Process
    let client: APIClient
    let origin: LocalOrigin
    let listen: String
    let dir: String

    init() async throws {
        let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        guard let bin = ["release", "debug"].map({ root.appendingPathComponent("target/\($0)/nekod").path })
            .first(where: { FileManager.default.isExecutableFile(atPath: $0) })
        else { throw Skip() }
        dir = NSTemporaryDirectory() + "hx-\(getpid())-\(UInt32.random(in: 0...99999))/"
        try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        origin = try LocalOrigin()
        core = Process()
        core.executableURL = URL(fileURLWithPath: bin)
        core.arguments = ["run", "--socket", dir + "a.sock", "--data-dir", dir + "data", "--listen", "127.0.0.1:0",
                          "--mcp-listen", "127.0.0.1:0", "--no-sdk", "--key-storage", "file"]
        core.standardOutput = FileHandle.nullDevice
        core.standardError = FileHandle.nullDevice
        try core.run()
        var c: APIClient?
        for _ in 0..<100 {
            let x = APIClient(socketPath: dir + "a.sock")
            if (try? x.connect()) != nil { c = x; break }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        guard let c else { throw Skip() }
        client = c
        let st = try await c.request { $0.status = .init() }
        guard case .status(let s)? = st.body else { throw Skip() }
        listen = s.listen
        _ = try await c.request { $0.watch = .init() }
    }

    struct Skip: Error {}

    func stop() {
        core.terminate()
        origin.stop()
        try? FileManager.default.removeItem(atPath: dir)
    }

    /// Runs curl through the proxy; returns the body once it finishes.
    func fetch(_ path: String) -> Process {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/curl")
        p.arguments = ["-s", "-m", "20", "-x", "http://\(listen)", "-w", "|%{http_code}", "-o", dir + "out-\(path.hashValue)",
                       "http://127.0.0.1:\(origin.port)\(path)"]
        p.standardOutput = FileHandle.nullDevice
        try? p.run()
        return p
    }

    func body(_ path: String) -> String { (try? String(contentsOfFile: dir + "out-\(path.hashValue)", encoding: .utf8)) ?? "" }

    func rules() async throws -> Neko_Api_V1_RulesResponse {
        let r = try await client.request { $0.rules = .init() }
        guard case .rules(let x)? = r.body else { throw Skip() }
        return x
    }

    func paused() async throws -> [PausedFlow] {
        let r = try await client.request { $0.paused = .init() }
        guard case .paused(let x)? = r.body else { throw Skip() }
        return x.items
    }

    func waitFor(_ what: String, tries: Int = 100, _ ok: () async throws -> Bool) async throws {
        for _ in 0..<tries {
            if try await ok() { return }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        Issue.record("timed out waiting for \(what)")
    }
}

@Suite struct AdminLiveTests {
    @Test func mockRuleLifecycle() async throws {
        guard let h = try? await Harness() else { Issue.record("nekod is not built"); return }
        defer { h.stop() }

        var form = RuleForm(kind: .mock)
        form.host = "127.0.0.1"; form.port = String(h.origin.port); form.path = "/mocked"
        form.status = "503"; form.body = "down"; form.name = "Checkout is down"
        _ = try await h.client.request { $0.putRule = .with { $0.rule = try! form.toRule() } }
        let rules = try await h.rules()
        #expect(rules.rules.count == 1)
        let rule = try #require(rules.rules.first)
        #expect(!rule.id.isEmpty && rule.name == "Checkout is down" && describeAction(rule) == "Answer 503")

        var p = h.fetch("/mocked"); p.waitUntilExit()
        #expect(h.body("/mocked") == "down")

        // Disabled: the origin answers.
        _ = try await h.client.request { $0.setRuleEnabled = .with { $0.id = rule.id; $0.enabled = false } }
        try? FileManager.default.removeItem(atPath: h.dir + "out-\("/mocked".hashValue)")
        p = h.fetch("/mocked"); p.waitUntilExit()
        #expect(h.body("/mocked") == "ok")

        _ = try await h.client.request { $0.removeRule = .with { $0.id = rule.id } }
        #expect(try await h.rules().rules.isEmpty)
    }

    @Test func breakpointHoldEditResume() async throws {
        guard let h = try? await Harness() else { Issue.record("nekod is not built"); return }
        defer { h.stop() }

        var form = RuleForm(kind: .breakpoint)
        form.host = "127.0.0.1"; form.port = String(h.origin.port); form.path = "/held"; form.phase = 1
        _ = try await h.client.request { $0.putRule = .with { $0.rule = try! form.toRule() } }

        let curl = h.fetch("/held")
        var held: PausedFlow?
        try await h.waitFor("a held request") { held = try await h.paused().first; return held != nil }
        let p = try #require(held)
        #expect(p.phase == .phaseRequest && p.method == "GET" && p.path == "/held")

        // Edit the path and let it through; only the changed field is sent.
        var e = BreakpointEdit(p)
        e.path = "/edited"
        let req = e.resume(p)
        #expect(req.hasPath && !req.hasMethod && !req.replaceHeaders)
        _ = try await h.client.request { $0.resume = req }
        curl.waitUntilExit()
        #expect(try await h.paused().isEmpty)

        let r = try await h.client.request { $0.search = .init() }
        guard case .search(let s)? = r.body else { Issue.record("no search"); return }
        let flow = try #require(s.items.first { $0.path == "/edited" })
        #expect(flow.flags & 256 != 0, "the flow should be flagged as edited")

        // Abort releases without sending upstream.
        let second = h.fetch("/held")
        try await h.waitFor("a second held request") { try await h.paused().count == 1 }
        let id = try await h.paused()[0].flowID
        _ = try await h.client.request { $0.resume = .with { $0.flowID = id; $0.abort = true } }
        second.waitUntilExit()
        #expect(try await h.paused().isEmpty)
    }

    @Test func aiApprovalCreatesTheRuleOnlyWhenAllowed() async throws {
        guard let h = try? await Harness() else { Issue.record("nekod is not built"); return }
        defer { h.stop() }
        let m = try await h.client.request { $0.mcpInfo = .init() }
        guard case .mcpInfo(let info)? = m.body, info.enabled else { Issue.record("mcp off"); return }
        #expect(info.writes == .mcpAsk)

        func callMock(_ name: String) async throws -> String {
            var req = URLRequest(url: URL(string: info.url)!)
            req.httpMethod = "POST"
            req.setValue("application/json", forHTTPHeaderField: "Content-Type")
            req.setValue("application/json, text/event-stream", forHTTPHeaderField: "Accept")
            req.setValue("Bearer \(info.token)", forHTTPHeaderField: "Authorization")
            let body: [String: Any] = ["jsonrpc": "2.0", "id": 1, "method": "tools/call",
                                       "params": ["name": "create_mock", "arguments": ["host": "api.example.com", "path": "/\(name)", "status": 503]]]
            req.httpBody = try JSONSerialization.data(withJSONObject: body)
            let (d, _) = try await URLSession.shared.data(for: req)
            return String(decoding: d, as: UTF8.self)
        }

        // Allowed.
        async let allowed = callMock("a")
        var pending: Neko_Api_V1_Approval?
        try await h.waitFor("an approval") {
            let r = try await h.client.request { $0.approvals = .init() }
            if case .approvals(let a)? = r.body { pending = a.pending.first }
            return pending != nil
        }
        let a = try #require(pending)
        #expect(a.tool == "create_mock" && a.detail.contains("api.example.com"))
        #expect(try await h.rules().rules.isEmpty, "nothing changes before the person decides")
        _ = try await h.client.request { $0.decideApproval = .with { $0.id = a.id; $0.allow = true } }
        let okText = try await allowed
        #expect(okText.contains("error") == false || okText.contains("\"isError\":false"))
        #expect(try await h.rules().rules.count == 1)

        // Denied.
        async let denied = callMock("b")
        pending = nil
        try await h.waitFor("a second approval") {
            let r = try await h.client.request { $0.approvals = .init() }
            if case .approvals(let x)? = r.body { pending = x.pending.first }
            return pending != nil
        }
        _ = try await h.client.request { $0.decideApproval = .with { $0.id = pending!.id; $0.allow = false } }
        let deniedText = try await denied
        #expect(deniedText.contains("declined"))
        #expect(try await h.rules().rules.count == 1)
    }
}
