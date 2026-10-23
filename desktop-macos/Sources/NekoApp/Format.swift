import AppKit
import NekoAPI

typealias Flow = Neko_Api_V1_FlowSummary
typealias FlowDetail = Neko_Api_V1_FlowDetail

enum Fmt {
    static func size(_ n: UInt64) -> String { n == 0 ? "–" : bytes(n) }

    /// 196 B, 1.2 KB, 18 KB: one decimal below ten, none above.
    static func bytes(_ n: UInt64) -> String {
        if n < 1024 { return "\(n) B" }
        var v = Double(n) / 1024
        for unit in ["KB", "MB", "GB"] {
            if v < 1024 || unit == "GB" { return v < 10 ? String(format: "%.1f %@", v, unit) : String(format: "%.0f %@", v, unit) }
            v /= 1024
        }
        return "\(n) B"
    }

    static let reasons: [UInt32: String] = [
        200: "OK", 201: "Created", 202: "Accepted", 204: "No Content", 206: "Partial Content", 301: "Moved Permanently",
        302: "Found", 303: "See Other", 304: "Not Modified", 307: "Temporary Redirect", 308: "Permanent Redirect",
        400: "Bad Request", 401: "Unauthorized", 403: "Forbidden", 404: "Not Found", 405: "Method Not Allowed",
        408: "Request Timeout", 409: "Conflict", 410: "Gone", 413: "Payload Too Large", 415: "Unsupported Media Type",
        422: "Unprocessable Entity", 429: "Too Many Requests", 500: "Internal Server Error", 501: "Not Implemented",
        502: "Bad Gateway", 503: "Service Unavailable", 504: "Gateway Timeout",
    ]

    static func statusLine(_ s: UInt32) -> String { reasons[s].map { "\(s) \($0)" } ?? "\(s)" }

    static func duration(_ ms: Double) -> String {
        if ms < 0 { return "–" }
        if ms < 1 { return "<1 ms" }
        if ms < 1000 { return String(format: "%.0f ms", ms) }
        return String(format: "%.2f s", ms / 1000)
    }
}

extension Flow {
    var isTunnel: Bool { flags & 2 != 0 }
    var isPinned: Bool { flags & 1 != 0 }

    var statusText: String {
        if status != 0 { return String(status) }
        if state == .failed || flags & 4 != 0 { return "Error" }
        if isTunnel { return "Tunnel" }
        return "…"
    }

    var statusColor: NSColor {
        switch status {
        case 200..<300: return .systemGreen
        case 300..<400: return .systemOrange
        case 400...: return .systemRed
        default: return state == .failed || flags & 4 != 0 ? .systemRed : .secondaryLabelColor
        }
    }
}
