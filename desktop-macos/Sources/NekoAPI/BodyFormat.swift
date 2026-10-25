import Foundation

// Pure helpers behind the Composer's Body and Tests editors: an order-preserving
// JSON formatter, a cheap syntax tokenizer, the body modes and the assertion
// tables. They live here, away from AppKit, so they can be unit tested.

// MARK: - JSON

/// Pretty-prints, minifies and validates JSON text without reordering keys or
/// rewriting numbers and strings (`JSONSerialization` would do both).
public enum JSONFormat {
    public struct Failure: Error, Equatable {
        public let message: String
    }

    /// Two-space indented text, or nil when the text is not valid JSON.
    public static func pretty(_ text: String) -> String? { try? render(text, pretty: true) }

    /// One line, no spaces between tokens, or nil when the text is not valid JSON.
    public static func minified(_ text: String) -> String? { try? render(text, pretty: false) }

    /// Why the text is not valid JSON, or nil when it is.
    public static func error(_ text: String) -> String? {
        do { _ = try render(text, pretty: false); return nil } catch let f as Failure { return f.message } catch { return "Invalid JSON" }
    }

    /// What the editor shows under a JSON body: nil for empty text and for text
    /// with `{{variable}}` placeholders, which only become JSON once filled in.
    public static func validationMessage(_ text: String) -> String? {
        if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || text.contains("{{") { return nil }
        return error(text)
    }

    static func render(_ text: String, pretty: Bool) throws -> String {
        var s = Scanner(text, pretty: pretty)
        try s.value(0)
        s.skipSpace()
        if s.i < s.b.count { throw s.unexpected("end of input") }
        return String(decoding: s.out, as: UTF8.self)
    }

    private struct Scanner {
        let b: [UInt8]
        var i = 0
        var out: [UInt8] = []
        let pretty: Bool

        init(_ text: String, pretty: Bool) {
            b = Array(text.utf8)
            self.pretty = pretty
            out.reserveCapacity(b.count + b.count / 4)
        }

        mutating func skipSpace() {
            while i < b.count, b[i] == 0x20 || b[i] == 0x09 || b[i] == 0x0A || b[i] == 0x0D { i += 1 }
        }

        func unexpected(_ expected: String? = nil) -> Failure {
            guard i < b.count else { return Failure(message: "Unexpected end of input") }
            var line = 1, lineStart = 0
            for k in 0..<i where b[k] == 0x0A { line += 1; lineStart = k + 1 }
            let column = String(decoding: b[lineStart..<i], as: UTF8.self).count + 1
            let ch = String(decoding: b[i..<min(i + 4, b.count)], as: UTF8.self).first.map(String.init) ?? "?"
            let shown = ch == "\n" ? "line break" : "'\(ch)'"
            var m = "Unexpected \(shown) at line \(line), column \(column)"
            if let expected { m += " (expected \(expected))" }
            return Failure(message: m)
        }

        mutating func newline(_ depth: Int) {
            guard pretty else { return }
            out.append(0x0A)
            out.append(contentsOf: repeatElement(0x20, count: depth * 2))
        }

        func isDigit(_ k: Int) -> Bool { k < b.count && b[k] >= 0x30 && b[k] <= 0x39 }

        mutating func value(_ depth: Int) throws {
            skipSpace()
            guard i < b.count else { throw unexpected() }
            if depth > 512 { throw Failure(message: "Too deeply nested") }
            switch b[i] {
            case 0x7B: try object(depth)
            case 0x5B: try array(depth)
            case 0x22: try string()
            case 0x74: try literal("true")
            case 0x66: try literal("false")
            case 0x6E: try literal("null")
            case 0x2D, 0x30...0x39: try number()
            default: throw unexpected()
            }
        }

        mutating func literal(_ word: String) throws {
            let w = Array(word.utf8)
            guard i + w.count <= b.count, Array(b[i..<i + w.count]) == w else { throw unexpected("a value") }
            out.append(contentsOf: w)
            i += w.count
        }

