import Foundation
import Testing
@testable import ProxyInspector

/// Decodes and re-encodes the golden frames written by the Rust test `core/crates/sdk/tests/fixtures.rs`.
struct WireTests {
    struct Frame { let name: String, bytes: Data }

    static let fixtures: [String: Any] = {
        let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("proto/sdk-fixtures.json")
        return try! JSONSerialization.jsonObject(with: Data(contentsOf: url)) as! [String: Any]
    }()

    func section(_ name: String) -> [String: Data] {
        var out: [String: Data] = [:]
        for f in Self.fixtures[name] as! [[String: String]] { out[f["name"]!] = Data(hex: f["hex"]!)! }
        return out
    }

    func pairing(_ k: String) -> Data { Data(hex: (Self.fixtures["pairing"] as! [String: String])[k]!)! }

    func envelope(_ frame: Data) throws -> Envelope {
        let n = try Frames.length(frame.prefix(4))
        #expect(n == frame.count - 4)
        return try Envelope.decode(frame.dropFirst(4))
    }

    var hello: Hello {
        var h = Hello()
        h.sdkName = "android"; h.sdkVersion = "0.1.0"; h.appId = "com.demo.shop"; h.appName = "Shop"; h.appVersion = "1.2.3"; h.build = "42"
        h.deviceInstallId = "c0ffee00-1111-4222-8333-444455556666"; h.deviceName = "Pixel 8"; h.deviceModel = "Google Pixel 8"
        h.osVersion = "Android 15"; h.deviceTimeMs = 1_800_000_000_000; h.connection = "usb"
        return h
    }

    @Test func sha256KnownVectors() {
        #expect(Data(SHA256.hash(Data())).hex == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        #expect(Data(SHA256.hash(Data("abc".utf8))).hex == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        // A message that crosses the 56 byte padding boundary.
        #expect(Data(SHA256.hash(Data("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq".utf8))).hex == "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1")
    }

    @Test func pairingCodeMatchesTheRustKnownAnswer() {
        let fp = Data((0..<32).map { UInt8($0) }), cn = Data((32..<64).map { UInt8($0) }), sn = Data((64..<96).map { UInt8($0) })
        #expect(Pairing.code(fingerprint: fp, clientNonce: cn, serverNonce: sn) == "730832")
        let code = (Self.fixtures["pairing"] as! [String: String])["code"]!
        #expect(Pairing.code(fingerprint: pairing("fingerprint_hex"), clientNonce: pairing("client_nonce_hex"), serverNonce: pairing("server_nonce_hex")) == code)
    }

    @Test func clientMessagesEncodeToTheSameBytesAsTheRustSide() throws {
        let fresh = section("fresh_pairing"), re = section("reconnect")
        #expect(Frames.encode(Envelope(kind: Envelope.hello, body: hello.encode())) == fresh["hello"])
        let nonce = pairing("client_nonce_hex")
        var w = ProtoWriter(); w.bytes(1, Pairing.commit(nonce))
        #expect(Frames.encode(Envelope(kind: Envelope.pairRequest, body: w.data)) == fresh["pair_request"])
        var r = ProtoWriter(); r.bytes(1, nonce)
        #expect(Frames.encode(Envelope(kind: Envelope.pairReveal, body: r.data)) == fresh["pair_reveal"])

        var h2 = hello; h2.token = pairing("token_hex")
        #expect(Frames.encode(Envelope(kind: Envelope.hello, body: h2.encode())) == re["hello"])
        let id = "0191a2b3-0000-7000-8000-000000000001"
        var cs = CallStart()
        cs.callId = id; cs.startedAtMs = 1_800_000_001_000; cs.method = "POST"; cs.url = "https://api.demo-shop.test/v1/cart/7/coupon"
        cs.headers = [Header(name: "Accept", value: "application/json")]; cs.httpVersion = "h2"; cs.clientStack = "okhttp"
        cs.routeTemplate = "/v1/cart/{id}/coupon"; cs.name = "Apply coupon"; cs.owner = "payments"; cs.screen = "CartActivity"; cs.requestBodySize = 19
        #expect(Frames.encode(Envelope(kind: Envelope.callStart, body: cs.encode())) == re["call_start"])
        let chunk = BodyChunk(callId: id, part: Part.request, seq: 0, data: Data(#"{"code":"SPRING25"}"#.utf8), last: true)
        #expect(Frames.encode(Envelope(kind: Envelope.bodyChunk, body: chunk.encode())) == re["body_chunk"])
        let rs = ResponseStart(callId: id, status: 200, headers: [Header(name: "Content-Type", value: "application/json")], httpVersion: "h2", responseBodySize: 17)
        #expect(Frames.encode(Envelope(kind: Envelope.responseStart, body: rs.encode())) == re["response_start"])
        let cc = CallComplete(callId: id, endedAtMs: 1_800_000_001_084, timings: Timings(dns: 0, connect: 0, tls: -1, requestSent: 1, ttfb: 60, download: 3), responseBodyAvailable: true, requestBodyAvailable: true)
        #expect(Frames.encode(Envelope(kind: Envelope.callComplete, body: cc.encode())) == re["call_complete"])
    }

    @Test func serverMessagesDecode() throws {
        let fresh = section("fresh_pairing")
        let ack = try HelloAck.decode(try envelope(fresh["hello_ack"]!).body)
        #expect(ack.state == AckState.pairingRequired)
        #expect(ack.desktopName == "fedora")
        let ch = try PairChallenge.decode(try envelope(fresh["pair_challenge"]!).body)
        #expect(ch.serverNonce == pairing("server_nonce_hex"))
        #expect(ch.timeoutS == 60)
        let res = try PairResult.decode(try envelope(fresh["pair_result"]!).body)
        #expect(res.allowed)
        #expect(res.token == pairing("token_hex"))
        #expect(try Config.decode(try envelope(fresh["config"]!).body) == Config())
        let re = section("reconnect")
        #expect(try HelloAck.decode(try envelope(re["hello_ack"]!).body).state == AckState.authenticated)
    }

    @Test func malformedInputIsRejectedNotCrashed() throws {
        #expect(throws: ProtoError.self) { try Frames.length(Data([0, 0, 0, 0])) }
        #expect(throws: ProtoError.self) { try Frames.length(Data([0x7f, 0, 0, 0])) }
        #expect(throws: ProtoError.self) { try Envelope.decode(Data([0x0a, 0x7f, 1])) }
        #expect(throws: ProtoError.self) { try Envelope.decode(Data()) }
        var w = ProtoWriter(); w.int(99, 5); w.string(1, "a"); w.string(2, "b")
        var r = ProtoReader(w.data), name = "", value = ""
        while try r.next() { switch r.field { case 1: name = try r.string(); case 2: value = try r.string(); default: try r.skip() } }
        #expect(name == "a" && value == "b", "unknown fields are skipped")
    }
}
