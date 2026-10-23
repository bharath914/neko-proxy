import Foundation
import Testing
@testable import NekoInspector

/// One direction of an in-memory byte pipe.
final class Pipe_ {
    private var buf = Data(), closed = false
    private let cond = NSCondition()
    func write(_ d: Data) { cond.lock(); buf.append(d); cond.broadcast(); cond.unlock() }
    func close() { cond.lock(); closed = true; cond.broadcast(); cond.unlock() }
    func read(_ n: Int, timeout: TimeInterval?) throws -> Data {
        let deadline = Date(timeIntervalSinceNow: timeout ?? 30)
        cond.lock(); defer { cond.unlock() }
        while buf.count < n {
            if closed { throw TransportError("closed") }
            if !cond.wait(until: deadline) { throw TransportError("timeout") }
        }
        let out = buf.prefix(n); buf = Data(buf.dropFirst(n))
        return Data(out)
    }
}

final class FakeConnection: Connection {
    let fingerprint: Data
    let toServer = Pipe_(), toClient = Pipe_()
    init(fingerprint: Data) { self.fingerprint = fingerprint }
    func send(_ data: Data) throws { toServer.write(data) }
    func receive(exactly count: Int, timeout: TimeInterval?) throws -> Data { try toClient.read(count, timeout: timeout) }
    func close() { toServer.close(); toClient.close() }

    // The desktop's side.
    func serverRead() throws -> Envelope {
        let n = try Frames.length(try toServer.read(4, timeout: 10))
        return try Envelope.decode(try toServer.read(n, timeout: 10))
    }
    func serverWrite(_ e: Envelope) { toClient.write(Frames.encode(e)) }
}

/// A fake desktop: accepts connections and plays a script.
final class FakeDesktop: Transport {
    let fingerprint = Data((0..<32).map { UInt8($0 &+ 7) })
    let token = Data((0..<32).map { UInt8($0 &+ 100) })
    var requireToken = false
    let received = Locked<[Envelope]>([])
    let pairingCodes = Locked<[String]>([])
    let helloTokens = Locked<[Data]>([])
    let connects = Locked<Int>(0)

    func connect(host: String, port: Int, pinned: Data?, timeout: TimeInterval) throws -> Connection {
        guard host == "127.0.0.1" else { throw TransportError("unreachable") }
        if let p = pinned, p != fingerprint { throw FingerprintMismatch(expected: p, saw: fingerprint) }
        connects.mutate { $0 += 1 }
        let c = FakeConnection(fingerprint: fingerprint)
        Thread { self.serve(c) }.start()
        return c
    }

    private func ack(_ state: Int) -> Envelope {
        var w = ProtoWriter(); w.int(1, state); w.string(2, "test-desktop"); w.string(3, "s1")
        return Envelope(kind: Envelope.helloAck, body: w.data)
    }

    private func serve(_ c: FakeConnection) {
        do {
            var hello = try c.serverRead()
            var r = ProtoReader(hello.body), token = Data()
            while try r.next() { if r.field == 13 { token = try r.bytes() } else { try r.skip() } }
            helloTokens.mutate { $0.append(token) }
            if token == self.token {
                c.serverWrite(ack(AckState.authenticated))
            } else {
                c.serverWrite(ack(AckState.pairingRequired))
                let req = try c.serverRead()
                var commit = Data(); var rr = ProtoReader(req.body)
                while try rr.next() { if rr.field == 1 { commit = try rr.bytes() } else { try rr.skip() } }
                let serverNonce = Data((0..<32).map { UInt8($0 &+ 64) })
                var cw = ProtoWriter(); cw.bytes(1, serverNonce); cw.int(2, 60)
                c.serverWrite(Envelope(kind: Envelope.pairChallenge, body: cw.data))
                let reveal = try c.serverRead()
                var cn = Data(); var rv = ProtoReader(reveal.body)
                while try rv.next() { if rv.field == 1 { cn = try rv.bytes() } else { try rv.skip() } }
                guard Pairing.commit(cn) == commit else { return }
                pairingCodes.mutate { $0.append(Pairing.code(fingerprint: fingerprint, clientNonce: cn, serverNonce: serverNonce)) }
                var pw = ProtoWriter(); pw.bool(1, true); pw.bytes(2, self.token)
                c.serverWrite(Envelope(kind: Envelope.pairResult, body: pw.data))
            }
            _ = hello; hello = Envelope(kind: 0, body: Data())
            c.serverWrite(Envelope(kind: Envelope.config, body: Config().encode()))
            while true { let e = try c.serverRead(); received.mutate { $0.append(e) } }
        } catch { }
    }
}

