import Foundation
import NekoAPI

typealias ComposedRequest = Neko_Api_V1_ComposedRequest
typealias ComposerState = Neko_Api_V1_ComposerState
typealias KeyValue = Neko_Api_V1_KeyValue
typealias Environment = Neko_Api_V1_Environment
typealias CollectionConfig = Neko_Api_V1_CollectionConfig
typealias ComposeAuthConfig = Neko_Api_V1_ComposeAuthConfig
typealias Assertion = Neko_Api_V1_Assertion
typealias TestResult = Neko_Api_V1_TestResult
typealias ResponseCookie = Neko_Api_V1_Cookie
typealias CodeLanguage = Neko_Api_V1_CodeLanguage

/// A result with the core's message as the error.
enum Outcome<T> {
    case ok(T)
    case failed(String)
}

/// One open request in the composer: what is being edited and its last response.
/// The model owns these; views read them and ask the model to change them.
@MainActor
final class RequestTab {
    let uid = UUID()
    var draft: ComposedRequest
    /// The saved copy the draft started from, to tell whether it changed.
    var original: ComposedRequest?
    var sending = false
    var sent: Neko_Api_V1_ComposerSent?
    var response: FlowDetail?
    var sendError: String?
    /// The URL as the bar shows it: the address plus the enabled parameters.
    var display = ""
    /// Bumped whenever the draft is replaced wholesale, so the editor rebuilds.
    var generation = 0
    /// Bumped when the core rewrote the params or path variables from the URL text.
    var paramsGeneration = 0
    var urlSeq = 0
    /// URL and params syncs sent to the core that have not answered yet.
    var syncsInFlight = 0
    /// Remembered per tab so switching back lands where the person was.
    var section = 0
    var responseSection = 0

    init(draft: ComposedRequest, original: ComposedRequest?) {
        self.draft = draft
        self.original = original
    }

    var dirty: Bool {
        guard let o = original else { return draft != ComposerModel.blank() }
        var a = draft, b = o
        a.updatedMs = 0; b.updatedMs = 0
        return a != b
    }

    var title: String {
        if !draft.name.isEmpty { return draft.name }
        return draft.url.isEmpty ? "Untitled request" : draft.url
    }
}

/// A folder or collection in the sidebar tree.
struct CollectionNode {
    /// Full path, `Shop/Orders`.
    var path: String
    /// Last path component.
    var name: String
    var folders: [CollectionNode]
    var requests: [ComposedRequest]
    var count: Int { requests.count + folders.reduce(0) { $0 + $1.count } }
}

/// The composer's state: the saved collections, environments and history as the
/// core reports them, the open request tabs, and calls to the core. Building,
/// variables, URL syncing, snippets and Postman files all happen in the core.
@MainActor
final class ComposerModel {
    private(set) var state = ComposerState()
    private(set) var tabs: [RequestTab] = []
    private(set) var active = 0
    private var client: APIClient?

    /// Lists, tab titles, environment or anything not tied to one editor.
    var onChange: (() -> Void)?
    /// The active tab changed or a tab's draft was replaced: rebuild the editor.
    var onTab: (() -> Void)?
    /// The URL text of a tab changed because its params changed.
    var onDisplay: ((RequestTab) -> Void)?
    /// The core rewrote a tab's params or path variables from the URL text.
    var onParams: ((RequestTab) -> Void)?
    /// A tab's sending flag or response changed.
    var onResponse: ((RequestTab) -> Void)?
    var onError: ((String) -> Void)?

    static let methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"]

    static func blank() -> ComposedRequest {
        .with { $0.method = "GET"; $0.auth = .inherit }
    }

    init() {
        tabs = [RequestTab(draft: Self.blank(), original: nil)]
    }

    var tab: RequestTab { tabs[active] }

    /// The request in the active editor.
    var draft: ComposedRequest {
        get { tab.draft }
        set { tab.draft = newValue }
    }

    var sending: Bool { tab.sending }
    var response: FlowDetail? { tab.response }
    var sendError: String? { tab.sendError }
    var sent: Neko_Api_V1_ComposerSent? { tab.sent }
    var dirty: Bool { tab.dirty }
    var draftGeneration: Int { tab.generation }

