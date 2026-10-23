import Foundation

/// Form state for the breakpoint and rule editors, and the one-line descriptions
/// shown in lists. Ported from `desktop/src/edit.rs` so both apps behave alike.
/// Validation of the rules themselves happens in the core; this only turns text
/// into fields.
public typealias Header = Neko_Api_V1_Header
public typealias Rule = Neko_Api_V1_Rule
public typealias PausedFlow = Neko_Api_V1_PausedFlow

public func headersToText(_ h: [Header]) -> String {
    h.map { "\($0.name): \($0.value)" }.joined(separator: "\n")
}

/// `Name: value` per line. Blank lines and lines without a colon are skipped.
public func textToHeaders(_ t: String) -> [Header] {
    t.split(whereSeparator: \.isNewline).compactMap { line in
        guard let i = line.firstIndex(of: ":") else { return nil }
        let name = line[..<i].trimmingCharacters(in: .whitespaces)
        if name.isEmpty { return nil }
        return Header.with {
            $0.name = name
            $0.value = line[line.index(after: i)...].trimmingCharacters(in: .whitespaces)
        }
    }
}

// MARK: breakpoint editor

public struct BreakpointEdit: Equatable {
    public var flowID: UInt64
    public var isRequest: Bool
    public var method: String
    public var path: String
    public var status: String
    public var headers: String
    public var body: String
    /// The body is not text; it is passed through untouched.
    public var binary: Bool

    public init(_ p: PausedFlow) {
        flowID = p.flowID
        isRequest = p.phase == .phaseRequest
        method = p.method
        path = p.path
        status = p.status == 0 ? "" : String(p.status)
        headers = headersToText(p.headers)
        if let s = String(data: p.body, encoding: .utf8) { body = s; binary = false } else { body = ""; binary = true }
    }

    /// Only the fields that differ from what was held.
    public func resume(_ orig: PausedFlow) -> Neko_Api_V1_ResumeRequest {
        var r = Neko_Api_V1_ResumeRequest()
        r.flowID = flowID
        let trim = { (s: String) in s.trimmingCharacters(in: .whitespaces) }
        if isRequest {
            if trim(method) != orig.method { r.method = trim(method) }
            if trim(path) != orig.path { r.path = trim(path) }
        } else if let s = UInt32(trim(status)), s != orig.status {
            r.status = s
        }
        let h = textToHeaders(headers)
        if h != orig.headers { r.replaceHeaders = true; r.headers = h }
        if !binary && Data(body.utf8) != orig.body { r.body = Data(body.utf8) }
        return r
    }

    public func changed(_ orig: PausedFlow) -> Bool {
        let r = resume(orig)
        return r.hasMethod || r.hasPath || r.hasStatus || r.replaceHeaders || r.hasBody
    }
}

// MARK: rule editor

public enum RuleKind: Int, CaseIterable {
    case mock, mapRemote, rewrite, throttle, breakpoint

    public var label: String { ["Mock", "Map remote", "Rewrite", "Throttle", "Breakpoint"][rawValue] }

    public var hint: String {
        [
            "Answer with a fixed response. The server is never contacted.",
            "Send the request to another host, port or scheme.",
            "Add, set or remove headers on the way out or back.",
            "Delay the request before it goes upstream.",
            "Hold the request or response until you resume it.",
        ][rawValue]
    }

    public init?(_ r: Rule) {
        switch r.action {
        case .mock?: self = .mock
        case .mapRemote?: self = .mapRemote
        case .rewrite?: self = .rewrite
        case .throttle?: self = .throttle
        case .breakpoint?: self = .breakpoint
        case nil: return nil
        }
    }
}

public struct RuleForm: Equatable {
    public var id = ""
    public var name = ""
    public var enabled = true
    public var kind: RuleKind
    public var scheme = "", host = "", port = "", method = "", path = "", query = ""
    public var status = "200", headers = "content-type: application/json", body = "", delayMs = ""
    public var toScheme = "", toHost = "", toPort = ""
    public var requestEdits = "", responseEdits = ""
    /// 0 both, 1 request, 2 response.
    public var phase = 0
    public var timeoutS = ""

    public init(kind: RuleKind) { self.kind = kind }

    /// A form pre-filled to match a flow's request.
    public init(kind: RuleKind, flow f: Neko_Api_V1_FlowSummary) {
        self.kind = kind
        scheme = f.scheme
        host = f.host
        method = f.method
        path = String(f.path.split(separator: "?", maxSplits: 1, omittingEmptySubsequences: false).first ?? "")
        let def: UInt32 = f.scheme == "https" ? 443 : 80
        if f.port != 0 && f.port != def { port = String(f.port) }
    }

    public init(_ r: Rule) {
        kind = RuleKind(r) ?? .mock
        id = r.id; name = r.name; enabled = r.enabled
        let m = r.matcher
        scheme = m.scheme; host = m.host; port = m.port == 0 ? "" : String(m.port)
        method = m.method; path = m.path; query = m.query
        switch r.action {
        case .mock(let a)?:
            status = String(a.status); headers = headersToText(a.headers); body = a.body
            delayMs = a.delayMs == 0 ? "" : String(a.delayMs)
        case .mapRemote(let a)?:
            toScheme = a.scheme; toHost = a.host; toPort = a.port == 0 ? "" : String(a.port)
        case .rewrite(let a)?:
            requestEdits = editsToText(a.requestHeaders); responseEdits = editsToText(a.responseHeaders)
        case .throttle(let a)?:
            delayMs = String(a.delayMs)
        case .breakpoint(let a)?:
            phase = min(max(a.phase.rawValue, 0), 2)
            timeoutS = a.timeoutS == 0 ? "" : String(a.timeoutS)
        case nil: break
        }
    }

