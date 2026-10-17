#if DEBUG
import Foundation

/// Messages of `proto/sdk.proto`, protocol version 1. Field numbers must match that file.
let protocolVersion = 1

enum Part { static let request = 0, response = 1 }
enum Encoding { static let none = 0, deflate = 1 }
enum AckState { static let authenticated = 0, pairingRequired = 1, denied = 2, ingestDisabled = 3 }
enum ErrorCode { static let other = 0, protocolViolation = 1, unsupportedVersion = 2, rateLimited = 3, tooManySessions = 4, frameTooLarge = 5 }
enum RedactionLevel { static let `default` = 0, strict = 1, none = 2 }

struct Header: Equatable {
    var name: String, value: String
    func encode() -> Data { var w = ProtoWriter(); w.string(1, name); w.string(2, value); return w.data }
}

struct Timings: Equatable {
    var dns = -1, connect = -1, tls = -1, requestSent = -1, ttfb = -1, download = -1
    func encode() -> Data {
        var w = ProtoWriter()
        w.int(1, dns); w.int(2, connect); w.int(3, tls); w.int(4, requestSent); w.int(5, ttfb); w.int(6, download)
        return w.data
    }
}

struct Hello {
    var protocolVersion = ProxyInspector.protocolVersion
    var sdkName = "", sdkVersion = "", appId = "", appName = "", appVersion = "", build = ""
    var deviceInstallId = "", deviceName = "", deviceModel = "", osVersion = ""
    var deviceTimeMs: Int64 = 0
    var token = Data()
    var connection = ""
    func encode() -> Data {
        var w = ProtoWriter()
        w.int(1, protocolVersion); w.string(2, sdkName); w.string(3, sdkVersion); w.string(4, appId); w.string(5, appName)
        w.string(6, appVersion); w.string(7, build); w.string(8, deviceInstallId); w.string(9, deviceName)
        w.string(10, deviceModel); w.string(11, osVersion); w.int(12, deviceTimeMs); w.bytes(13, token); w.string(14, connection)
        return w.data
    }
}

struct HelloAck {
    var state = 0, desktopName = "", sessionId = "", serverTimeMs: Int64 = 0, retryAfterS = 0, message = ""
    static func decode(_ d: Data) throws -> HelloAck {
        var r = ProtoReader(d), a = HelloAck()
        while try r.next() {
            switch r.field {
            case 1: a.state = try r.int(); case 2: a.desktopName = try r.string(); case 3: a.sessionId = try r.string()
            case 4: a.serverTimeMs = try r.int64(); case 5: a.retryAfterS = try r.int(); case 6: a.message = try r.string()
            default: try r.skip()
            }
        }
        return a
    }
}

struct PairChallenge {
    var serverNonce = Data(), timeoutS = 0
    static func decode(_ d: Data) throws -> PairChallenge {
        var r = ProtoReader(d), c = PairChallenge()
        while try r.next() { switch r.field { case 1: c.serverNonce = try r.bytes(); case 2: c.timeoutS = try r.int(); default: try r.skip() } }
        return c
    }
}

struct PairResult {
    var allowed = false, token = Data(), message = ""
    static func decode(_ d: Data) throws -> PairResult {
        var r = ProtoReader(d), p = PairResult()
        while try r.next() { switch r.field { case 1: p.allowed = try r.bool(); case 2: p.token = try r.bytes(); case 3: p.message = try r.string(); default: try r.skip() } }
        return p
    }
}

struct CallStart {
    var callId = "", startedAtMs: Int64 = 0, method = "", url = "", headers: [Header] = [], httpVersion = ""
    var clientStack = "", routeTemplate = "", name = "", owner = "", tag = "", screen = ""
    var requestBodyTruncated = false, requestBodySize: Int64 = 0
    func encode() -> Data {
        var w = ProtoWriter()
        w.string(1, callId); w.int(2, startedAtMs); w.string(3, method); w.string(4, url)
        for h in headers { w.message(5, h.encode()) }
        w.string(6, httpVersion); w.string(7, clientStack); w.string(8, routeTemplate); w.string(9, name)
        w.string(10, owner); w.string(11, tag); w.string(12, screen); w.bool(13, requestBodyTruncated); w.int(14, requestBodySize)
        return w.data
    }
}

struct BodyChunk {
    var callId = "", part = 0, seq = 0, data = Data(), encoding = 0, last = false, truncated = false
    func encode() -> Data {
        var w = ProtoWriter()
        w.string(1, callId); w.int(2, part); w.int(3, seq); w.bytes(4, data); w.int(5, encoding); w.bool(6, last); w.bool(7, truncated)
        return w.data
    }
}

struct ResponseStart {
    var callId = "", status = 0, headers: [Header] = [], httpVersion = "", responseBodySize: Int64 = 0
    func encode() -> Data {
        var w = ProtoWriter()
        w.string(1, callId); w.int(2, status)
        for h in headers { w.message(3, h.encode()) }
        w.string(4, httpVersion); w.int(5, responseBodySize)
        return w.data
    }
}

struct CallComplete {
    var callId = "", endedAtMs: Int64 = 0, timings: Timings? = nil, fromCache = false
    var responseBodyAvailable = false, requestBodyAvailable = false, requestRedacted = false, responseRedacted = false
    var responseBodySkipped = false, requestBodySkipped = false, responseBodySize: Int64 = 0, requestBodySize: Int64 = 0
    func encode() -> Data {
        var w = ProtoWriter()
        w.string(1, callId); w.int(2, endedAtMs)
        if let t = timings { w.message(3, t.encode()) }
        w.bool(4, fromCache); w.bool(5, responseBodyAvailable); w.bool(6, requestBodyAvailable); w.bool(7, requestRedacted)
        w.bool(8, responseRedacted); w.bool(9, responseBodySkipped); w.bool(10, requestBodySkipped)
        w.int(11, responseBodySize); w.int(12, requestBodySize)
        return w.data
    }
}