    func attach(_ c: APIClient) async {
        client = c
        await refresh()
        for t in tabs { await normalize(t) }
    }

    /// Nil error on success, otherwise the reason the core gave.
    @discardableResult
    private func call(_ build: (inout ClientMessage) -> Void) async -> (ServerMessage?, String?) {
        guard let c = client else { return (nil, "Not connected to the core.") }
        do {
            let r = try await c.request(build)
            if case .composer(let s)? = r.body { state = s; onChange?() }
            return (r, nil)
        } catch {
            return (nil, error.localizedDescription)
        }
    }

    func refresh() async { await call { $0.composerGet = .init() } }

    // MARK: tabs

    func select(_ i: Int) {
        guard tabs.indices.contains(i), i != active else { return }
        active = i
        onChange?(); onTab?()
    }

    @discardableResult
    func newTab(_ r: ComposedRequest? = nil, saved: Bool = false) -> RequestTab {
        let t = RequestTab(draft: r ?? Self.blank(), original: saved ? r : nil)
        // Reuse an empty untouched tab instead of piling them up.
        if r == nil, tabs.count == 1, !tabs[0].dirty, tabs[0].original == nil, tabs[0].draft.url.isEmpty {
            active = 0
            onChange?(); onTab?()
            return tabs[0]
        }
        tabs.insert(t, at: min(active + 1, tabs.count))
        active = tabs.firstIndex { $0 === t } ?? tabs.count - 1
        onChange?(); onTab?()
        Task { await normalize(t) }
        return t
    }

    func newRequest() { newTab() }

    /// Closes a tab; the last one is replaced by a blank request.
    func close(_ i: Int) {
        guard tabs.indices.contains(i) else { return }
        tabs.remove(at: i)
        if tabs.isEmpty { tabs = [RequestTab(draft: Self.blank(), original: nil)]; active = 0 }
        else if active > i || active >= tabs.count { active = max(0, min(active - (active > i ? 1 : 0), tabs.count - 1)) }
        onChange?(); onTab?()
    }

    func closeOthers(keeping i: Int) {
        guard tabs.indices.contains(i) else { return }
        tabs = [tabs[i]]; active = 0
        onChange?(); onTab?()
    }

    func move(_ from: Int, to: Int) {
        guard tabs.indices.contains(from), tabs.indices.contains(to), from != to else { return }
        let current = tab
        let t = tabs.remove(at: from)
        tabs.insert(t, at: to)
        active = tabs.firstIndex { $0 === current } ?? 0
        onChange?()
    }

    func nextTab(_ delta: Int) {
        guard tabs.count > 1 else { return }
        select((active + delta + tabs.count) % tabs.count)
    }

    /// Opens a saved request, focusing its tab if it is already open.
    func open(_ id: String) {
        guard let r = state.requests.first(where: { $0.id == id }) else { return }
        if let i = tabs.firstIndex(where: { $0.draft.id == id }) { select(i); return }
        newTab(r, saved: true)
    }

    func openHistory(_ h: Neko_Api_V1_ComposerHistoryEntry) {
        var r = h.request
        // A history entry is a copy; editing it should not overwrite a saved request by accident.
        let stillSaved = state.requests.contains { $0.id == r.id }
        if !stillSaved { r.id = "" }
        let t = newTab(r, saved: false)
        t.original = stillSaved ? state.requests.first { $0.id == r.id } : r
        if h.flowID != 0 || !h.error.isEmpty {
            t.sent = .with {
                $0.flowID = h.flowID; $0.status = h.status; $0.method = h.method; $0.url = h.url
                $0.error = h.error; $0.durationMs = h.durationMs
            }
            t.sendError = h.error.isEmpty ? nil : h.error
        }
        onResponse?(t)
        if h.flowID != 0 { Task { await loadResponse(h.flowID, into: t) } }
    }

    /// Replaces the active tab's request wholesale (an import).
    func load(_ r: ComposedRequest, saved: Bool) {
        let t = tab
        t.draft = r
        t.original = saved ? r : nil
        t.sent = nil; t.response = nil; t.sendError = nil
        t.generation += 1
        onChange?(); onTab?(); onResponse?(t)
        Task { await normalize(t) }
    }