        mutating func number() throws {
            let start = i
            if b[i] == 0x2D { i += 1 }
            guard i < b.count else { throw unexpected("a digit") }
            if b[i] == 0x30 { i += 1 }
            else if isDigit(i) { while isDigit(i) { i += 1 } }
            else { throw unexpected("a digit") }
            if i < b.count, b[i] == 0x2E {
                i += 1
                guard isDigit(i) else { throw unexpected("a digit") }
                while isDigit(i) { i += 1 }
            }
            if i < b.count, b[i] == 0x65 || b[i] == 0x45 {
                i += 1
                if i < b.count, b[i] == 0x2B || b[i] == 0x2D { i += 1 }
                guard isDigit(i) else { throw unexpected("a digit") }
                while isDigit(i) { i += 1 }
            }
            out.append(contentsOf: b[start..<i])
        }

        mutating func string() throws {
            let start = i
            i += 1
            while true {
                guard i < b.count else { throw unexpected() }
                let c = b[i]
                if c == 0x22 { i += 1; break }
                if c < 0x20 { throw unexpected("a closing quote") }
                if c == 0x5C {
                    i += 1
                    guard i < b.count else { throw unexpected() }
                    switch b[i] {
                    case 0x22, 0x5C, 0x2F, 0x62, 0x66, 0x6E, 0x72, 0x74: i += 1
                    case 0x75:
                        i += 1
                        for _ in 0..<4 {
                            guard i < b.count, Self.isHex(b[i]) else { throw unexpected("a hex digit") }
                            i += 1
                        }
                    default: throw unexpected("a valid escape")
                    }
                    continue
                }
                i += 1
            }
            out.append(contentsOf: b[start..<i])
        }

        static func isHex(_ c: UInt8) -> Bool {
            (c >= 0x30 && c <= 0x39) || (c >= 0x41 && c <= 0x46) || (c >= 0x61 && c <= 0x66)
        }

        mutating func object(_ depth: Int) throws {
            out.append(0x7B)
            i += 1
            skipSpace()
            if i < b.count, b[i] == 0x7D { out.append(0x7D); i += 1; return }
            while true {
                newline(depth + 1)
                skipSpace()
                guard i < b.count, b[i] == 0x22 else { throw unexpected("a string key") }
                try string()
                skipSpace()
                guard i < b.count, b[i] == 0x3A else { throw unexpected("':'") }
                i += 1
                out.append(0x3A)
                if pretty { out.append(0x20) }
                try value(depth + 1)
                skipSpace()
                guard i < b.count else { throw unexpected() }
                if b[i] == 0x2C { out.append(0x2C); i += 1; continue }
                if b[i] == 0x7D { break }
                throw unexpected("',' or '}'")
            }
            newline(depth)
            out.append(0x7D)
            i += 1
        }

        mutating func array(_ depth: Int) throws {
            out.append(0x5B)
            i += 1
            skipSpace()
            if i < b.count, b[i] == 0x5D { out.append(0x5D); i += 1; return }
            while true {
                newline(depth + 1)
                try value(depth + 1)
                skipSpace()
                guard i < b.count else { throw unexpected() }
                if b[i] == 0x2C { out.append(0x2C); i += 1; continue }
                if b[i] == 0x5D { break }
                throw unexpected("',' or ']'")
            }
            newline(depth)
            out.append(0x5D)
            i += 1
        }
    }
}

// MARK: - Syntax tinting

/// A small tokenizer for light syntax tinting. It is not a parser: it finds
/// strings, numbers, keywords, tags and comments, and never fails.
public enum BodySyntax {
    public enum Language: Equatable {
        case plain, json, xml, javascript, graphql

        public init(_ kind: Neko_Api_V1_ComposeBody) {
            switch kind {
            case .json: self = .json
            case .xml, .html: self = .xml
            case .javascript: self = .javascript
            case .graphql: self = .graphql
            default: self = .plain
            }
        }
    }

    public enum Kind: Equatable {
        case key, string, number, literal, punct, tag, attr, value, comment, entity, variable
    }

    /// `location` and `length` are UTF-16 offsets, as `NSRange` wants.
    public struct Token: Equatable {
        public var location: Int
        public var length: Int
        public var kind: Kind
    }

    /// Bodies over this size are shown untinted.
    public static let limit = 200_000

