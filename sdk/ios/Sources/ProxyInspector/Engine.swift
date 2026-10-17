#if DEBUG
import Foundation

struct Candidate: Equatable {
    var host: String, port: Int
    var kind = ""
    var fingerprintShort: String? = nil
    var desktopName: String? = nil
}

enum LogLevel { case debug, info, warn, error }

/// A blocking, pinned TLS connection. The Apple implementation wraps Network.framework (`NetworkTransport`);
/// tests use an in-memory one.
protocol Connection: AnyObject {
    var fingerprint: Data { get }
    func send(_ data: Data) throws
    /// Exactly `count` bytes, or throws when the connection ends or `timeout` passes.
    func receive(exactly count: Int, timeout: TimeInterval?) throws -> Data
    func close()
}

struct FingerprintMismatch: Error { let expected: Data, saw: Data }
struct TransportError: Error, CustomStringConvertible { let description: String; init(_ d: String) { description = d } }

protocol Transport {
    /// While paired, `pinned` is the only certificate fingerprint accepted. While pairing it is nil: any
    /// certificate is accepted for the handshake and its fingerprint is returned for the pairing code.
    func connect(host: String, port: Int, pinned: Data?, timeout: TimeInterval) throws -> Connection
}

struct Pairing_ { var token: Data, fingerprint: Data, lastHost = "", lastPort = 0, desktopName = "" }

protocol PairingStorage: AnyObject {
    func installId() -> String
    func load() -> Pairing_?
    func save(_ p: Pairing_)
    func clear()
}

/// Everything that differs between iOS and a plain Swift environment.
protocol Platform: AnyObject {
    var appId: String { get }
    var appName: String { get }
    var appVersion: String { get }
    var build: String { get }
    var deviceName: String { get }
    var deviceModel: String { get }
    var osVersion: String { get }
    var storage: PairingStorage { get }
    var transport: Transport { get }
    func localCandidates() -> [Candidate]
    func discover(timeout: TimeInterval) -> [Candidate]
    func log(_ level: LogLevel, _ message: String)
    func notify(_ message: String)
}

extension Platform {
    func discover(timeout: TimeInterval) -> [Candidate] { [] }
    func notify(_ message: String) {}
}

/// Settings capture runs under, once a desktop has accepted this app.
final class Live {
    let config: Config
    unowned let engine: Engine
    let redactor: Redactor
    init(config: Config, engine: Engine) { self.config = config; self.engine = engine; redactor = Redactor(level: config.redaction, extraKeys: config.extraRedactKeys) }

    func hostAllowed(_ host: String) -> Bool {
        let h = host.lowercased()
        if config.hostDeny.contains(where: { match($0, h) }) { return false }
        return config.hostAllow.isEmpty || config.hostAllow.contains(where: { match($0, h) })
    }

    private func match(_ pattern: String, _ host: String) -> Bool {
        let p = pattern.lowercased()
        if p.hasPrefix("*.") { return host.hasSuffix(String(p.dropFirst(1))) || host == String(p.dropFirst(2)) }
        return host == p
    }
}

enum EngineState { case stopped, disconnected, connecting, pairing, connected }

struct EngineSettings { var host: String? = nil, port = Engine.defaultPort, desktopName: String? = nil }

/// Owns the connection to the desktop: discovery, TLS, pairing, and the single sender thread.
/// Capture is on only while this holds a `Live`; otherwise a request pays one atomic read.
final class Engine {
    static let defaultPort = 8897
    static let sdkVersion = "0.1.0"
    static let retryReset: TimeInterval = -1, retryBackoff: TimeInterval = -2

    let platform: Platform
    var settings: () -> EngineSettings
    let ring = BodyRing()
    private let queue = DropOldestQueue<(Sink) throws -> Void>(capacity: 2048)
    private let control = Locked<[(Sink) throws -> Void]>([])
    private let liveBox = Locked<Live?>(nil)
    private let stateBox = Locked<EngineState>(.stopped)
    private let stopFlag = Locked<Bool>(true)
    private let screenBox = Locked<String>("")
    private var thread: Thread?
    let bytesSent = Locked<Int>(0)

    typealias Sink = (Envelope) throws -> Void

    init(platform: Platform, settings: @escaping () -> EngineSettings) { self.platform = platform; self.settings = settings }

    var live: Live? { liveBox.value }
    var state: EngineState { stateBox.value }
    var screen: String { get { screenBox.value } set { screenBox.value = newValue } }
    var dropped: Int { queue.dropped }
    private var stopped: Bool { stopFlag.value }