    /// Changes the active tab's request and tells the lists and tab titles.
    func edit(_ change: (inout ComposedRequest) -> Void) {
        change(&tab.draft)
        onChange?()
    }

    // MARK: URL and params

    /// Asks the core to fill path variables and compute the URL text.
    func normalize(_ t: RequestTab) async {
        let r = t.draft
        guard case .composerURL(let u)? = await call({ $0.composerURL = .with { $0.request = r } }).0?.body else { return }
        t.draft.pathVars = u.request.pathVars
        t.display = u.display
        t.paramsGeneration += 1
        onParams?(t); onDisplay?(t)
    }

    /// The person typed in the URL bar.
    func urlEdited(_ text: String) {
        let t = tab
        t.display = text
        t.urlSeq += 1
        let seq = t.urlSeq
        let r = t.draft
        t.syncsInFlight += 1
        Task {
            defer { t.syncsInFlight -= 1 }
            guard case .composerURL(let u)? = await call({ $0.composerURL = .with { $0.request = r; $0.edit = true; $0.text = text } }).0?.body,
                  seq == t.urlSeq else { return }
            let paramsChanged = t.draft.params != u.request.params || t.draft.pathVars != u.request.pathVars
            t.draft.url = u.request.url
            t.draft.params = u.request.params
            t.draft.pathVars = u.request.pathVars
            if paramsChanged { t.paramsGeneration += 1; onParams?(t) }
            onChange?()
        }
    }

    /// The Params table or path variables changed; the URL text follows.
    func paramsEdited(_ rows: [KeyValue]? = nil, pathVars: [KeyValue]? = nil) {
        let t = tab
        if let rows { t.draft.params = rows }
        if let pathVars { t.draft.pathVars = pathVars }
        t.urlSeq += 1
        let seq = t.urlSeq
        let r = t.draft
        t.syncsInFlight += 1
        Task {
            defer { t.syncsInFlight -= 1 }
            guard case .composerURL(let u)? = await call({ $0.composerURL = .with { $0.request = r } }).0?.body,
                  seq == t.urlSeq else { return }
            // Typing a `:name` into the URL is handled by urlEdited; here only the text moves.
            t.display = u.display
            onDisplay?(t)
            onChange?()
        }
    }

    // MARK: saving

    func save(name: String? = nil, collection: String? = nil) async -> String? {
        let t = tab
        var r = t.draft
        if let name { r.name = name }
        if let collection { r.collection = collection }
        let (res, err) = await call { $0.composerSave = .with { $0.request = r } }
        if let err { return err }
        if case .composer(let s)? = res?.body, let saved = s.requests.first(where: { $0.id == s.savedID }) {
            // Only rebuild the editor when the core changed what is in it, so saving keeps focus.
            var a = saved, b = r
            for x in [\ComposedRequest.id, \.name, \.collection] { a[keyPath: x] = ""; b[keyPath: x] = "" }
            a.updatedMs = 0; b.updatedMs = 0
            t.draft = saved
            t.original = saved
            if a != b { t.generation += 1; onTab?() }
            onChange?()
        }
        return nil
    }

    func renameRequest(_ request: ComposedRequest) async -> String? {
        let error = await call { $0.composerSave = .with { $0.request = request } }.1
        if error == nil, let t = tabs.first(where: { $0.draft.id == request.id }) {
            t.draft.name = request.name; t.original?.name = request.name
            onChange?()
        }
        return error
    }

    /// Moves a saved request to another collection or folder.
    func move(request id: String, toCollection path: String) async -> String? {
        guard var r = state.requests.first(where: { $0.id == id }) else { return nil }
        r.collection = path
        let error = await call { $0.composerSave = .with { $0.request = r } }.1
        if error == nil, let t = tabs.first(where: { $0.draft.id == id }) {
            t.draft.collection = path; t.original?.collection = path
            onChange?()
        }
        return error
    }