struct CallFailed {
    var callId = "", kind = "", message = "", timings: Timings? = nil, endedAtMs: Int64 = 0
    func encode() -> Data {
        var w = ProtoWriter()
        w.string(1, callId); w.string(2, kind); w.string(3, message)
        if let t = timings { w.message(4, t.encode()) }
        w.int(5, endedAtMs)
        return w.data
    }
}

struct Config: Equatable {
    var captureEnabled = true, hostAllow: [String] = [], hostDeny: [String] = []
    var bodyCapBytes = 1 << 20, eagerBodyBytes = 64 << 10, redaction = RedactionLevel.default, extraRedactKeys: [String] = []
    func encode() -> Data {
        var w = ProtoWriter()
        w.bool(1, captureEnabled); hostAllow.forEach { w.string(2, $0) }; hostDeny.forEach { w.string(3, $0) }
        w.int(4, bodyCapBytes); w.int(5, eagerBodyBytes); w.int(6, redaction); extraRedactKeys.forEach { w.string(7, $0) }
        return w.data
    }
    static func decode(_ d: Data) throws -> Config {
        var r = ProtoReader(d)
        var c = Config(captureEnabled: false, bodyCapBytes: 0, eagerBodyBytes: 0)
        while try r.next() {
            switch r.field {
            case 1: c.captureEnabled = try r.bool(); case 2: c.hostAllow.append(try r.string()); case 3: c.hostDeny.append(try r.string())
            case 4: c.bodyCapBytes = try r.int(); case 5: c.eagerBodyBytes = try r.int(); case 6: c.redaction = try r.int()
            case 7: c.extraRedactKeys.append(try r.string())
            default: try r.skip()
            }
        }
        return c
    }
}

struct BodyFetch {
    var callId = "", part = 0
    static func decode(_ d: Data) throws -> BodyFetch {
        var r = ProtoReader(d), f = BodyFetch()
        while try r.next() { switch r.field { case 1: f.callId = try r.string(); case 2: f.part = try r.int(); default: try r.skip() } }
        return f
    }
}

struct BodyFetchResult {
    var callId = "", part = 0, available = false
    func encode() -> Data { var w = ProtoWriter(); w.string(1, callId); w.int(2, part); w.bool(3, available); return w.data }
}

struct Ping {
    var timeMs: Int64 = 0
    func encode() -> Data { var w = ProtoWriter(); w.int(1, timeMs); return w.data }
    static func decode(_ d: Data) throws -> Ping {
        var r = ProtoReader(d), p = Ping()
        while try r.next() { if r.field == 1 { p.timeMs = try r.int64() } else { try r.skip() } }
        return p
    }
}

struct Pong {
    var echoMs: Int64 = 0, timeMs: Int64 = 0
    func encode() -> Data { var w = ProtoWriter(); w.int(1, echoMs); w.int(2, timeMs); return w.data }
}

struct WireError {
    var code = 0, message = "", minVersion = 0, maxVersion = 0
    static func decode(_ d: Data) throws -> WireError {
        var r = ProtoReader(d), e = WireError()
        while try r.next() {
            switch r.field { case 1: e.code = try r.int(); case 2: e.message = try r.string(); case 3: e.minVersion = try r.int(); case 4: e.maxVersion = try r.int(); default: try r.skip() }
        }
        return e
    }
}

/// Which field of the Envelope's oneof is set, and its bytes.
struct Envelope {
    var kind: Int, body: Data
    static let hello = 1, helloAck = 2, pairRequest = 3, pairChallenge = 4, pairReveal = 5, pairResult = 6, config = 7
    static let callStart = 8, bodyChunk = 9, responseStart = 10, callComplete = 11, callFailed = 12
    static let bodyFetch = 13, bodyFetchResult = 14, ping = 15, pong = 16, error = 17

    func encode() -> Data { var w = ProtoWriter(); w.message(kind, body); return w.data }

    static func decode(_ d: Data) throws -> Envelope {
        var r = ProtoReader(d)
        var found: Envelope?
        while try r.next() {
            if r.wire == 2 && (hello...error).contains(r.field) { found = Envelope(kind: r.field, body: try r.bytes()) } else { try r.skip() }
        }
        guard let f = found else { throw ProtoError("envelope has no message") }
        return f
    }
}

/// 4 bytes big-endian length, then one Envelope.
enum Frames {
    static let maxFrame = 1 << 20

    static func encode(_ e: Envelope) -> Data {
        let body = e.encode()
        var out = Data(capacity: 4 + body.count)
        let n = UInt32(body.count)
        out.append(contentsOf: [UInt8(n >> 24), UInt8((n >> 16) & 0xFF), UInt8((n >> 8) & 0xFF), UInt8(n & 0xFF)])
        out.append(body)
        return out
    }

    /// The frame length from a 4 byte header, validated.
    static func length(_ header: Data) throws -> Int {
        let b = [UInt8](header)
        let n = Int(b[0]) << 24 | Int(b[1]) << 16 | Int(b[2]) << 8 | Int(b[3])
        if n <= 0 || n > maxFrame { throw ProtoError("bad frame length \(n)") }
        return n
    }
}
#endif
