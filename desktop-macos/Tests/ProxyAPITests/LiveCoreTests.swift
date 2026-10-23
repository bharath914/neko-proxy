import Foundation
import Testing
@testable import ProxyAPI

/// Talks to a real `proxyd`. Skipped when the binary has not been built.
@Suite struct LiveCoreTests {
    @Test func captureAndFetchOverTheSocket() async throws {
        let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let proxyd = ["release", "debug"].map { root.appendingPathComponent("target/\($0)/proxyd").path }
            .first { FileManager.default.isExecutableFile(atPath: $0) }
        guard let proxyd else {
            Issue.record("proxyd is not built: run `cargo build -p proxy-cli` first")
            return
        }

        let tmp = NSTemporaryDirectory() + "pxt-\(getpid())/"
        try FileManager.default.createDirectory(atPath: tmp, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(atPath: tmp) }
        let sock = tmp + "a.sock"

        // A tiny HTTP origin so the test needs no network.
        let origin = try LocalOrigin()
        defer { origin.stop() }

        let p = Process()
        p.executableURL = URL(fileURLWithPath: proxyd)
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

        let status = try await c.request { $0.status = .init() }
        guard case .status(let st)? = status.body else { Issue.record("no status"); return }
        let listen = st.listen
        #expect(listen.hasPrefix("127.0.0.1:"))

        // Drive one request through the proxy.
        let curl = Process()
        curl.executableURL = URL(fileURLWithPath: "/usr/bin/curl")
        curl.arguments = ["-s", "-o", "/dev/null", "-x", "http://\(listen)", "http://127.0.0.1:\(origin.port)/hello?x=1"]
        try curl.run()
        curl.waitUntilExit()

        var found: Proxy_Api_V1_FlowSummary?
        for _ in 0..<50 {
            let r = try await c.request { $0.search = .init() }
            if case .search(let s)? = r.body, let f = s.items.first { found = f; break }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        let flow = try #require(found)
        #expect(flow.method == "GET")
        #expect(flow.status == 200)
        #expect(flow.path.hasPrefix("/hello"))

        let d = try await c.request { $0.getFlow = .with { $0.id = flow.id; $0.parts = 7 } }
        guard case .flow(let detail)? = d.body else { Issue.record("no detail"); return }
        #expect(detail.curl.contains("/hello"))
        #expect(detail.responseBody.text == "ok")
    }
}

final class LocalOrigin {
    let port: UInt16
    private let fd: Int32
    private var running = true

    init() throws {
        let fd = socket(AF_INET, SOCK_STREAM, 0)
        self.fd = fd
        var one: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &one, 4)
        var a = sockaddr_in()
        a.sin_family = sa_family_t(AF_INET)
        a.sin_addr.s_addr = inet_addr("127.0.0.1")
        a.sin_port = 0
        let rc = withUnsafePointer(to: &a) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size)) }
        }
        guard rc == 0, listen(fd, 8) == 0 else { throw APIError.connect("origin bind") }
        var len = socklen_t(MemoryLayout<sockaddr_in>.size)
        withUnsafeMutablePointer(to: &a) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { _ = getsockname(fd, $0, &len) }
        }
        port = UInt16(bigEndian: a.sin_port)
        Thread.detachNewThread {
            while true {
                let c = accept(fd, nil, nil)
                if c < 0 { return }
                var buf = [UInt8](repeating: 0, count: 4096)
                _ = read(c, &buf, buf.count)
                let resp = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                _ = resp.withCString { write(c, $0, strlen($0)) }
                close(c)
            }
        }
    }

    func stop() { shutdown(fd, SHUT_RDWR); close(fd) }
}