    func duplicate(_ id: String) async {
        guard var r = state.requests.first(where: { $0.id == id }) else { return }
        r.id = ""
        r.name += " copy"
        _ = await call { $0.composerSave = .with { $0.request = r } }
    }

    func remove(_ id: String) async {
        _ = await call { $0.composerRemove = .with { $0.id = id } }
        for t in tabs where t.draft.id == id { t.draft.id = ""; t.original = nil }
        onChange?()
    }

    func addCollection(_ name: String) async -> String? {
        await call { $0.composerCollection = .with { $0.name = name } }.1
    }

    func renameCollection(_ from: String, to: String) async -> String? {
        let err = await call { $0.composerCollection = .with { $0.name = from; $0.renameTo = to } }.1
        if err == nil {
            for t in tabs {
                if t.draft.collection == from { t.draft.collection = to; t.original?.collection = to }
                else if t.draft.collection.hasPrefix(from + "/") {
                    let rest = String(t.draft.collection.dropFirst(from.count))
                    t.draft.collection = to + rest; t.original?.collection = to + rest
                }
            }
            onChange?()
        }
        return err
    }

    func removeCollection(_ name: String) async {
        _ = await call { $0.composerCollection = .with { $0.name = name; $0.remove = true } }
        for t in tabs where t.draft.collection == name || t.draft.collection.hasPrefix(name + "/") {
            t.draft.id = ""; t.original = nil
        }
        onChange?()
    }

    // MARK: environments, globals, collection settings

    func setEnvironments(_ envs: [Environment], active: String) async -> String? {
        await call { $0.composerEnvironments = .with { $0.environments = envs; $0.active = active } }.1
    }

    func setActive(_ name: String) async { _ = await setEnvironments(state.environments, active: name) }

    func setGlobals(_ vars: [KeyValue]) async -> String? {
        await call { $0.composerGlobals = .with { $0.vars = vars } }.1
    }

    func config(for path: String) -> CollectionConfig {
        state.configs.first { $0.name == path } ?? .with { $0.name = path; $0.auth = .with { $0.kind = .inherit } }
    }

    func setConfig(_ c: CollectionConfig) async -> String? {
        await call { $0.composerConfig = .with { $0.config = c } }.1
    }

    func clearHistory() async { _ = await call { $0.composerClearHistory = .init() } }

    // MARK: sending

    func send() async { await send(tab) }

    func send(_ t: RequestTab) async {
        guard !t.sending else { return }
        t.sending = true
        t.sendError = nil
        onResponse?(t)
        // What was just typed is still on its way to the core and back.
        for _ in 0..<100 where t.syncsInFlight > 0 { try? await Task.sleep(nanoseconds: 10_000_000) }
        let r = t.draft
        let generation = t.generation
        let (res, err) = await call { $0.composerSend = .with { $0.request = r } }
        t.sending = false
        if generation != t.generation { await refresh(); onResponse?(t); return }
        if let err {
            t.sent = nil; t.response = nil; t.sendError = err
        } else if case .composerSent(let s)? = res?.body {
            t.sent = s
            t.sendError = s.error.isEmpty ? nil : s.error
            t.response = nil
            if s.flowID != 0 { await loadResponse(s.flowID, into: t) }
        }
        await refresh()
        onResponse?(t)
    }

    private func loadResponse(_ id: UInt64, into t: RequestTab) async {
        guard let c = client else { return }
        if let r = try? await c.request({ $0.getFlow = .with { $0.id = id; $0.parts = 7 } }), case .flow(let d)? = r.body {
            guard t.sent?.flowID == id else { return }
            t.response = d
            onResponse?(t)
        }
    }

    /// What `resolve` answers without a core, for screenshots.
    var fixtureResolved: Neko_Api_V1_ComposerResolved?

    /// What the draft would send, with variables filled in.
    func resolve() async -> Neko_Api_V1_ComposerResolved? {
        if client == nil { return fixtureResolved }
        let r = draft
        if case .composerResolved(let v)? = await call({ $0.composerResolve = .with { $0.request = r } }).0?.body { return v }
        return nil
    }

    // MARK: editing helpers (all logic is in the core)

