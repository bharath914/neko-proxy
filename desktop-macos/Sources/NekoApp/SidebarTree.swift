import Foundation

/// One line of the Collections tree as the sidebar draws it. Pure data, so the
/// tree can be flattened, filtered and compared without touching any view.
struct SidebarRow: Equatable {
    enum Kind: Equatable {
        case folder(path: String, name: String, count: Int, expanded: Bool)
        case request(id: String, name: String, method: String)
    }
    var kind: Kind
    var depth: Int
}

enum SidebarTree {
    /// Does a saved request match the search text (name, URL or method)?
    static func matches(_ r: ComposedRequest, _ query: String) -> Bool {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        if q.isEmpty { return true }
        return r.name.lowercased().contains(q) || r.url.lowercased().contains(q) || r.method.lowercased().contains(q)
    }

    /// Flattens the tree into drawable rows. Collapsed folders hide their content;
    /// a search shows only matching requests with every folder above them open.
    static func rows(tree: [CollectionNode], orphans: [ComposedRequest] = [], collapsed: Set<String>, query: String) -> [SidebarRow] {
        let q = query.trimmingCharacters(in: .whitespaces)
        let searching = !q.isEmpty
        var out: [SidebarRow] = []

        func request(_ r: ComposedRequest, _ depth: Int) {
            out.append(SidebarRow(kind: .request(id: r.id, name: r.name.isEmpty ? (r.url.isEmpty ? "Untitled request" : r.url) : r.name, method: r.method), depth: depth))
        }

        /// Does anything in this node survive the search?
        func survives(_ n: CollectionNode) -> Bool {
            if !searching { return true }
            if n.name.lowercased().contains(q.lowercased()) { return true }
            return n.requests.contains { matches($0, q) } || n.folders.contains(where: survives)
        }

        func walk(_ n: CollectionNode, _ depth: Int, everything: Bool) {
            guard survives(n) else { return }
            let folderHit = searching && n.name.lowercased().contains(q.lowercased())
            let showAll = everything || folderHit
            let expanded = searching || !collapsed.contains(n.path)
            out.append(SidebarRow(kind: .folder(path: n.path, name: n.name, count: n.count, expanded: expanded), depth: depth))
            guard expanded else { return }
            for f in n.folders { walk(f, depth + 1, everything: showAll) }
            for r in n.requests where !searching || showAll || matches(r, q) { request(r, depth + 1) }
        }

        for n in tree { walk(n, 0, everything: false) }
        for r in orphans where !searching || matches(r, q) { request(r, 0) }
        return out
    }

    /// Requests that sit in no known collection; the tree leaves them out.
    static func orphans(collections: [String], requests: [ComposedRequest]) -> [ComposedRequest] {
        let known = Set(collections)
        return requests.filter { !known.contains($0.collection) }
    }

    /// Short method label that fits a narrow column.
    static func methodLabel(_ m: String) -> String {
        switch m.uppercased() {
        case "DELETE": return "DEL"
        case "OPTIONS": return "OPT"
        case "CONNECT": return "CONN"
        case "": return "GET"
        case let s: return s.count > 5 ? String(s.prefix(4)) : s
        }
    }

    /// `Shop/Orders` and `Archive` give `Shop/Orders/Archive`; the parent of
    /// `Shop/Orders` is `Shop`.
    static func child(_ parent: String, _ name: String) -> String { parent.isEmpty ? name : parent + "/" + name }
    static func parent(of path: String) -> String {
        guard let i = path.lastIndex(of: "/") else { return "" }
        return String(path[..<i])
    }
    static func leaf(of path: String) -> String { path.split(separator: "/").last.map(String.init) ?? path }

    /// Every collection and folder path, for "Move to…" and "New folder in…".
    static func destinations(_ collections: [String]) -> [String] { collections.sorted { $0.localizedStandardCompare($1) == .orderedAscending } }
}
