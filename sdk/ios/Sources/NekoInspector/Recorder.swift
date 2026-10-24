#if DEBUG
import Foundation

/// What a capture mechanism knows when a call starts.
struct CallInfo {
    var method: String, url: String, headers: [(String, String)], clientStack: String
    var routeTemplate = "", name = "", owner = "", tag = "", requestBodySize: Int64 = 0, httpVersion = ""
}

enum Runtime {
    static let engineBox = Locked<Engine?>(nil)
    static var engine: Engine? { engineBox.value }
    static var live: Live? { engine?.live }
}

enum Recorder {
    /// One atomic read: the whole cost of a request while no desktop is connected.
    static func active() -> Bool { Runtime.live != nil }

    static func begin(_ info: CallInfo) -> CallHandle? {
        guard let live = Runtime.live else { return nil }
        guard live.hostAllowed(hostOf(info.url)) else { return nil }
        if isDuplicate(info) { return nil }
        return CallHandle(live: live, info: info)
    }

    static func hostOf(_ url: String) -> String {
        URLComponents(string: url)?.host ?? ""
    }

    // Alamofire's monitor and the URLSession capture can both see a call. The Alamofire record carries the
    // metadata, so it wins: a URLSession call with the same method and URL within 50 ms of one is dropped.
    private static let marks = Locked<[(String, String, Date)]>([])

    private static func isDuplicate(_ info: CallInfo) -> Bool {
        marks.mutate { m in
            let now = Date()
            m.removeAll { now.timeIntervalSince($0.2) > 0.05 }
            if info.clientStack == "alamofire" { m.append((info.method, info.url, now)); return false }
            if info.clientStack != "urlsession" { return false }
            if let i = m.firstIndex(where: { $0.0 == info.method && $0.1 == info.url }) { m.remove(at: i); return true }
            return false
        }
    }
}

/// One call in flight. Methods run on the app's thread and only snapshot and copy; serialization,
/// redaction and compression happen on the sender thread.
final class CallHandle {
    let callId = UUID().uuidString.lowercased()
    private let live: Live
    private let engine: Engine
    private let startedAt = Date()
    private let finished = Locked<Bool>(false)
    private var reqTee: Tee?, resTee: Tee?
    private var reqCT = "", resCT = ""
    private var reqSkipped: Bool, resSkipped: Bool
    private var resSize: Int64 = 0

    init(live: Live, info: CallInfo) {
        self.live = live; engine = live.engine
        reqSkipped = !live.redactor.bodiesAllowed; resSkipped = reqSkipped
        reqCT = info.headers.first { $0.0.lowercased() == "content-type" }?.1 ?? ""
        reqSkipped = reqSkipped || Self.skipType(reqCT, info.requestBodySize)
        if !reqSkipped { reqTee = Tee(cap: live.config.bodyCapBytes) }
        let screen = engine.screen, r = live.redactor.fork(), started = Int64(startedAt.timeIntervalSince1970 * 1000), id = callId
        engine.emit { sink in
            var cs = CallStart()
            cs.callId = id; cs.startedAtMs = started; cs.method = info.method; cs.url = r.url(info.url); cs.headers = r.headers(info.headers)
            cs.httpVersion = info.httpVersion; cs.clientStack = info.clientStack; cs.routeTemplate = info.routeTemplate; cs.name = info.name
            cs.owner = info.owner; cs.tag = info.tag; cs.screen = screen; cs.requestBodySize = info.requestBodySize
            try sink(Envelope(kind: Envelope.callStart, body: cs.encode()))
        }
    }

    func onRequestBody(_ d: Data) { reqTee?.write(d) }

    func responseStarted(status: Int, headers: [(String, String)], httpVersion: String, size: Int64) {
        resCT = headers.first { $0.0.lowercased() == "content-type" }?.1 ?? ""
        resSize = size
        resSkipped = resSkipped || Self.skipType(resCT, size)
        if !resSkipped { resTee = Tee(cap: live.config.bodyCapBytes) }
        let r = live.redactor.fork(), id = callId
        engine.emit { sink in
            var rs = ResponseStart()
            rs.callId = id; rs.status = status; rs.headers = r.headers(headers); rs.httpVersion = httpVersion; rs.responseBodySize = size
            try sink(Envelope(kind: Envelope.responseStart, body: rs.encode()))
        }
    }

    func onResponseBody(_ d: Data) { resTee?.write(d) }

    func complete(timings: Timings?, fromCache: Bool) {
        guard finished.mutate({ let was = $0; $0 = true; return !was }) else { return }
        let ended = Int64(Date().timeIntervalSince1970 * 1000)
        let r = live.redactor.fork(), eager = live.config.eagerBodyBytes, id = callId, engine = self.engine
        let req = reqTee, res = resTee, reqCT = self.reqCT, resCT = self.resCT, reqSk = reqSkipped, resSk = resSkipped
        let resTotal = res?.total ?? resSize, reqTotal = req?.total ?? 0
        engine.emit { sink in
            var reqAvail = true, resAvail = true
            if let t = req, !t.data.isEmpty {
                let body = r.body(contentType: reqCT, t.data)
                if body.count <= eager { try Bodies.sendChunks(sink, callId: id, part: Part.request, body: body, firstSeq: 0, last: true, truncated: t.truncated) }
                else { engine.ring.put(id, Part.request, body); reqAvail = false }
            }
            if let t = res, !t.data.isEmpty {
                let body = r.body(contentType: resCT, t.data)
                if body.count <= eager { try Bodies.sendChunks(sink, callId: id, part: Part.response, body: body, firstSeq: 0, last: true, truncated: t.truncated) }
                else { engine.ring.put(id, Part.response, body); resAvail = false }
            }
            var cc = CallComplete()
            cc.callId = id; cc.endedAtMs = ended; cc.timings = timings; cc.fromCache = fromCache
            cc.responseBodyAvailable = resAvail; cc.requestBodyAvailable = reqAvail
            cc.requestRedacted = r.changed; cc.responseRedacted = r.changed
            cc.responseBodySkipped = resSk; cc.requestBodySkipped = reqSk
            cc.responseBodySize = resTotal; cc.requestBodySize = reqTotal
            try sink(Envelope(kind: Envelope.callComplete, body: cc.encode()))
        }
    }

    func fail(kind: String, message: String, timings: Timings?) {
        guard finished.mutate({ let was = $0; $0 = true; return !was }) else { return }
        let ended = Int64(Date().timeIntervalSince1970 * 1000), id = callId
        engine.emit { sink in
            var cf = CallFailed()
            cf.callId = id; cf.kind = kind; cf.message = String(message.prefix(500)); cf.timings = timings; cf.endedAtMs = ended
            try sink(Envelope(kind: Envelope.callFailed, body: cf.encode()))
        }
    }

    private static func skipType(_ ct: String, _ size: Int64) -> Bool {
        let t = ct.lowercased()
        if t.hasPrefix("image/") || t.hasPrefix("video/") || t.hasPrefix("audio/") || t.hasPrefix("font/") { return true }
        if t.hasPrefix("application/octet-stream") || t.hasPrefix("application/zip") || t.hasPrefix("application/pdf") { return size < 0 || size > 64 * 1024 || size == 0 }
        return false
    }
}
#endif