    /// Rows from "name: value" lines; a leading `//` disables the row.
    func bulkParse(_ text: String) async -> [KeyValue] {
        if case .composerBulk(let b)? = await call({ $0.composerBulk = .with { $0.text = text } }).0?.body { return b.rows }
        return []
    }

    func bulkFormat(_ rows: [KeyValue]) async -> String {
        if case .composerBulk(let b)? = await call({ $0.composerBulk = .with { $0.rows = .with { $0.rows = rows } } }).0?.body { return b.text }
        return ""
    }

    /// The languages snippets can be written in.
    func codeLanguages() async -> [CodeLanguage] {
        if case .composerCode(let c)? = await call({ $0.composerCode = .with { $0.request = draft } }).0?.body { return c.languages }
        return []
    }

    /// A snippet for the active request; the string is the error when it fails.
    func code(_ lang: String) async -> (String, String?) {
        let r = draft
        if case .composerCode(let c)? = await call({ $0.composerCode = .with { $0.request = r; $0.lang = lang } }).0?.body {
            return c.error.isEmpty ? (c.code, nil) : ("", c.error)
        }
        return ("", "Not connected to the core.")
    }

    // MARK: import and export

    /// Nil on success.
    func importCurl(_ text: String) async -> String? {
        let (res, err) = await call { $0.composerImport = .with { $0.curl = text } }
        if let err { return err }
        if case .composed(let r)? = res?.body { newTab(r, saved: false) }
        return nil
    }

    func importFlow(_ id: UInt64) async -> String? {
        let (res, err) = await call { $0.composerImport = .with { $0.flowID = id } }
        if let err { return err }
        if case .composed(let r)? = res?.body { newTab(r, saved: false) }
        return nil
    }

    /// Adds the collections and environments in a Postman file. Returns what was
    /// added, or the error.
    func importPostman(_ json: String) async -> Outcome<String> {
        let (res, err) = await call { $0.composerTransfer = .with { $0.importJson = json } }
        if let err { return .failed(err) }
        if case .composer(let s)? = res?.body { return .ok(s.notice) }
        return .failed("Unexpected reply from the core.")
    }

    /// The JSON and a file name for a collection or environment.
    func exportPostman(environment: Bool, name: String) async -> Outcome<(String, String)> {
        let (res, err) = await call { $0.composerTransfer = .with { $0.export = .with { $0.environment = environment; $0.name = name } } }
        if let err { return .failed(err) }
        if case .composerExport(let e)? = res?.body { return .ok((e.json, e.filename)) }
        return .failed("Unexpected reply from the core.")
    }

    // MARK: sidebar tree

    /// Collections as a tree, folders before requests.
    func tree() -> [CollectionNode] { Self.tree(collections: state.collections, requests: state.requests) }

    static func tree(collections: [String], requests: [ComposedRequest]) -> [CollectionNode] {
        func node(_ path: String) -> CollectionNode {
            let prefix = path + "/"
            let kids = collections.filter { $0.hasPrefix(prefix) && !$0.dropFirst(prefix.count).contains("/") }
            return CollectionNode(
                path: path,
                name: path.split(separator: "/").last.map(String.init) ?? path,
                folders: kids.map(node),
                requests: requests.filter { $0.collection == path }
            )
        }
        return collections.filter { !$0.contains("/") }.map(node)
    }

    // MARK: fixtures

    /// Loads a fixed state for screenshots.
    func loadFixture(_ s: ComposerState, draft d: ComposedRequest, response r: FlowDetail?, sent st: Neko_Api_V1_ComposerSent?, display: String? = nil) {
        state = s
        let t = RequestTab(draft: d, original: s.requests.first { $0.id == d.id })
        t.response = r
        t.sent = st
        t.display = display ?? d.url
        tabs = [t]
        active = 0
        onChange?(); onTab?(); onResponse?(t)
    }

    /// Adds a tab for screenshots.
    func addFixtureTab(_ d: ComposedRequest) {
        let t = RequestTab(draft: d, original: state.requests.first { $0.id == d.id })
        t.display = d.url
        tabs.append(t)
        onChange?()
    }
}