    public static func tokens(_ text: String, _ language: Language) -> [Token] {
        let u = Array(text.utf16)
        var out: [Token] = []
        switch language {
        case .json: json(u, &out)
        case .xml: xml(u, &out)
        case .javascript: script(u, &out)
        case .graphql: graphql(u, &out)
        case .plain: break
        }
        variables(u, &out)
        return out
    }

    // MARK: character classes

    private static func c(_ s: Unicode.Scalar) -> UInt16 { UInt16(s.value) }
    private static func isDigit(_ x: UInt16) -> Bool { x >= 0x30 && x <= 0x39 }
    private static func isAlpha(_ x: UInt16) -> Bool { (x >= 0x41 && x <= 0x5A) || (x >= 0x61 && x <= 0x7A) || x == 0x5F || x == 0x24 }
    private static func isSpace(_ x: UInt16) -> Bool { x == 0x20 || x == 0x09 || x == 0x0A || x == 0x0D }
    private static func word(_ u: [UInt16], _ from: Int, _ to: Int) -> String { String(decoding: u[from..<to], as: UTF16.self) }

    /// `{{name}}` placeholders, drawn over whatever else was found.
    private static func variables(_ u: [UInt16], _ out: inout [Token]) {
        var i = 0
        let n = u.count
        while i + 3 < n {
            if u[i] == c("{"), u[i + 1] == c("{") {
                var j = i + 2
                while j + 1 < n, j - i < 80, u[j] != 0x0A, !(u[j] == c("}") && u[j + 1] == c("}")) { j += 1 }
                if j + 1 < n, u[j] == c("}"), u[j + 1] == c("}"), j > i + 2 {
                    out.append(Token(location: i, length: j + 2 - i, kind: .variable))
                    i = j + 2
                    continue
                }
            }
            i += 1
        }
    }

    // MARK: JSON

    private static func json(_ u: [UInt16], _ out: inout [Token]) {
        var i = 0
        let n = u.count
        while i < n {
            let x = u[i]
            if x == c("\"") {
                let start = i
                i += 1
                while i < n, u[i] != c("\""), u[i] != 0x0A {
                    if u[i] == c("\\") { i += 1 }
                    i += 1
                }
                if i < n, u[i] == c("\"") { i += 1 }
                i = min(i, n)
                var j = i
                while j < n, isSpace(u[j]) { j += 1 }
                out.append(Token(location: start, length: i - start, kind: j < n && u[j] == c(":") ? .key : .string))
            } else if isDigit(x) || (x == c("-") && i + 1 < n && isDigit(u[i + 1])) {
                let start = i
                i += 1
                while i < n, isDigit(u[i]) || u[i] == c(".") || u[i] == c("e") || u[i] == c("E") || u[i] == c("+") || u[i] == c("-") { i += 1 }
                out.append(Token(location: start, length: i - start, kind: .number))
            } else if isAlpha(x) {
                let start = i
                while i < n, isAlpha(u[i]) { i += 1 }
                let w = word(u, start, i)
                if w == "true" || w == "false" || w == "null" { out.append(Token(location: start, length: i - start, kind: .literal)) }
            } else {
                if x == c("{") || x == c("}") || x == c("[") || x == c("]") || x == c(",") || x == c(":") {
                    out.append(Token(location: i, length: 1, kind: .punct))
                }
                i += 1
            }
        }
    }

    // MARK: XML and HTML

    private static func hasPrefix(_ u: [UInt16], _ i: Int, _ s: String) -> Bool {
        let p = Array(s.utf16)
        return i + p.count <= u.count && Array(u[i..<i + p.count]) == p
    }

    private static func isNameChar(_ x: UInt16) -> Bool { isAlpha(x) || isDigit(x) || x == c(":") || x == c(".") || x == c("-") }