    func start() {
        guard stopped else { return }
        stopFlag.value = false
        stateBox.value = .disconnected
        let t = Thread { [weak self] in self?.run() }
        t.name = "inspector-io"
        thread = t
        t.start()
    }

    func stop() {
        stopFlag.value = true
        liveBox.value = nil
        stateBox.value = .stopped
        thread = nil
    }

    func forgetPairing() { platform.storage.clear() }

    /// Queue work for the sender thread. Only call while capture is active.
    func emit(_ job: @escaping (Sink) throws -> Void) { queue.offer(job) }

    private func log(_ l: LogLevel, _ m: String) { platform.log(l, m) }

    // MARK: connection loop

    private func run() {
        let backoff: [TimeInterval] = [2, 5, 15, 60]
        var failures = 0
        while !stopped {
            var retry = Engine.retryBackoff
            do { retry = try attempt() } catch { log(.debug, "connection attempt failed: \(error)") }
            liveBox.value = nil
            if stateBox.value != .stopped { stateBox.value = .disconnected }
            let wait: TimeInterval
            if retry == Engine.retryReset { failures = 0; wait = backoff[0] }
            else if retry == Engine.retryBackoff { wait = backoff[min(failures, backoff.count - 1)]; failures += 1 }
            else { wait = retry }
            var slept: TimeInterval = 0
            while slept < wait, !stopped { Thread.sleep(forTimeInterval: 0.1); slept += 0.1 }
        }
    }

    private func candidates(_ pairing: Pairing_?, _ s: EngineSettings) -> [Candidate] {
        var list: [Candidate] = []
        if let h = s.host { list.append(Candidate(host: h, port: s.port, kind: "wifi", desktopName: s.desktopName)) }
        if let p = pairing, !p.lastHost.isEmpty, p.lastPort > 0 { list.append(Candidate(host: p.lastHost, port: p.lastPort)) }
        list += platform.localCandidates().map { var c = $0; if c.port == 0 { c.port = s.port }; return c }
        var seen = Set<String>()
        return list.filter { seen.insert("\($0.host):\($0.port)").inserted }
    }

    private func attempt() throws -> TimeInterval {
        let s = settings()
        let pairing = platform.storage.load()
        stateBox.value = .connecting
        var mismatch: FingerprintMismatch?
        for c in candidates(pairing, s) {
            if stopped { return 0 }
            do {
                let conn = try platform.transport.connect(host: c.host, port: c.port, pinned: pairing?.fingerprint, timeout: 1.5)
                return try session(conn, c, pairing, s)
            } catch let m as FingerprintMismatch { mismatch = m } catch { continue }
        }
        let found = platform.discover(timeout: 2)
        if pairing == nil {
            let distinct = Set(found.compactMap { $0.fingerprintShort ?? $0.desktopName })
            var pick: [Candidate] = found
            if let wanted = s.desktopName { pick = found.filter { $0.desktopName?.lowercased() == wanted.lowercased() } }
            else if distinct.count > 1 {
                log(.warn, "Several desktops found: \(found.compactMap { $0.desktopName }.joined(separator: ", ")). Not pairing automatically. Choose one with Inspector.start(desktopName:).")
                pick = []
            }
            for c in pick {
                guard let conn = try? platform.transport.connect(host: c.host, port: c.port, pinned: nil, timeout: 2) else { continue }
                return try session(conn, c, nil, s)
            }
        } else if let p = pairing {
            for c in found where c.fingerprintShort == nil || p.fingerprint.hex.hasPrefix(c.fingerprintShort!) {
                do {
                    let conn = try platform.transport.connect(host: c.host, port: c.port, pinned: p.fingerprint, timeout: 2)
                    return try session(conn, c, p, s)
                } catch let m as FingerprintMismatch { mismatch = m } catch { continue }
            }
        }
        if let m = mismatch {
            log(.error, "A desktop answered with a different certificate (\(m.saw.hex.prefix(16))) than the one this app paired with. Not connecting. If you replaced the certificate on the desktop, call Inspector.forgetPairing() and run again.")
        }
        return Engine.retryBackoff
    }

    // MARK: one connection

    private func readEnvelope(_ conn: Connection, timeout: TimeInterval?) throws -> Envelope {
        let n = try Frames.length(try conn.receive(exactly: 4, timeout: timeout))
        return try Envelope.decode(try conn.receive(exactly: n, timeout: timeout ?? 15))
    }

    private func write(_ conn: Connection, _ e: Envelope) throws {
        let d = Frames.encode(e)
        try conn.send(d)
        bytesSent.mutate { $0 += d.count }
    }

