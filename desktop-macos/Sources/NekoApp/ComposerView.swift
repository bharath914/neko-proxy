import AppKit
import NekoAPI

/// The Composer screen: a Postman-style workspace. Sidebar of collections on the
/// left; open request tabs on top; the request editor above the response.
/// This view only wires the parts together; each part reads the model.
@MainActor
final class ComposerView: NSView {
    let model: ComposerModel
    var onInspect: ((UInt64) -> Void)?
    private let sidebar: ComposerSidebar
    private let tabBar: RequestTabBar
    private let editor: RequestEditor
    private let response: ResponsePane
    private let sidebarPane = Surface()
    private let main = FlippedView()
    private let split = NSSplitView()
    private let editorHost = FlippedView()
    private let responseHost = FlippedView()
    override var isFlipped: Bool { true }

    init(model: ComposerModel) {
        self.model = model
        sidebar = ComposerSidebar(model: model)
        tabBar = RequestTabBar(model: model)
        editor = RequestEditor(model: model)
        response = ResponsePane(model: model)
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false

        sidebarPane.fill = Theme.grouped
        sidebarPane.translatesAutoresizingMaskIntoConstraints = false
        sidebarPane.addSubview(sidebar)
        sidebar.pin(to: sidebarPane)
        let sidebarWidth = sidebarPane.widthAnchor.constraint(equalToConstant: 280)
        sidebarWidth.priority = .defaultHigh
        sidebarWidth.isActive = true

        main.translatesAutoresizingMaskIntoConstraints = false
        split.isVertical = false
        split.dividerStyle = .thin
        split.translatesAutoresizingMaskIntoConstraints = false
        for (host, v) in [(editorHost, editor as NSView), (responseHost, response as NSView)] {
            host.translatesAutoresizingMaskIntoConstraints = true
            host.addSubview(v)
            v.pin(to: host)
            split.addArrangedSubview(host)
        }
        let line = Divider()
        for v in [tabBar, line, split] as [NSView] { main.addSubview(v) }
        NSLayoutConstraint.activate([
            tabBar.topAnchor.constraint(equalTo: main.topAnchor),
            tabBar.leadingAnchor.constraint(equalTo: main.leadingAnchor),
            tabBar.trailingAnchor.constraint(equalTo: main.trailingAnchor),
            line.topAnchor.constraint(equalTo: tabBar.bottomAnchor),
            line.leadingAnchor.constraint(equalTo: main.leadingAnchor),
            line.trailingAnchor.constraint(equalTo: main.trailingAnchor),
            split.topAnchor.constraint(equalTo: line.bottomAnchor),
            split.leadingAnchor.constraint(equalTo: main.leadingAnchor),
            split.trailingAnchor.constraint(equalTo: main.trailingAnchor),
            split.bottomAnchor.constraint(equalTo: main.bottomAnchor),
        ])

        let divider = Divider(vertical: true)
        for v in [sidebarPane, divider, main] as [NSView] { addSubview(v) }
        NSLayoutConstraint.activate([
            sidebarPane.leadingAnchor.constraint(equalTo: leadingAnchor),
            sidebarPane.topAnchor.constraint(equalTo: topAnchor),
            sidebarPane.bottomAnchor.constraint(equalTo: bottomAnchor),
            divider.leadingAnchor.constraint(equalTo: sidebarPane.trailingAnchor),
            divider.topAnchor.constraint(equalTo: topAnchor),
            divider.bottomAnchor.constraint(equalTo: bottomAnchor),
            main.leadingAnchor.constraint(equalTo: divider.trailingAnchor),
            main.trailingAnchor.constraint(equalTo: trailingAnchor),
            main.topAnchor.constraint(equalTo: topAnchor),
            main.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])

        sidebar.onNewRequest = { [weak self] in self?.newRequest() }
        sidebar.onImportCurl = { [weak self] in self?.importCurl() }
        editor.onSend = { [weak self] in self?.send() }
        response.onInspect = { [weak self] id in self?.onInspect?(id) }
        model.onChange = { [weak self] in self?.chromeChanged() }
        model.onTab = { [weak self] in self?.tabChanged() }
        model.onDisplay = { [weak self] t in if t === self?.model.tab { self?.editor.displayChanged(t) } }
        model.onParams = { [weak self] t in if t === self?.model.tab { self?.editor.paramsChanged(t) } }
        model.onResponse = { [weak self] t in if t === self?.model.tab { self?.response.reload(); self?.editor.reloadChrome() } }
        tabChanged()
    }
    required init?(coder: NSCoder) { fatalError() }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.split.setPosition(self.split.bounds.height * 0.5, ofDividerAt: 0)
        }
    }

    private func chromeChanged() {
        sidebar.reload()
        tabBar.reload()
        editor.reloadChrome()
    }

    private func tabChanged() {
        sidebar.reload()
        tabBar.reload()
        editor.rebuild()
        response.reload()
    }

    // MARK: actions (menu, shortcuts and the command palette call these)

    func newRequest() { model.newRequest(); editor.focusURL() }
    func send() { Task { await model.send() } }
    func save() { editor.save() }
    func copyCurl() { editor.copyCurl() }
    func closeTab() { model.close(model.active) }
    func nextTab(_ delta: Int = 1) { model.nextTab(delta) }
    func focusURL() { editor.focusURL() }
    func editEnvironments() { EnvironmentsSheet.present(model: model, from: window) }
    func showCode() { editor.presentCode() }
    /// 0 Collections, 1 Environments, 2 History.
    func showSidebarTab(_ i: Int) { sidebar.selectedTab = i }

    func importCurl() {
        askText(window, title: "Import cURL", info: "Paste a cURL command to open it as a request.", placeholder: "curl https://…", button: "Import") { [weak self] text in
            Task {
                guard let self else { return }
                if let error = await self.model.importCurl(text) { tell(self.window, "Composer", error) }
            }
        }
    }
}
