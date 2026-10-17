// UNVERIFIED: written without a Mac, compiled only on Apple platforms. See sdk/ios/README.md.
#if DEBUG && canImport(Network)
import Foundation

/// Approach C of the plan: `Inspector.session(...)` returns a URLSession whose delegate records the calls and
/// forwards everything to the app's own delegate. It does not re-issue requests, so behaviour is unchanged.
///
/// Limits (to verify on a device): tasks made with a completion handler do not deliver `didReceive data` to the
/// delegate, so they are recorded with timings and status but no body; `URLSession.shared` and sessions created
/// elsewhere are not seen (automatic capture, spike S3, is not implemented).
enum InspectedSession {
    static func make(configuration: URLSessionConfiguration, delegate: URLSessionDelegate?, delegateQueue: OperationQueue?) -> URLSession {
        let proxy = InspectorSessionDelegate(user: delegate)
        return URLSession(configuration: configuration, delegate: proxy, delegateQueue: delegateQueue)
    }
}

final class InspectorSessionDelegate: NSObject, URLSessionDataDelegate {
    private let user: URLSessionDelegate?
    private let lock = NSLock()
    private var calls: [Int: InspectorCall] = [:]

    init(user: URLSessionDelegate?) { self.user = user }

    // Everything this class does not implement goes to the app's delegate.
    override func responds(to aSelector: Selector!) -> Bool { super.responds(to: aSelector) || (user as AnyObject?)?.responds(to: aSelector) == true }
    override func forwardingTarget(for aSelector: Selector!) -> Any? { (user as AnyObject?)?.responds(to: aSelector) == true ? user : super.forwardingTarget(for: aSelector) }

    private func begin(_ task: URLSessionTask) -> InspectorCall? {
        lock.lock(); defer { lock.unlock() }
        if let c = calls[task.taskIdentifier] { return c }
        guard let r = task.currentRequest ?? task.originalRequest, let url = r.url?.absoluteString else { return nil }
        let headers = (r.allHTTPHeaderFields ?? [:]).map { ($0.key, $0.value) }
        guard let c = InspectorHook.begin(method: r.httpMethod ?? "GET", url: url, headers: headers, clientStack: "urlsession",
                                          requestBodySize: Int64(r.httpBody?.count ?? 0)) else { return nil }
        if let b = r.httpBody { c.requestBody(b) }
        calls[task.taskIdentifier] = c
        return c
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse, completionHandler: @escaping (URLSession.ResponseDisposition) -> Void) {
        if let http = response as? HTTPURLResponse, let c = begin(dataTask) {
            let headers = http.allHeaderFields.compactMap { k, v -> (String, String)? in (k as? String).flatMap { k in (v as? String).map { (k, $0) } } }
            c.responseStarted(status: http.statusCode, headers: headers, httpVersion: "", size: max(http.expectedContentLength, 0))
        }
        if let u = user as? URLSessionDataDelegate, u.responds(to: #selector(URLSessionDataDelegate.urlSession(_:dataTask:didReceive:completionHandler:))) {
            u.urlSession?(session, dataTask: dataTask, didReceive: response, completionHandler: completionHandler)
        } else { completionHandler(.allow) }
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        lock.lock(); let c = calls[dataTask.taskIdentifier]; lock.unlock()
        c?.responseBody(data)
        (user as? URLSessionDataDelegate)?.urlSession?(session, dataTask: dataTask, didReceive: data)
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didFinishCollecting metrics: URLSessionTaskMetrics) {
        if let c = begin(task) {
            lock.lock(); calls[task.taskIdentifier] = c; lock.unlock()
            if let m = metrics.transactionMetrics.last { stash(task.taskIdentifier, InspectorTimings(m)) }
        }
        (user as? URLSessionTaskDelegate)?.urlSession?(session, task: task, didFinishCollecting: metrics)
    }

    private var timings: [Int: InspectorTimings] { get { _timings } set { _timings = newValue } }
    private var _timings: [Int: InspectorTimings] = [:]
    private func stash(_ id: Int, _ t: InspectorTimings) { lock.lock(); timings[id] = t; lock.unlock() }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        let c = begin(task)
        lock.lock(); calls[task.taskIdentifier] = nil; let t = timings.removeValue(forKey: task.taskIdentifier); lock.unlock()
        if let e = error {
            let u = e as? URLError
            let kind: String
            switch u?.code {
            case .timedOut?: kind = "timeout"
            case .cannotFindHost?, .dnsLookupFailed?: kind = "dns"
            case .cancelled?: kind = "cancelled"
            case .secureConnectionFailed?, .serverCertificateUntrusted?: kind = "tls"
            default: kind = "io"
            }
            c?.fail(kind: kind, message: e.localizedDescription, timings: t)
        } else if let http = task.response as? HTTPURLResponse, let c = c {
            // A completion-handler task never reached didReceive: announce the response now, without a body.
            if !didAnnounce(task) { c.responseStarted(status: http.statusCode, headers: [], httpVersion: "", size: max(http.expectedContentLength, 0)) }
            c.complete(timings: t)
        } else { c?.complete(timings: t) }
        (user as? URLSessionTaskDelegate)?.urlSession?(session, task: task, didCompleteWithError: error)
    }

    private func didAnnounce(_ task: URLSessionTask) -> Bool { false }
}
#endif