    private func session(_ conn: Connection, _ c: Candidate, _ pairing: Pairing_?, _ s: EngineSettings) throws -> TimeInterval {
        defer { conn.close() }
        let kind = c.kind.isEmpty ? (c.host == "localhost" || c.host == "127.0.0.1" ? "simulator" : "wifi") : c.kind
        var hello = Hello()
        hello.sdkName = "ios"; hello.sdkVersion = Engine.sdkVersion; hello.appId = platform.appId; hello.appName = platform.appName
        hello.appVersion = platform.appVersion; hello.build = platform.build; hello.deviceInstallId = platform.storage.installId()
        hello.deviceName = platform.deviceName; hello.deviceModel = platform.deviceModel; hello.osVersion = platform.osVersion
        hello.deviceTimeMs = Int64(Date().timeIntervalSince1970 * 1000); hello.token = pairing?.token ?? Data(); hello.connection = kind
        try write(conn, Envelope(kind: Envelope.hello, body: hello.encode()))
        let first = try readEnvelope(conn, timeout: 10)
        if first.kind == Envelope.error { logWireError(try WireError.decode(first.body)); return 60 }
        guard first.kind == Envelope.helloAck else { return Engine.retryBackoff }
        let ack = try HelloAck.decode(first.body)
        if let wanted = s.desktopName, !ack.desktopName.isEmpty, ack.desktopName.lowercased() != wanted.lowercased(), pairing == nil {
            log(.info, "Skipping desktop \"\(ack.desktopName)\" (looking for \"\(wanted)\").")
            return Engine.retryBackoff
        }
        var paired = pairing
        switch ack.state {
        case AckState.authenticated: break
        case AckState.pairingRequired:
            stateBox.value = .pairing
            guard let p = try pair(conn, c, ack) else { return 600 }
            paired = p
        case AckState.denied:
            log(.warn, "The desktop denied this app. \(ack.message) Backing off.")
            return max(TimeInterval(ack.retryAfterS), 60)
        default:
            log(.debug, "Desktop is not accepting SDK connections: \(ack.message)")
            return max(TimeInterval(ack.retryAfterS), 5)
        }
        if var p = paired { p.lastHost = c.host; p.lastPort = c.port; p.desktopName = ack.desktopName; platform.storage.save(p) }
        return try authenticated(conn, ack.desktopName, c)
    }

    private func pair(_ conn: Connection, _ c: Candidate, _ ack: HelloAck) throws -> Pairing_? {
        let nonce = Pairing.nonce()
        try write(conn, Envelope(kind: Envelope.pairRequest, body: { var w = ProtoWriter(); w.bytes(1, Pairing.commit(nonce)); return w.data }()))
        guard let chEnv = try expect(conn, Envelope.pairChallenge, timeout: 10) else { return nil }
        let challenge = try PairChallenge.decode(chEnv.body)
        try write(conn, Envelope(kind: Envelope.pairReveal, body: { var w = ProtoWriter(); w.bytes(1, nonce); return w.data }()))
        let code = Pairing.spaced(Pairing.code(fingerprint: conn.fingerprint, clientNonce: nonce, serverNonce: challenge.serverNonce))
        let name = ack.desktopName.isEmpty ? c.host : ack.desktopName
        log(.warn, "Pairing code \(code) for \(name). Click Allow on the desktop if it shows the same code.")
        platform.notify("Inspector pairing code \(code). Check it on \(name).")
        guard let resEnv = try expect(conn, Envelope.pairResult, timeout: TimeInterval(max(challenge.timeoutS, 10) + 10)) else { return nil }
        let result = try PairResult.decode(resEnv.body)
        if !result.allowed { log(.warn, "Pairing was not accepted: \(result.message)"); return nil }
        log(.info, "Paired with \(name).")
        let p = Pairing_(token: result.token, fingerprint: conn.fingerprint, lastHost: c.host, lastPort: c.port, desktopName: ack.desktopName)
        platform.storage.save(p)
        return p
    }

    private func expect(_ conn: Connection, _ kind: Int, timeout: TimeInterval) throws -> Envelope? {
        while true {
            let e = try readEnvelope(conn, timeout: timeout)
            switch e.kind {
            case kind: return e
            case Envelope.ping:
                let p = try Ping.decode(e.body)
                try write(conn, Envelope(kind: Envelope.pong, body: Pong(echoMs: p.timeMs, timeMs: Int64(Date().timeIntervalSince1970 * 1000)).encode()))
            case Envelope.error: logWireError(try WireError.decode(e.body)); return nil
            default: return nil
            }
        }
    }

    private func logWireError(_ e: WireError) {
        if e.code == ErrorCode.unsupportedVersion {
            log(.error, "The desktop does not support this SDK's protocol version (\(protocolVersion)): \(e.message) It speaks \(e.minVersion) to \(e.maxVersion). Update the desktop app or the SDK.")
        } else { log(.warn, "The desktop closed the connection: \(e.message)") }
    }