    private static func xml(_ u: [UInt16], _ out: inout [Token]) {
        var i = 0
        let n = u.count
        while i < n {
            if u[i] == c("<") {
                if hasPrefix(u, i, "<!--") {
                    var j = i + 4
                    while j < n, !hasPrefix(u, j, "-->") { j += 1 }
                    j = min(j + 3, n)
                    out.append(Token(location: i, length: j - i, kind: .comment))
                    i = j
                    continue
                }
                let start = i
                i += 1
                if i < n, u[i] == c("/") || u[i] == c("!") || u[i] == c("?") { i += 1 }
                while i < n, isNameChar(u[i]) { i += 1 }
                out.append(Token(location: start, length: i - start, kind: .tag))
                while i < n, u[i] != c(">") {
                    let x = u[i]
                    if isSpace(x) { i += 1 }
                    else if x == c("\"") || x == c("'") {
                        let s = i
                        i += 1
                        while i < n, u[i] != x { i += 1 }
                        i = min(i + 1, n)
                        out.append(Token(location: s, length: i - s, kind: .value))
                    } else if x == c("=") || x == c("/") || x == c("?") {
                        out.append(Token(location: i, length: 1, kind: .punct)); i += 1
                    } else if isNameChar(x) {
                        let s = i
                        while i < n, isNameChar(u[i]) { i += 1 }
                        out.append(Token(location: s, length: i - s, kind: .attr))
                    } else if x == c("<") {
                        break
                    } else { i += 1 }
                }
                if i < n, u[i] == c(">") { out.append(Token(location: i, length: 1, kind: .tag)); i += 1 }
            } else if u[i] == c("&") {
                var j = i + 1
                while j < n, j - i < 10, isNameChar(u[j]) || u[j] == c("#") { j += 1 }
                if j < n, u[j] == c(";"), j > i + 1 {
                    out.append(Token(location: i, length: j + 1 - i, kind: .entity))
                    i = j + 1
                } else { i += 1 }
            } else { i += 1 }
        }
    }

    // MARK: JavaScript and GraphQL

    private static let jsKeywords: Set<String> = [
        "const", "let", "var", "function", "return", "if", "else", "for", "while", "do", "switch", "case", "break",
        "continue", "new", "class", "extends", "import", "export", "from", "default", "async", "await", "try", "catch",
        "finally", "throw", "typeof", "instanceof", "in", "of", "this", "true", "false", "null", "undefined", "void", "delete",
    ]

    private static let gqlKeywords: Set<String> = [
        "query", "mutation", "subscription", "fragment", "on", "type", "input", "enum", "interface", "union", "scalar",
        "schema", "extend", "directive", "implements", "true", "false", "null",
    ]

    private static func quoted(_ u: [UInt16], _ i: inout Int, multiline: Bool, _ out: inout [Token]) {
        let n = u.count
        let q = u[i], start = i
        i += 1
        while i < n, u[i] != q, multiline || u[i] != 0x0A {
            if u[i] == c("\\") { i += 1 }
            i += 1
        }
        i = min(i, n)
        if i < n, u[i] == q { i += 1 }
        out.append(Token(location: start, length: i - start, kind: .string))
    }

    private static func script(_ u: [UInt16], _ out: inout [Token]) {
        var i = 0
        let n = u.count
        while i < n {
            let x = u[i]
            if x == c("/"), i + 1 < n, u[i + 1] == c("/") {
                let s = i
                while i < n, u[i] != 0x0A { i += 1 }
                out.append(Token(location: s, length: i - s, kind: .comment))
            } else if x == c("/"), i + 1 < n, u[i + 1] == c("*") {
                let s = i
                i += 2
                while i < n, !hasPrefix(u, i, "*/") { i += 1 }
                i = min(i + 2, n)
                out.append(Token(location: s, length: i - s, kind: .comment))
            } else if x == c("\"") || x == c("'") || x == c("`") {
                quoted(u, &i, multiline: x == c("`"), &out)
            } else if isDigit(x) {
                let s = i
                while i < n, isDigit(u[i]) || isAlpha(u[i]) || u[i] == c(".") { i += 1 }
                out.append(Token(location: s, length: i - s, kind: .number))
            } else if isAlpha(x) {
                let s = i
                while i < n, isAlpha(u[i]) || isDigit(u[i]) { i += 1 }
                if jsKeywords.contains(word(u, s, i)) { out.append(Token(location: s, length: i - s, kind: .literal)) }
            } else {
                if "{}[](),;:.=<>+-*/%!&|?".utf16.contains(x) { out.append(Token(location: i, length: 1, kind: .punct)) }
                i += 1
            }
        }
    }

