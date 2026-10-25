import Foundation

/// Finding text inside a displayed body: case-insensitive, on the string as shown.
public enum BodySearch {
    /// Ranges (UTF-16, as NSString uses) of every match, in order. Capped so a
    /// one-letter query over a large body stays cheap.
    public static func ranges(in text: String, query: String, limit: Int = 2000) -> [NSRange] {
        guard !query.isEmpty, !text.isEmpty else { return [] }
        let ns = text as NSString
        var out: [NSRange] = []
        var from = 0
        while from < ns.length, out.count < limit {
            let r = ns.range(of: query, options: [.caseInsensitive, .literal], range: NSRange(location: from, length: ns.length - from))
            if r.location == NSNotFound || r.length == 0 { break }
            out.append(r)
            from = r.location + r.length
        }
        return out
    }

    /// The index after stepping `delta` through `count` matches, wrapping.
    public static func step(_ current: Int, by delta: Int, count: Int) -> Int {
        guard count > 0 else { return 0 }
        return ((current + delta) % count + count) % count
    }

    /// "3 of 12", "No results" or an empty string when there is no query.
    public static func summary(current: Int, count: Int, query: String) -> String {
        if query.isEmpty { return "" }
        return count == 0 ? "No results" : "\(current + 1) of \(count)\(count >= 2000 ? "+" : "")"
    }
}

/// The first lines of a body, bounded in both lines and characters, so a
/// multi-megabyte body never reaches the text system whole.
public enum BodyText {
    public static func lines(_ text: String, maxLines: Int, maxChars: Int = 400_000) -> (lines: [Substring], more: Int) {
        var out: [Substring] = []
        var chars = 0
        var idx = text.startIndex
        while idx < text.endIndex, out.count < maxLines, chars < maxChars {
            let end = text[idx...].firstIndex(of: "\n") ?? text.endIndex
            let line = text[idx..<end]
            out.append(line)
            chars += line.count + 1
            idx = end < text.endIndex ? text.index(after: end) : text.endIndex
        }
        var more = 0
        if idx < text.endIndex {
            more = text.utf8[idx...].reduce(1) { $0 + ($1 == 10 ? 1 : 0) }
        }
        return (out, more)
    }
}

/// What the Tests tab and its badge say.
public struct TestSummary: Equatable {
    public var passed: Int
    public var total: Int
    public init(passed: Int, total: Int) { self.passed = passed; self.total = total }
    public var allPassed: Bool { total > 0 && passed == total }
    public var failed: Int { total - passed }
    /// "2 of 3 passed", "All 3 passed", or empty.
    public var line: String {
        if total == 0 { return "" }
        if passed == total { return total == 1 ? "1 test passed" : "All \(total) passed" }
        return "\(passed) of \(total) passed"
    }
    /// The count on the tab: "2/3".
    public var badge: String { total == 0 ? "" : "\(passed)/\(total)" }
}

/// A short file name for saving a response body.
public func bodyFileName(kind: String, contentType: String) -> String {
    let ct = contentType.lowercased()
    let ext: String
    if kind == "image" {
        ext = ct.contains("png") ? "png" : ct.contains("jpeg") || ct.contains("jpg") ? "jpg" : ct.contains("gif") ? "gif"
            : ct.contains("webp") ? "webp" : ct.contains("svg") ? "svg" : "img"
    } else if kind == "json" { ext = "json" }
    else if kind == "html" { ext = "html" }
    else if ct.contains("xml") { ext = "xml" }
    else if ct.contains("css") { ext = "css" }
    else if ct.contains("javascript") { ext = "js" }
    else { ext = "txt" }
    return "response.\(ext)"
}