final class MemoryStorage: PairingStorage {
    var pairing: Pairing_?
    let id = UUID().uuidString.lowercased()
    func installId() -> String { id }
    func load() -> Pairing_? { pairing }
    func save(_ p: Pairing_) { pairing = p }
    func clear() { pairing = nil }
}

final class FakePlatform: Platform {
    let appId = "com.demo.shop", appName = "Demo Shop", appVersion = "1.2.3", build = "42"
    let deviceName = "Test iPhone", deviceModel = "iPhone", osVersion = "iOS 19"
    let storage: PairingStorage
    let transport: Transport
    let logs = Locked<[String]>([]), notices = Locked<[String]>([])
    init(desktop: FakeDesktop, storage: PairingStorage = MemoryStorage()) { transport = desktop; self.storage = storage }
    func localCandidates() -> [Candidate] { [Candidate(host: "127.0.0.1", port: 0, kind: "simulator")] }
    func log(_ level: LogLevel, _ message: String) { logs.mutate { $0.append(message) } }
    func notify(_ message: String) { notices.mutate { $0.append(message) } }
}

func wait(_ what: String, timeout: TimeInterval = 8, _ cond: () -> Bool) -> Bool {
    let end = Date(timeIntervalSinceNow: timeout)
    while Date() < end { if cond() { return true }; Thread.sleep(forTimeInterval: 0.02) }
    return false
}

@Suite(.serialized)
struct EngineTests {
    func start(_ platform: FakePlatform) -> Engine {
        let e = Engine(platform: platform, settings: { EngineSettings() })
        Runtime.engineBox.value = e
        e.start()
        return e
    }

    func stop(_ e: Engine) { e.stop(); Runtime.engineBox.value = nil }

    func call(_ url: String = "https://api.shop.test/v1/me", body: String? = nil, response: String = #"{"ok":true}"#) {
        var info = CallInfo(method: "POST", url: url, headers: [("Accept", "application/json"), ("Authorization", "Bearer abc"), ("Content-Type", "application/json")], clientStack: "urlsession")
        info.name = "Profile"
        let h = Recorder.begin(info)!
        if let b = body { h.onRequestBody(Data(b.utf8)) }
        h.responseStarted(status: 200, headers: [("Content-Type", "application/json")], httpVersion: "h2", size: Int64(response.utf8.count))
        h.onResponseBody(Data(response.utf8))
        var t = Timings(); t.ttfb = 20; t.download = 3
        h.complete(timings: t, fromCache: false)
    }