    private func authenticated(_ conn: Connection, _ desktop: String, _ c: Candidate) throws -> TimeInterval {
        let readerDead = Locked<Bool>(false)
        let lastRead = Locked<Date>(Date())
        let reader = Thread { [weak self] in
            guard let self = self else { return }
            do {
                while !readerDead.value {
                    let e = try self.readEnvelope(conn, timeout: nil)
                    lastRead.value = Date()
                    switch e.kind {
                    case Envelope.config:
                        let cfg = try Config.decode(e.body)
                        self.liveBox.value = cfg.captureEnabled ? Live(config: cfg, engine: self) : nil
                        self.stateBox.value = .connected
                    case Envelope.ping:
                        let p = try Ping.decode(e.body)
                        self.control.mutate { $0.append { sink in try sink(Envelope(kind: Envelope.pong, body: Pong(echoMs: p.timeMs, timeMs: Int64(Date().timeIntervalSince1970 * 1000)).encode())) } }
                    case Envelope.bodyFetch:
                        let f = try BodyFetch.decode(e.body)
                        self.control.mutate { $0.append { sink in self.answerFetch(sink, f) } }
                    case Envelope.error: self.logWireError(try WireError.decode(e.body)); readerDead.value = true
                    default: break
                    }
                }
            } catch { }
            readerDead.value = true
        }
        reader.name = "inspector-read"
        reader.start()
        log(.info, "Connected to \(desktop) (\(c.host):\(c.port)).")
        let sink: Sink = { [unowned self] e in try self.write(conn, e) }
        var lastWrite = Date()
        defer { liveBox.value = nil; queue.clear(); control.value = []; conn.close(); readerDead.value = true; log(.info, "Disconnected from \(desktop).") }
        do {
            while !stopped, !readerDead.value {
                var wrote = false
                let pending = control.mutate { l -> [(Sink) throws -> Void] in let r = l; l = []; return r }
                for j in pending { try j(sink); wrote = true }
                if let job = queue.poll(timeout: 0.5) { try job(sink); wrote = true }
                if wrote { lastWrite = Date() }
                if Date().timeIntervalSince(lastWrite) >= 15 {
                    try write(conn, Envelope(kind: Envelope.ping, body: Ping(timeMs: Int64(Date().timeIntervalSince1970 * 1000)).encode())); lastWrite = Date()
                }
                if Date().timeIntervalSince(lastRead.value) > 60 { log(.debug, "desktop went silent"); break }
            }
        } catch { log(.debug, "connection lost: \(error)") }
        return Engine.retryReset
    }

    private func answerFetch(_ sink: Sink, _ f: BodyFetch) {
        let body = ring.get(f.callId, f.part)
        try? sink(Envelope(kind: Envelope.bodyFetchResult, body: BodyFetchResult(callId: f.callId, part: f.part, available: body != nil).encode()))
        if let b = body { try? Bodies.sendChunks(sink, callId: f.callId, part: f.part, body: b, firstSeq: 0, last: true, truncated: false) }
    }
}

/// Thread-safe box.
final class Locked<T> {
    private var v: T
    private let lock = NSLock()
    init(_ v: T) { self.v = v }
    var value: T { get { lock.lock(); defer { lock.unlock() }; return v } set { lock.lock(); v = newValue; lock.unlock() } }
    @discardableResult func mutate<R>(_ f: (inout T) -> R) -> R { lock.lock(); defer { lock.unlock() }; return f(&v) }
}

/// Chunking of bodies for the wire. Compression is left out on iOS for now: the desktop accepts uncompressed chunks.
enum Bodies {
    static let chunk = 64 * 1024

    static func sendChunks(_ sink: Engine.Sink, callId: String, part: Int, body: Data, firstSeq: Int, last: Bool, truncated: Bool) throws {
        var seq = firstSeq, off = 0
        if body.isEmpty {
            if last || truncated { try sink(Envelope(kind: Envelope.bodyChunk, body: BodyChunk(callId: callId, part: part, seq: seq, last: last, truncated: truncated).encode())) }
            return
        }
        while off < body.count {
            let n = min(chunk, body.count - off)
            let isLast = off + n >= body.count
            let piece = body.subdata(in: (body.startIndex + off)..<(body.startIndex + off + n))
            try sink(Envelope(kind: Envelope.bodyChunk, body: BodyChunk(callId: callId, part: part, seq: seq, data: piece, last: last && isLast, truncated: truncated && isLast).encode()))
            seq += 1; off += n
        }
    }
}
#endif
