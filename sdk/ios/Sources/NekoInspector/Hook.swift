import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// Timings of one call in milliseconds, -1 when unknown. For capture integrations.
public struct InspectorTimings {
    public var dns = -1, connect = -1, tls = -1, requestSent = -1, ttfb = -1, download = -1
    public init() {}
}

/// One call being recorded, for capture integrations such as `NekoInspectorAlamofire`.
/// In Release builds every method does nothing.
public final class InspectorCall {
    #if DEBUG
    let handle: CallHandle
    init(_ h: CallHandle) { handle = h }
    #endif
    public func requestBody(_ data: Data) {
        #if DEBUG
        handle.onRequestBody(data)
        #endif
    }

    public func responseStarted(status: Int, headers: [(String, String)], httpVersion: String, size: Int64) {
        #if DEBUG
        handle.responseStarted(status: status, headers: headers, httpVersion: httpVersion, size: size)
        #endif
    }

    public func responseBody(_ data: Data) {
        #if DEBUG
        handle.onResponseBody(data)
        #endif
    }

    public func complete(timings: InspectorTimings?, fromCache: Bool = false) {
        #if DEBUG
        handle.complete(timings: timings.map(Self.wire), fromCache: fromCache)
        #endif
    }

    public func fail(kind: String, message: String, timings: InspectorTimings? = nil) {
        #if DEBUG
        handle.fail(kind: kind, message: message, timings: timings.map(Self.wire))
        #endif
    }

    #if DEBUG
    private static func wire(_ t: InspectorTimings) -> Timings {
        Timings(dns: t.dns, connect: t.connect, tls: t.tls, requestSent: t.requestSent, ttfb: t.ttfb, download: t.download)
    }
    #endif
}

public enum InspectorHook {
    /// Starts recording a call. Nil when nothing is captured (no desktop connected, or the host is filtered).
    public static func begin(method: String, url: String, headers: [(String, String)], clientStack: String,
                             template: String = "", name: String = "", owner: String = "", tag: String = "",
                             requestBodySize: Int64 = 0) -> InspectorCall? {
        #if DEBUG
        var info = CallInfo(method: method, url: url, headers: headers, clientStack: clientStack)
        info.routeTemplate = template; info.name = name; info.owner = owner; info.tag = tag; info.requestBodySize = requestBodySize
        return Recorder.begin(info).map(InspectorCall.init)
        #else
        return nil
        #endif
    }

    /// True while a desktop is connected and capture is on. One atomic read.
    public static var isActive: Bool {
        #if DEBUG
        return Recorder.active()
        #else
        return false
        #endif
    }
}

extension InspectorTimings {
    /// Maps one URLSession transaction's metrics to the protocol's phases.
    public init(_ m: URLSessionTaskTransactionMetrics) {
        func ms(_ a: Date?, _ b: Date?) -> Int { guard let a = a, let b = b, b >= a else { return -1 }; return Int((b.timeIntervalSince(a) * 1000).rounded()) }
        let reused = m.isReusedConnection
        dns = reused ? -1 : ms(m.domainLookupStartDate, m.domainLookupEndDate)
        let tlsMs = ms(m.secureConnectionStartDate, m.secureConnectionEndDate)
        let c = ms(m.connectStartDate, m.connectEndDate)
        connect = reused ? 0 : (c >= 0 && tlsMs > 0 ? max(c - tlsMs, 0) : c)
        tls = m.secureConnectionStartDate == nil ? -1 : tlsMs
        requestSent = ms(m.requestStartDate, m.requestEndDate)
        ttfb = ms(m.requestEndDate, m.responseStartDate)
        download = ms(m.responseStartDate, m.responseEndDate)
    }
}
