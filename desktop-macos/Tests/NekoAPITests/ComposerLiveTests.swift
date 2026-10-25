import Foundation
import Testing
@testable import NekoAPI
@testable import NekoApp

/// The composer model against a real `nekod`: URL sync, send, tests, snippets, Postman files.
@MainActor
@Suite struct ComposerLiveTests {
    @Test func sendsSyncsAndTestsAgainstARealCore() async throws {
        let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let nekod = ["release", "debug"].map { root.appendingPathComponent("target/\($0)/nekod").path }
            .first { FileManager.default.isExecutableFile(atPath: $0) }
        guard let nekod else { Issue.record("nekod is not built: run `cargo build -p neko-cli` first"); return }
        let tmp = NSTemporaryDirectory() + "pxc-\(getpid())/"
        try FileManager.default.createDirectory(atPath: tmp, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(atPath: tmp) }
        let sock = tmp + "a.sock"
        let origin = try LocalOrigin()
        defer { origin.stop() }

        let p = Process()
        p.executableURL = URL(fileURLWithPath: nekod)
        p.arguments = ["run", "--socket", sock, "--data-dir", tmp + "data", "--listen", "127.0.0.1:0",
                       "--no-mcp", "--no-sdk", "--key-storage", "file"]
        p.standardOutput = FileHandle.nullDevice
        p.standardError = FileHandle.nullDevice
        try p.run()
        defer { p.terminate() }
        var client: APIClient?
        for _ in 0..<100 {
            let c = APIClient(socketPath: sock)
            if (try? c.connect()) != nil { client = c; break }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        let c = try #require(client, "core never accepted a connection")

        let m = ComposerModel()
        await m.attach(c)

        // A folder with a variable and bearer auth, inherited by the request.
        #expect(await m.addCollection("Shop/Orders") == nil)
        #expect(await m.setConfig(.with {
            $0.name = "Shop"
            $0.auth = .with { $0.kind = .bearer; $0.token = "{{tok}}" }
            $0.vars = [.with { $0.name = "tok"; $0.value = "abc" }, .with { $0.name = "host"; $0.value = "http://127.0.0.1:\(origin.port)" }]
        }) == nil)
        #expect(m.tree().first?.folders.first?.path == "Shop/Orders")

        // Typing the URL fills the Params table; the params table rewrites the URL.
        m.edit { $0.collection = "Shop/Orders"; $0.method = "GET"; $0.auth = .inherit }
        m.urlEdited("{{host}}/items/:id?page=2&q=a%20b")
        for _ in 0..<50 where m.tab.draft.params.isEmpty { try await Task.sleep(nanoseconds: 50_000_000) }
        #expect(m.tab.draft.url == "{{host}}/items/:id")
        #expect(m.tab.draft.params.map(\.name) == ["page", "q"])
        #expect(m.tab.draft.params[1].value == "a b")
        #expect(m.tab.draft.pathVars.map(\.name) == ["id"])
        m.edit { $0.pathVars[0].value = "42" }
        m.paramsEdited([.with { $0.name = "page"; $0.value = "3" }])
        for _ in 0..<50 where !m.tab.display.hasSuffix("page=3") { try await Task.sleep(nanoseconds: 50_000_000) }
        #expect(m.tab.display == "{{host}}/items/:id?page=3")

        // Tests run against the stored response.
        m.edit {
            $0.tests = [
                .with { $0.kind = .status; $0.op = .equals; $0.expected = "2xx" },
                .with { $0.kind = .body; $0.op = .contains; $0.expected = "ok" },
                .with { $0.kind = .status; $0.op = .equals; $0.expected = "404" },
            ]
        }
        await m.send()
        let sent = try #require(m.tab.sent)
        #expect(sent.error.isEmpty, "\(sent.error)")
        #expect(sent.status == 200)
        #expect(sent.tests.map(\.passed) == [true, true, false])
        #expect(sent.url.contains("/items/42?page=3"))
        #expect(m.tab.response?.responseBody.text == "ok")

        // The inherited bearer token was sent.
        let resolved = try #require(await m.resolve())
        #expect(resolved.headers.contains { $0.name == "Authorization" && $0.value == "Bearer abc" })
        #expect(resolved.generated.contains("Authorization"))

        // Snippets and bulk edit come from the core.
        let langs = await m.codeLanguages()
        #expect(langs.count >= 10)
        let (code, err) = await m.code("python")
        #expect(err == nil && code.contains("Bearer abc"))
        let rows = await m.bulkParse("a: 1\n// b: 2")
        #expect(rows.count == 2 && rows[1].disabled)

        // Save, export as Postman, import again.
        #expect(await m.save(name: "Item", collection: "Shop/Orders") == nil)
        #expect(m.state.requests.count == 1 && !m.dirty)
        guard case .ok(let (json, file)) = await m.exportPostman(environment: false, name: "Shop") else { Issue.record("export failed"); return }
        #expect(file == "Shop.postman_collection.json")
        guard case .ok(let notice) = await m.importPostman(json) else { Issue.record("import failed"); return }
        #expect(notice.contains("Shop (2)"))
        #expect(m.state.requests.count == 2)
    }

    /// Everything saved survives a restart of the core; open tabs are app state and do not.
    @Test func savedStateSurvivesACoreRestart() async throws {
        let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let nekod = ["release", "debug"].map { root.appendingPathComponent("target/\($0)/nekod").path }
            .first { FileManager.default.isExecutableFile(atPath: $0) }
        guard let nekod else { Issue.record("nekod is not built"); return }
        let tmp = NSTemporaryDirectory() + "pxp-\(getpid())/"
        try FileManager.default.createDirectory(atPath: tmp, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(atPath: tmp) }
        let origin = try LocalOrigin()
        defer { origin.stop() }

        func start(_ n: Int) async throws -> (Process, APIClient) {
            let sock = tmp + "a\(n).sock"
            let p = Process()
            p.executableURL = URL(fileURLWithPath: nekod)
            p.arguments = ["run", "--socket", sock, "--data-dir", tmp + "data", "--listen", "127.0.0.1:0",
                           "--no-mcp", "--no-sdk", "--key-storage", "file"]
            p.standardOutput = FileHandle.nullDevice
            p.standardError = FileHandle.nullDevice
            try p.run()
            for _ in 0..<100 {
                let c = APIClient(socketPath: sock)
                if (try? c.connect()) != nil { return (p, c) }
                try await Task.sleep(nanoseconds: 50_000_000)
            }
            p.terminate()
            throw APIError.connect("core never accepted a connection")
        }

        let (p1, c1) = try await start(1)
        let m = ComposerModel()
        await m.attach(c1)
        m.edit {
            $0.method = "POST"; $0.url = "http://127.0.0.1:\(origin.port)/save?x=1"; $0.name = "Persist me"
            $0.bodyKind = .json; $0.body = "{\"a\":1}"
            $0.tests = [.with { $0.kind = .status; $0.op = .equals; $0.expected = "200" }]
            $0.headers = [.with { $0.name = "X-Test"; $0.value = "1"; $0.description_p = "kept" }]
        }
        #expect(await m.save(name: "Persist me", collection: "Suite/Nested") == nil)
        #expect(await m.setEnvironments([.with { $0.name = "Prod"; $0.vars = [.with { $0.name = "k"; $0.value = "v"; $0.secret = true }] }], active: "Prod") == nil)
        #expect(await m.setGlobals([.with { $0.name = "g"; $0.value = "1" }]) == nil)
        #expect(await m.setConfig(.with { $0.name = "Suite"; $0.auth = .with { $0.kind = .bearer; $0.token = "t" } }) == nil)
        await m.send()
        #expect(m.tab.sent?.status == 200)
        let before = m.state
        p1.terminate(); p1.waitUntilExit()

        let (p2, c2) = try await start(2)
        defer { p2.terminate() }
        let m2 = ComposerModel()
        await m2.attach(c2)
        let after = m2.state
        #expect(after.collections == ["Suite", "Suite/Nested"])
        #expect(after.requests.count == 1)
        let r = try #require(after.requests.first)
        #expect(r.name == "Persist me" && r.collection == "Suite/Nested" && r.method == "POST")
        #expect(r.body == "{\"a\":1}" && r.tests.count == 1 && r.headers[0].description_p == "kept")
        #expect(after.environments.first?.vars.first?.secret == true)
        #expect(after.activeEnvironment == "Prod")
        #expect(after.globals.first?.name == "g")
        #expect(after.configs.first?.auth.kind == .bearer)
        #expect(after.history.count == 1 && after.history[0].status == 200)
        #expect(after.requests == before.requests)
        // A fresh model starts with one blank tab: open tabs are not stored.
        #expect(m2.tabs.count == 1 && m2.tab.draft.url.isEmpty)
    }
}
