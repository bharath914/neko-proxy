import AppKit
import NekoAPI

@MainActor
final class MainWindowController: NSWindowController {
    let root: RootViewController
    var model: FlowModel { root.model }

    init(model: FlowModel) {
        root = RootViewController(model: model)
        let window = NSWindow(contentViewController: root)
        window.styleMask = [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView]
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.title = "Neko Proxy"
        window.isMovableByWindowBackground = false
        window.setContentSize(NSSize(width: 1320, height: 820))
        window.minSize = NSSize(width: 960, height: 560)
        window.setFrameAutosaveName("MainWindow2")
        window.backgroundColor = NSColor(name: nil) { _ in Theme.content }
        super.init(window: window)
        root.onError = { [weak self] m in self?.showError(m) }
    }
    required init?(coder: NSCoder) { fatalError() }

    private var shownError = false
    private func showError(_ message: String) {
        guard !shownError, let w = window else { return }
        shownError = true
        let a = NSAlert()
        a.alertStyle = .warning
        a.messageText = "Can't reach the proxy core"
        a.informativeText = message
        a.beginSheetModal(for: w)
    }
}