    @Test func pairsStreamsAndReconnectsWithTheStoredToken() throws {
        let desktop = FakeDesktop()
        let storage = MemoryStorage()
        let platform = FakePlatform(desktop: desktop, storage: storage)
        let engine = start(platform)
        #expect(wait("capture") { Recorder.active() })
        #expect(desktop.pairingCodes.value.count == 1)
        // The code the SDK logged and showed is the one the desktop computed.
        let code = Pairing.spaced(desktop.pairingCodes.value[0])
        #expect(platform.logs.value.contains { $0.contains("Pairing code \(code)") })
        #expect(platform.notices.value.first?.contains(code) == true)
        #expect(storage.pairing?.token == desktop.token)
        #expect(storage.pairing?.fingerprint == desktop.fingerprint)

        call(body: #"{"password":"hunter2","note":"hi"}"#)
        #expect(wait("frames") { desktop.received.value.count >= 5 })
        let kinds = desktop.received.value.map { $0.kind }
        #expect(kinds == [Envelope.callStart, Envelope.responseStart, Envelope.bodyChunk, Envelope.bodyChunk, Envelope.callComplete])
        // Redacted on the device, before it was sent.
        let callStart = desktop.received.value[0]
        var r = ProtoReader(callStart.body), sawAuth = false
        while try r.next() {
            if r.field == 5 {
                var h = ProtoReader(try r.bytes()), n = "", v = ""
                while try h.next() { if h.field == 1 { n = try h.string() } else if h.field == 2 { v = try h.string() } else { try h.skip() } }
                if n == "Authorization" { #expect(v == "[redacted]"); sawAuth = true }
            } else { try r.skip() }
        }
        #expect(sawAuth)
        var cr = ProtoReader(desktop.received.value[2].body), reqBody = ""
        while try cr.next() { if cr.field == 4 { reqBody = String(decoding: try cr.bytes(), as: UTF8.self) } else { try cr.skip() } }
        #expect(reqBody == #"{"password":"[redacted]","note":"hi"}"#)
        stop(engine)

        // A new engine with the same storage presents the token and is not asked to pair.
        let engine2 = start(FakePlatform(desktop: desktop, storage: storage))
        #expect(wait("reconnect") { Recorder.active() })
        #expect(desktop.pairingCodes.value.count == 1)
        #expect(desktop.helloTokens.value.last == desktop.token)
        stop(engine2)
    }

    @Test func aDifferentCertificateIsRefusedAndNothingIsSent() {
        let desktop = FakeDesktop()
        let storage = MemoryStorage()
        storage.pairing = Pairing_(token: desktop.token, fingerprint: Data(repeating: 9, count: 32))
        let platform = FakePlatform(desktop: desktop, storage: storage)
        let engine = start(platform)
        #expect(wait("the mismatch to be logged") { platform.logs.value.contains { $0.contains("different certificate") } })
        #expect(!Recorder.active())
        #expect(desktop.connects.value == 0)
        stop(engine)
    }

    @Test func whenNothingIsConnectedACallDoesNoCaptureWork() {
        #expect(Runtime.engine == nil)
        #expect(!Recorder.active())
        #expect(Recorder.begin(CallInfo(method: "GET", url: "https://x.test/", headers: [], clientStack: "urlsession")) == nil)
    }

    @Test func urlSessionAndAlamofireRecordsOfTheSameCallAreDeduplicated() throws {
        let desktop = FakeDesktop()
        let engine = start(FakePlatform(desktop: desktop))
        #expect(wait("capture") { Recorder.active() })
        let af = Recorder.begin(CallInfo(method: "GET", url: "https://x.test/a", headers: [], clientStack: "alamofire"))
        let us = Recorder.begin(CallInfo(method: "GET", url: "https://x.test/a", headers: [], clientStack: "urlsession"))
        #expect(af != nil)
        #expect(us == nil, "the Alamofire record carries the metadata and wins")
        #expect(Recorder.begin(CallInfo(method: "GET", url: "https://x.test/b", headers: [], clientStack: "urlsession")) != nil)
        stop(engine)
    }

    @Test func ringAndQueueBehave() {
        let ring = BodyRing(budget: 100)
        ring.put("a", 1, Data(count: 60)); ring.put("b", 1, Data(count: 60))
        #expect(ring.get("a", 1) == nil && ring.get("b", 1) != nil)
        #expect(!ring.put("huge", 1, Data(count: 101)))
        let q = DropOldestQueue<Int>(capacity: 3)
        (1...5).forEach { q.offer($0) }
        #expect(q.dropped == 2)
        #expect([q.poll(timeout: 0.01), q.poll(timeout: 0.01), q.poll(timeout: 0.01)] == [3, 4, 5])
        #expect(q.poll(timeout: 0.01) == nil)
    }
}
