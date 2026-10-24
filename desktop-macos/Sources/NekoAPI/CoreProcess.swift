import Foundation

/// Where the core keeps things on macOS. Mirrors `core/crates/cli/src/paths.rs`.
public enum CorePaths {
    public static var home: String { NSHomeDirectory() }

    public static var dataDir: String {
        ProcessInfo.processInfo.environment["NEKO_DATA_DIR"]
            ?? home + "/Library/Application Support/neko"
    }

    public static var socket: String {
        if let p = ProcessInfo.processInfo.environment["NEKO_SOCKET"] { return p }
        var tmp = ProcessInfo.processInfo.environment["TMPDIR"] ?? "/tmp/"
        if !tmp.hasSuffix("/") { tmp += "/" }
        return tmp + "proxy/api.sock"
    }

    public static var caPath: String { dataDir + "/ca.pem" }
    public static var logDir: String { home + "/Library/Logs/Neko" }
}

/// Finds, starts and stops `nekod`. The core outlives nothing it spawned: if a
/// core already answers on the socket it is reused and left running on quit.
public final class CoreProcess {
    private var process: Process?
    public private(set) var spawned = false

    public init() {}

    public static func locate() -> String? {
        let fm = FileManager.default
        var candidates: [String] = []
        if let p = ProcessInfo.processInfo.environment["NEKOD"] { candidates.append(p) }
        if let exe = Bundle.main.executableURL?.deletingLastPathComponent() {
            candidates.append(exe.appendingPathComponent("nekod").path)
        }
        if let res = Bundle.main.resourcePath { candidates.append(res + "/nekod") }
        // Development: walk up from the executable looking for a cargo target dir.
        var dir = Bundle.main.executableURL?.deletingLastPathComponent()
        for _ in 0..<8 {
            guard let d = dir else { break }
            candidates.append(d.appendingPathComponent("target/release/nekod").path)
            candidates.append(d.appendingPathComponent("target/debug/nekod").path)
            dir = d.deletingLastPathComponent()
        }
        candidates.append(FileManager.default.currentDirectoryPath + "/target/release/nekod")
        return candidates.first { fm.isExecutableFile(atPath: $0) }
    }

    /// Connects if a core is already listening, otherwise starts one and retries.
    public func connect(timeout: TimeInterval = 8) async throws -> APIClient {
        let path = CorePaths.socket
        if let c = try? await Self.tryConnect(path) { return c }
        guard let nekod = Self.locate() else {
            throw APIError.connect("nekod was not found. Build it with `cargo build --release -p neko-cli` or set NEKOD.")
        }
        try spawn(nekod, socket: path)
        let deadline = Date().addingTimeInterval(timeout)
        var last: Error = APIError.connect("nekod did not start")
        while Date() < deadline {
            if let p = process, !p.isRunning {
                throw APIError.connect("nekod exited with status \(p.terminationStatus). See \(CorePaths.logDir)/nekod.log")
            }
            do { return try await Self.tryConnect(path) } catch { last = error }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        throw last
    }

    private static func tryConnect(_ path: String) async throws -> APIClient {
        try await Task.detached {
            let c = APIClient(socketPath: path)
            try c.connect()
            return c
        }.value
    }

    private func spawn(_ nekod: String, socket: String) throws {
        try FileManager.default.createDirectory(atPath: CorePaths.logDir, withIntermediateDirectories: true)
        let logPath = CorePaths.logDir + "/nekod.log"
        FileManager.default.createFile(atPath: logPath, contents: nil)
        let log = FileHandle(forWritingAtPath: logPath)
        let p = Process()
        p.executableURL = URL(fileURLWithPath: nekod)
        p.arguments = ["run", "--socket", socket]
        p.standardOutput = log
        p.standardError = log
        p.standardInput = FileHandle.nullDevice
        try p.run()
        process = p
        spawned = true
    }

    public func stop() {
        guard let p = process, p.isRunning else { return }
        p.terminate()
        p.waitUntilExit()
    }
}
