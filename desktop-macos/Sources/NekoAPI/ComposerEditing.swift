import Foundation

/// Small pure helpers for the composer's request editor: `{{variable}}` and
/// `:path` tokens in the URL bar, which variables resolve, and where inherited
/// auth comes from. Kept here so they can be tested without AppKit.
public enum ComposerEdit {
    public struct Token: Equatable {
        public enum Kind: Equatable { case variable, pathVariable }
        public var kind: Kind
        /// UTF-16 range in the text, including braces or the colon.
        public var range: NSRange
        /// The variable name without braces, trimmed.
        public var name: String
        public init(kind: Kind, range: NSRange, name: String) {
            self.kind = kind; self.range = range; self.name = name
        }
    }

    private static let variable = try! NSRegularExpression(pattern: #"\{\{\s*([^{}]+?)\s*\}\}"#)
    private static let pathVar = try! NSRegularExpression(pattern: #"(?<=/):([A-Za-z_][A-Za-z0-9_]*)"#)

    /// Every `{{name}}` and `/:name` in a URL, in order of appearance.
    public static func tokens(in text: String) -> [Token] {
        let ns = text as NSString
        let all = NSRange(location: 0, length: ns.length)
        var out: [Token] = []
        var braces: [NSRange] = []
        for m in variable.matches(in: text, range: all) {
            braces.append(m.range)
            out.append(Token(kind: .variable, range: m.range, name: ns.substring(with: m.range(at: 1))))
        }
        for m in pathVar.matches(in: text, range: all) {
            // `:name` inside braces is part of a variable, not a path variable.
            if braces.contains(where: { NSIntersectionRange($0, m.range).length > 0 }) { continue }
            out.append(Token(kind: .pathVariable, range: m.range, name: ns.substring(with: m.range(at: 1))))
        }
        return out.sorted { $0.range.location < $1.range.location }
    }

    /// `Shop/Orders/Admin` gives `["Shop/Orders/Admin", "Shop/Orders", "Shop"]`, deepest first.
    public static func chain(_ collection: String) -> [String] {
        let parts = collection.split(separator: "/").map(String.init)
        return parts.indices.reversed().map { parts[0...$0].joined(separator: "/") }
    }

    /// Names that resolve for a request in `collection`: the active environment,
    /// globals and the variables of the collection and its parent folders.
    /// Disabled rows do not count. Dynamic names (`$guid`) always resolve.
    public static func knownVariables(_ state: Neko_Api_V1_ComposerState, collection: String) -> Set<String> {
        var names = Set<String>()
        func add(_ rows: [Neko_Api_V1_KeyValue]) {
            for r in rows where !r.disabled && !r.name.isEmpty { names.insert(r.name) }
        }
        if let env = state.environments.first(where: { $0.name == state.activeEnvironment }) { add(env.vars) }
        add(state.globals)
        for path in chain(collection) {
            if let c = state.configs.first(where: { $0.name == path }) { add(c.vars) }
        }
        return names
    }

    public static func resolves(_ name: String, known: Set<String>) -> Bool {
        name.hasPrefix("$") || known.contains(name)
    }

    /// Where an inheriting request gets its auth: the deepest collection or
    /// folder on its path that sets something other than inherit.
    public static func authSource(collection: String, configs: [Neko_Api_V1_CollectionConfig]) -> (path: String, kind: Neko_Api_V1_ComposeAuth)? {
        for path in chain(collection) {
            if let c = configs.first(where: { $0.name == path }), c.auth.kind != .inherit {
                return (path, c.auth.kind)
            }
        }
        return nil
    }

    public static func authName(_ kind: Neko_Api_V1_ComposeAuth) -> String {
        switch kind {
        case .none: return "No Auth"
        case .bearer: return "Bearer Token"
        case .basic: return "Basic Auth"
        case .inherit: return "Inherit auth from parent"
        case .apiKey: return "API Key"
        case .UNRECOGNIZED: return "Unknown"
        }
    }

    /// What the Inherit card says.
    public static func inheritSummary(collection: String, configs: [Neko_Api_V1_CollectionConfig]) -> String {
        if collection.isEmpty { return "This request is not in a collection, so there is no auth to inherit." }
        guard let s = authSource(collection: collection, configs: configs) else {
            return "No auth is set on the collection."
        }
        let name = s.path.split(separator: "/").last.map(String.init) ?? s.path
        if s.kind == .none { return "Sending without auth, as set on \(name)." }
        return "Using \(authName(s.kind)) from \(name)."
    }

    /// Header names offered while typing in the Headers table.
    public static let commonHeaders = [
        "Accept", "Accept-Encoding", "Accept-Language", "Authorization", "Cache-Control", "Connection",
        "Content-Length", "Content-Type", "Cookie", "DNT", "Host", "If-Modified-Since", "If-None-Match",
        "Origin", "Pragma", "Referer", "User-Agent", "X-Api-Key", "X-Requested-With", "X-Forwarded-For",
    ]

    /// Suggestions for a typed prefix, case-insensitive, keeping their casing.
    public static func headerSuggestions(prefix: String) -> [String] {
        let p = prefix.lowercased()
        guard !p.isEmpty else { return [] }
        return commonHeaders.filter { $0.lowercased().hasPrefix(p) && $0.lowercased() != p }
    }

    /// Enabled rows with a name, which is what Postman counts in a tab.
    public static func enabledCount(_ rows: [Neko_Api_V1_KeyValue]) -> Int {
        rows.filter { !$0.disabled && !$0.name.isEmpty }.count
    }

    /// The file name shown for a file row.
    public static func fileName(_ path: String) -> String {
        path.split(separator: "/").last.map(String.init) ?? path
    }
}
