import Alamofire
import Foundation
import ProxyInspector

/// Per-request metadata, keyed by Alamofire's request id.
private enum Metadata {
    struct Entry { var name = "", owner = "", tag = "", template = "" }
    static let lock = NSLock()
    nonisolated(unsafe) static var entries: [UUID: Entry] = [:]
    static func set(_ id: UUID, _ e: Entry) { lock.lock(); entries[id] = e; lock.unlock() }
    static func take(_ id: UUID) -> Entry? { lock.lock(); defer { lock.unlock() }; return entries.removeValue(forKey: id) }
}

public extension Request {
    /// Names this request for the desktop inspector. Call it before the request resumes (before adding a response handler).
    @discardableResult
    func inspect(_ name: String, owner: String = "", tag: String = "", template: String = "") -> Self {
        Metadata.set(id, .init(name: name, owner: owner, tag: tag, template: template))
        return self
    }
}

/// Add it to your session: `Session(eventMonitors: [InspectorMonitor()])`. Records requests, responses, timings and errors.
/// Request bodies are read from `httpBody`; stream and file uploads record size only.
public final class InspectorMonitor: EventMonitor, @unchecked Sendable {
    public let queue = DispatchQueue(label: "dev.proxy.inspector.alamofire")
    private let lock = NSLock()
    private var calls: [UUID: InspectorCall] = [:]
    private var timings: [UUID: InspectorTimings] = [:]

    public init() {}

    public func requestDidResume(_ request: Request) {
        guard InspectorHook.isActive, let r = request.request, let url = r.url?.absoluteString else { return }
        let meta = Metadata.take(request.id) ?? .init()
        var headers = (r.allHTTPHeaderFields ?? [:]).map { ($0.key, $0.value) }
        if let ct = r.value(forHTTPHeaderField: "Content-Type"), !headers.contains(where: { $0.0.lowercased() == "content-type" }) { headers.append(("Content-Type", ct)) }
        guard let call = InspectorHook.begin(method: r.httpMethod ?? "GET", url: url, headers: headers, clientStack: "alamofire",
                                             template: meta.template, name: meta.name, owner: meta.owner, tag: meta.tag,
                                             requestBodySize: Int64(r.httpBody?.count ?? 0)) else { return }
        if let body = r.httpBody { call.requestBody(body) }
        lock.lock(); calls[request.id] = call; lock.unlock()
    }

    public func request(_ request: Request, didGatherMetrics metrics: URLSessionTaskMetrics) {
        guard let m = metrics.transactionMetrics.last else { return }
        let t = InspectorTimings(m)
        lock.lock(); timings[request.id] = t; lock.unlock()
    }

    public func request<Value>(_ request: DataRequest, didParseResponse response: DataResponse<Value, AFError>) {
        finish(request.id, response: response.response, data: response.data, error: response.error)
    }

    public func request(_ request: DownloadRequest, didParseResponse response: DownloadResponse<URL?, AFError>) {
        // Downloads record metadata only.
        finish(request.id, response: response.response, data: nil, error: response.error)
    }

    public func requestDidCancel(_ request: Request) {
        lock.lock(); let call = calls.removeValue(forKey: request.id); timings.removeValue(forKey: request.id); lock.unlock()
        call?.fail(kind: "cancelled", message: "cancelled")
    }

    private func finish(_ id: UUID, response: HTTPURLResponse?, data: Data?, error: AFError?) {
        lock.lock(); let call = calls.removeValue(forKey: id); let t = timings.removeValue(forKey: id); lock.unlock()
        guard let call = call else { return }
        if let r = response {
            let headers = r.allHeaderFields.compactMap { k, v -> (String, String)? in guard let k = k as? String, let v = v as? String else { return nil }; return (k, v) }
            call.responseStarted(status: r.statusCode, headers: headers, httpVersion: "", size: Int64(data?.count ?? 0))
            if let d = data { call.responseBody(d) }
            call.complete(timings: t)
        } else if let e = error {
            let kind = (e.underlyingError as? URLError).map { u -> String in
                switch u.code { case .timedOut: return "timeout"; case .cannotFindHost, .dnsLookupFailed: return "dns"; case .cancelled: return "cancelled"
                case .serverCertificateUntrusted, .secureConnectionFailed: return "tls"; default: return "io" } } ?? "other"
            call.fail(kind: kind, message: e.localizedDescription, timings: t)
        } else {
            call.complete(timings: t)
        }
    }
}