    private static func graphql(_ u: [UInt16], _ out: inout [Token]) {
        var i = 0
        let n = u.count
        while i < n {
            let x = u[i]
            if x == c("#") {
                let s = i
                while i < n, u[i] != 0x0A { i += 1 }
                out.append(Token(location: s, length: i - s, kind: .comment))
            } else if x == c("\"") {
                quoted(u, &i, multiline: false, &out)
            } else if isDigit(x) || (x == c("-") && i + 1 < n && isDigit(u[i + 1])) {
                let s = i
                i += 1
                while i < n, isDigit(u[i]) || u[i] == c(".") || u[i] == c("e") || u[i] == c("E") { i += 1 }
                out.append(Token(location: s, length: i - s, kind: .number))
            } else if x == c("$") || x == c("@") {
                let s = i
                i += 1
                while i < n, isAlpha(u[i]) || isDigit(u[i]) { i += 1 }
                out.append(Token(location: s, length: i - s, kind: .attr))
            } else if isAlpha(x) {
                let s = i
                while i < n, isAlpha(u[i]) || isDigit(u[i]) { i += 1 }
                if gqlKeywords.contains(word(u, s, i)) { out.append(Token(location: s, length: i - s, kind: .literal)) }
            } else {
                if "{}[]():!=|&,".utf16.contains(x) { out.append(Token(location: i, length: 1, kind: .punct)) }
                i += 1
            }
        }
    }
}

// MARK: - Body modes

/// The radio row of the Body section. Several body kinds share one radio: raw
/// covers text, JavaScript, JSON, HTML and XML.
public enum BodyMode: Int, CaseIterable {
    case none, formData, urlEncoded, raw, binary, graphql

    public var title: String {
        switch self {
        case .none: return "none"
        case .formData: return "form-data"
        case .urlEncoded: return "x-www-form-urlencoded"
        case .raw: return "raw"
        case .binary: return "binary"
        case .graphql: return "GraphQL"
        }
    }

    public init(_ kind: Neko_Api_V1_ComposeBody) {
        switch kind {
        case .formData: self = .formData
        case .form: self = .urlEncoded
        case .text, .javascript, .json, .html, .xml: self = .raw
        case .binary: self = .binary
        case .graphql: self = .graphql
        default: self = .none
        }
    }

    /// The body kind for this radio; `raw` uses the given language.
    public func kind(raw: Neko_Api_V1_ComposeBody) -> Neko_Api_V1_ComposeBody {
        switch self {
        case .none: return .none
        case .formData: return .formData
        case .urlEncoded: return .form
        case .raw: return BodyMode.rawKinds.contains(raw) ? raw : .json
        case .binary: return .binary
        case .graphql: return .graphql
        }
    }

    /// The languages of the raw popup, in menu order.
    public static let rawKinds: [Neko_Api_V1_ComposeBody] = [.text, .javascript, .json, .html, .xml]

    public static func rawTitle(_ kind: Neko_Api_V1_ComposeBody) -> String {
        switch kind {
        case .text: return "Text"
        case .javascript: return "JavaScript"
        case .json: return "JSON"
        case .html: return "HTML"
        case .xml: return "XML"
        default: return "Text"
        }
    }
}

// MARK: - Tests (declarative assertions)

/// What each kind of assertion offers in the Tests editor. Menu orders follow
/// the proto enums.
public enum AssertionSchema {
    public typealias Kind = Neko_Api_V1_AssertKind
    public typealias Op = Neko_Api_V1_AssertOp

    public static let kinds: [Kind] = [.status, .header, .body, .json, .time, .size]

    public static func title(_ k: Kind) -> String {
        switch k {
        case .status: return "Status code"
        case .header: return "Header"
        case .body: return "Body"
        case .json: return "JSON value"
        case .time: return "Response time"
        case .size: return "Response size"
        default: return "Status code"
        }
    }

