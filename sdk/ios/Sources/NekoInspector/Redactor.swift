#if DEBUG
import Foundation

/// Hides secrets before anything leaves the device. Runs on the sender queue. Same rules as the Android SDK.
final class Redactor {
    static let mark = "[redacted]"
    private let level: Int
    private let keys: Set<String>
    private(set) var changed = false

    private static let baseKeys: Set<String> = [
        "password", "passwd", "pwd", "token", "access_token", "refresh_token", "id_token", "secret", "client_secret",
        "api_key", "apikey", "authorization", "otp", "pin", "cvv", "cvc", "card_number", "cardnumber", "ssn", "session_token", "auth_token",
    ]
    private static let sensitiveHeaders: Set<String> = ["authorization", "proxy-authorization", "cookie", "set-cookie", "x-csrf-token", "x-xsrf-token"]
    private static let safeHeaders: Set<String> = [
        "accept", "accept-encoding", "accept-language", "cache-control", "content-encoding", "content-length", "content-type", "date", "etag",
        "host", "if-none-match", "last-modified", "server", "user-agent", "vary", "connection", "transfer-encoding", "age", "expires",
    ]

    init(level: Int, extraKeys: [String]) {
        self.level = level
        keys = Self.baseKeys.union(extraKeys.map { $0.lowercased().replacingOccurrences(of: "-", with: "_") })
    }

    private init(level: Int, keys: Set<String>) { self.level = level; self.keys = keys }

    /// A fresh redactor with the same policy, so each call tracks its own `changed`.
    func fork() -> Redactor { Redactor(level: level, keys: keys) }

    var bodiesAllowed: Bool { level != RedactionLevel.strict }

    func header(_ name: String, _ value: String) -> String {
        if level == RedactionLevel.none { return value }
        let n = name.lowercased()
        let hide = level == RedactionLevel.strict ? !Self.safeHeaders.contains(n) : isSensitiveHeader(n)
        if !hide { return value }
        changed = true
        return Self.mark
    }

    func headers(_ list: [(String, String)]) -> [Header] { list.map { Header(name: $0.0, value: header($0.0, $0.1)) } }

    func url(_ url: String) -> String {
        if level == RedactionLevel.none { return url }
        guard let q = url.firstIndex(of: "?") else { return url }
        let fragIdx = url[q...].firstIndex(of: "#") ?? url.endIndex
        let query = String(url[url.index(after: q)..<fragIdx])
        let out = redactPairs(query)
        return out == query ? url : String(url[...q]) + out + String(url[fragIdx...])
    }

    func body(contentType: String, _ bytes: Data) -> Data {
        if level == RedactionLevel.none || bytes.isEmpty { return bytes }
        var ct = contentType.lowercased()
        if ct.trimmingCharacters(in: .whitespaces).isEmpty, let f = bytes.first, f == UInt8(ascii: "{") || f == UInt8(ascii: "[") { ct = "application/json" }
        if ct.contains("json") {
            guard let s = String(data: bytes, encoding: .utf8) else { return bytes }
            let r = redactJson(s)
            return r == s ? bytes : Data(r.utf8)
        }
        if ct.contains("x-www-form-urlencoded") {
            guard let s = String(data: bytes, encoding: .utf8) else { return bytes }
            let r = redactPairs(s)
            return r == s ? bytes : Data(r.utf8)
        }
        return bytes
    }

    private func isSensitiveHeader(_ n: String) -> Bool {
        Self.sensitiveHeaders.contains(n) || n.contains("api-key") || n.contains("apikey") || n.contains("api_key") ||
            n.hasSuffix("-token") || n.contains("secret") || n.contains("auth")
    }

    private func isSensitiveKey(_ k: String) -> Bool { keys.contains(k.lowercased().replacingOccurrences(of: "-", with: "_")) }

    private func redactPairs(_ s: String) -> String {
        if s.isEmpty { return s }
        var hit = false
        let parts = s.split(separator: "&", omittingEmptySubsequences: false).map { p -> String in
            if let eq = p.firstIndex(of: "="), eq != p.startIndex, p.index(after: eq) != p.endIndex,
               isSensitiveKey(String(p[..<eq]).removingPercentEncoding ?? String(p[..<eq])) {
                hit = true
                return String(p[...eq]) + Self.mark
            }
            return String(p)
        }
        if !hit { return s }
        changed = true
        return parts.joined(separator: "&")
    }

    /// Replaces the value of every sensitive key. A small scanner, not a parser: it never fails on malformed JSON.
    func redactJson(_ s: String) -> String {
        let c = Array(s.unicodeScalars)
        var out = String.UnicodeScalarView()
        var copied = 0, i = 0, any = false
        let n = c.count
        func stringEnd(_ start: Int) -> Int {
            var i = start + 1
            while i < n { if c[i] == "\\" { i += 1 } else if c[i] == "\"" { return i }; i += 1 }
            return -1
        }
        func valueEnd(_ start: Int) -> Int {
            if start >= n { return start }
            switch c[start] {
            case "\"": let e = stringEnd(start); return e < 0 ? start : e + 1
            case "{", "[":
                var depth = 0, i = start
                while i < n {
                    switch c[i] {
                    case "\"": let e = stringEnd(i); if e < 0 { return start }; i = e
                    case "{", "[": depth += 1
                    case "}", "]": depth -= 1; if depth == 0 { return i + 1 }
                    default: break
                    }
                    i += 1
                }
                return start
            default:
                var i = start
                while i < n, !",}] \n\r\t".unicodeScalars.contains(c[i]) { i += 1 }
                return i
            }
        }
        while i < n {
            if c[i] != "\"" { i += 1; continue }
            let keyEnd = stringEnd(i)
            if keyEnd < 0 { break }
            var j = keyEnd + 1
            while j < n, CharacterSet.whitespacesAndNewlines.contains(c[j]) { j += 1 }
            let key = String(String.UnicodeScalarView(c[(i + 1)..<keyEnd])).replacingOccurrences(of: "\\\"", with: "\"").replacingOccurrences(of: "\\\\", with: "\\")
            if j < n, c[j] == ":", isSensitiveKey(key) {
                var v = j + 1
                while v < n, CharacterSet.whitespacesAndNewlines.contains(c[v]) { v += 1 }
                let vEnd = valueEnd(v)
                if vEnd > v {
                    out.append(contentsOf: c[copied..<v])
                    out.append(contentsOf: "\"\(Self.mark)\"".unicodeScalars)
                    copied = vEnd; i = vEnd; any = true
                    continue
                }
            }
            i = keyEnd + 1
        }
        if !any { return s }
        out.append(contentsOf: c[copied..<n])
        changed = true
        return String(out)
    }
}
#endif