    public struct Invalid: Error, Equatable { public let message: String }

    public func toRule() throws -> Rule {
        func num(_ s: String, _ what: String) throws -> UInt32 {
            let t = s.trimmingCharacters(in: .whitespaces)
            if t.isEmpty { return 0 }
            guard let n = UInt32(t) else { throw Invalid(message: "\(what) must be a number") }
            return n
        }
        let trim = { (s: String) in s.trimmingCharacters(in: .whitespaces) }
        if trim(host).isEmpty { throw Invalid(message: "Enter a host (use * for any host)") }
        var rule = Rule()
        switch kind {
        case .mock:
            var a = Neko_Api_V1_MockAction()
            a.status = try num(status, "Status")
            a.headers = textToHeaders(headers)
            a.body = body
            a.delayMs = try num(delayMs, "Delay")
            rule.mock = a
        case .mapRemote:
            var a = Neko_Api_V1_MapRemoteAction()
            a.scheme = trim(toScheme); a.host = trim(toHost)
            a.port = try num(toPort, "Port")
            rule.mapRemote = a
        case .rewrite:
            var a = Neko_Api_V1_RewriteAction()
            a.requestHeaders = try textToEdits(requestEdits)
            a.responseHeaders = try textToEdits(responseEdits)
            rule.rewrite = a
        case .throttle:
            var a = Neko_Api_V1_ThrottleAction()
            a.delayMs = try num(delayMs, "Delay")
            rule.throttle = a
        case .breakpoint:
            var a = Neko_Api_V1_BreakpointAction()
            a.phase = Neko_Api_V1_BreakpointAction.Phase(rawValue: phase) ?? .both
            a.timeoutS = try num(timeoutS, "Timeout")
            rule.breakpoint = a
        }
        rule.id = id; rule.name = trim(name); rule.enabled = enabled
        var m = Neko_Api_V1_RuleMatch()
        m.scheme = trim(scheme); m.host = trim(host); m.method = trim(method)
        m.path = trim(path); m.query = trim(query)
        m.port = try num(port, "Port")
        rule.matcher = m
        return rule
    }
}

public func editsToText(_ e: [Neko_Api_V1_HeaderEdit]) -> String {
    e.map { e in
        switch e.op {
        case .add: return "add \(e.name): \(e.value)"
        case .remove: return "remove \(e.name)"
        default: return "set \(e.name): \(e.value)"
        }
    }.joined(separator: "\n")
}

public func textToEdits(_ t: String) throws -> [Neko_Api_V1_HeaderEdit] {
    var out: [Neko_Api_V1_HeaderEdit] = []
    for (i, raw) in t.split(whereSeparator: \.isNewline).enumerated() {
        let line = raw.trimmingCharacters(in: .whitespaces)
        if line.isEmpty { continue }
        let bad = RuleForm.Invalid(message: "Line \(i + 1): use `set Name: value`, `add Name: value` or `remove Name`")
        guard let sp = line.firstIndex(where: \.isWhitespace) else { throw bad }
        let verb = line[..<sp].lowercased()
        let rest = line[sp...].trimmingCharacters(in: .whitespaces)
        var e = Neko_Api_V1_HeaderEdit()
        switch verb {
        case "remove", "delete":
            e.op = .remove; e.name = rest
        case "set", "add":
            guard let c = rest.firstIndex(of: ":") else { throw bad }
            e.op = verb == "set" ? .set : .add
            e.name = rest[..<c].trimmingCharacters(in: .whitespaces)
            e.value = rest[rest.index(after: c)...].trimmingCharacters(in: .whitespaces)
        default: throw bad
        }
        if e.name.isEmpty { throw bad }
        out.append(e)
    }
    return out
}

/// One-line description of a rule's target, for lists and prompts.
public func describeMatch(_ m: Neko_Api_V1_RuleMatch) -> String {
    var s = ""
    if !m.method.isEmpty { s += m.method + " " }
    s += m.host
    if m.port != 0 { s += ":\(m.port)" }
    s += m.path.isEmpty ? "/*" : m.path
    if !m.query.isEmpty { s += "?" + m.query }
    return s
}

/// One-line description of what a rule does.
public func describeAction(_ r: Rule) -> String {
    switch r.action {
    case .mock(let a)?: return "Answer \(a.status)" + (a.delayMs > 0 ? ", after \(a.delayMs) ms" : "")
    case .mapRemote(let a)?:
        var to = a.scheme.isEmpty ? "" : a.scheme + "://"
        to += a.host.isEmpty ? "(same host)" : a.host
        if a.port != 0 { to += ":\(a.port)" }
        return "Send to \(to)"
    case .rewrite(let a)?:
        let n = a.requestHeaders.count + a.responseHeaders.count
        return "Edit \(n) header\(n == 1 ? "" : "s")"
    case .throttle(let a)?: return "Delay \(a.delayMs) ms"
    case .breakpoint(let a)?:
        switch a.phase {
        case .request: return "Hold request"
        case .response: return "Hold response"
        default: return "Hold request and response"
        }
    case nil: return ""
    }
}