    public static func operators(for k: Kind) -> [Op] {
        switch k {
        case .status: return [.equals, .notEquals, .lessThan, .greaterThan]
        case .header: return [.equals, .notEquals, .contains, .notContains, .exists, .notExists, .matches]
        case .body: return [.equals, .notEquals, .contains, .notContains, .exists, .matches]
        case .json: return [.equals, .notEquals, .contains, .notContains, .exists, .notExists, .lessThan, .greaterThan, .matches]
        case .time, .size: return [.equals, .lessThan, .greaterThan]
        default: return [.equals]
        }
    }

    /// What a new assertion of this kind starts with.
    public static func defaultOp(for k: Kind) -> Op {
        switch k {
        case .header: return .exists
        case .body: return .contains
        case .time, .size: return .lessThan
        default: return .equals
        }
    }

    public static func opTitle(_ o: Op) -> String {
        switch o {
        case .equals: return "equals"
        case .notEquals: return "does not equal"
        case .contains: return "contains"
        case .notContains: return "does not contain"
        case .exists: return "exists"
        case .notExists: return "does not exist"
        case .lessThan: return "is less than"
        case .greaterThan: return "is greater than"
        case .matches: return "matches regex"
        default: return "equals"
        }
    }

    public static func needsTarget(_ k: Kind) -> Bool { k == .header || k == .json }
    public static func needsExpected(_ o: Op) -> Bool { o != .exists && o != .notExists }

    public static func targetHint(_ k: Kind) -> String { k == .header ? "Header name" : "data.items[0].id" }

    public static func expectedHint(_ k: Kind, _ o: Op) -> String {
        if o == .matches { return "Regular expression" }
        switch k {
        case .status: return "200 or 2xx"
        case .time: return "Milliseconds, e.g. 500"
        case .size: return "Bytes, e.g. 1024"
        case .body: return "Text"
        default: return "Value"
        }
    }

    /// The assertion after the person picked another kind: the operator stays if
    /// the new kind offers it, the target goes if the new kind has none.
    public static func changing(_ a: Neko_Api_V1_Assertion, to k: Kind) -> Neko_Api_V1_Assertion {
        var r = a
        r.kind = k
        if !operators(for: k).contains(r.op) { r.op = defaultOp(for: k) }
        if !needsTarget(k) { r.target = "" }
        if !needsExpected(r.op) { r.expected = "" }
        return r
    }

    /// The assertion after the person picked another operator.
    public static func changing(_ a: Neko_Api_V1_Assertion, to o: Op) -> Neko_Api_V1_Assertion {
        var r = a
        r.op = o
        if !needsExpected(o) { r.expected = "" }
        return r
    }

    /// One line, for tooltips: "Status equals 200".
    public static func describe(_ a: Neko_Api_V1_Assertion) -> String {
        var parts: [String]
        switch a.kind {
        case .status: parts = ["Status"]
        case .header: parts = ["Header", a.target]
        case .body: parts = ["Body"]
        case .json: parts = ["JSON", a.target]
        case .time: parts = ["Response time"]
        case .size: parts = ["Response size"]
        default: parts = ["Status"]
        }
        parts.append(opTitle(a.op))
        if needsExpected(a.op) { parts.append(a.expected) }
        if a.kind == .time && needsExpected(a.op) { parts.append("ms") }
        if a.kind == .size && needsExpected(a.op) { parts.append("bytes") }
        return parts.filter { !$0.isEmpty }.joined(separator: " ")
    }

    /// The quick-add chips: title, the assertion, and which field to focus after
    /// adding ("target", "expected" or none).
    public struct Preset {
        public let title: String
        public let assertion: Neko_Api_V1_Assertion
        public let focus: String?
    }

    public static let presets: [Preset] = [
        Preset(title: "Status is 200", assertion: .with { $0.kind = .status; $0.op = .equals; $0.expected = "200" }, focus: nil),
        Preset(title: "Response time < 500 ms", assertion: .with { $0.kind = .time; $0.op = .lessThan; $0.expected = "500" }, focus: nil),
        Preset(title: "Body contains…", assertion: .with { $0.kind = .body; $0.op = .contains }, focus: "expected"),
        Preset(title: "JSON value equals…", assertion: .with { $0.kind = .json; $0.op = .equals }, focus: "target"),
        Preset(title: "Header exists…", assertion: .with { $0.kind = .header; $0.op = .exists }, focus: "target"),
    ]
}
