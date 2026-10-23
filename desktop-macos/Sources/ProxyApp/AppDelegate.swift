import AppKit
import ProxyAPI

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private var controller: MainWindowController?

    func applicationDidFinishLaunching(_ notification: Notification) {
        Theme.registerFonts()
        Appearance.apply()
        NSApp.mainMenu = buildMenu()
        let c = MainWindowController(model: FlowModel())
        controller = c
        c.showWindow(nil)
        c.window?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        Task { await c.model.start() }
        // Debug aid: write a PNG of the live window shortly after launch.
        if let path = ProcessInfo.processInfo.environment["PROXY_LIVE_SHOT"] {
            DispatchQueue.main.asyncAfter(deadline: .now() + 3) {
                guard let v = c.window?.contentView, let rep = v.bitmapImageRepForCachingDisplay(in: v.bounds) else { return }
                v.cacheDisplay(in: v.bounds, to: rep)
                try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path))
            }
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }

    func applicationWillTerminate(_ notification: Notification) {
        controller?.model.stop()
    }

    @objc func menuInstallCA() { controller?.root.setSection(.settings); controller?.root.installCA() }
    @objc func menuCopyEnv() {
        let l = controller?.model.listen ?? ""
        let hp = l.isEmpty ? "127.0.0.1:8899" : l
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString("export HTTP_PROXY=http://\(hp) HTTPS_PROXY=http://\(hp)\nexport SSL_CERT_FILE=\"\(CorePaths.caPath)\" CURL_CA_BUNDLE=\"\(CorePaths.caPath)\" NODE_EXTRA_CA_CERTS=\"\(CorePaths.caPath)\"", forType: .string)
    }
    @objc func menuCopyCurl() { controller?.root.copyCurl() }
    @objc func menuCopyURL() { controller?.root.copyURL() }
    @objc func menuFind() { controller?.root.setSection(.inspector); controller?.root.focusSearch() }
    @objc func menuRecord() { controller?.root.toggleRecording() }
    @objc func menuClear() { controller?.root.confirmClear() }

    private func buildMenu() -> NSMenu {
        let main = NSMenu()
        func add(_ title: String, _ items: (NSMenu) -> Void) {
            let item = NSMenuItem()
            let menu = NSMenu(title: title)
            items(menu)
            item.submenu = menu
            main.addItem(item)
        }
        func item(_ menu: NSMenu, _ title: String, _ action: Selector?, _ key: String = "",
                  _ mods: NSEvent.ModifierFlags = .command, target: AnyObject? = nil) {
            let i = menu.addItem(withTitle: title, action: action, keyEquivalent: key)
            i.keyEquivalentModifierMask = mods
            i.target = target
        }
        add("Proxy") { m in
            item(m, "About Proxy", #selector(NSApplication.orderFrontStandardAboutPanel(_:)))
            m.addItem(.separator())
            item(m, "Install CA Certificate…", #selector(AppDelegate.menuInstallCA))
            item(m, "Copy Terminal Proxy Settings", #selector(AppDelegate.menuCopyEnv))
            m.addItem(.separator())
            item(m, "Hide Proxy", #selector(NSApplication.hide(_:)), "h")
            item(m, "Quit Proxy", #selector(NSApplication.terminate(_:)), "q")
        }
        add("Edit") { m in
            item(m, "Copy", #selector(NSText.copy(_:)), "c")
            item(m, "Select All", #selector(NSText.selectAll(_:)), "a")
            m.addItem(.separator())
            item(m, "Copy as cURL", #selector(AppDelegate.menuCopyCurl), "c", [.command, .shift])
            item(m, "Copy URL", #selector(AppDelegate.menuCopyURL), "c", [.command, .option])
            m.addItem(.separator())
            item(m, "Find", #selector(AppDelegate.menuFind), "f")
        }
        add("Capture") { m in
            item(m, "Pause or Resume", #selector(AppDelegate.menuRecord), "r")
            item(m, "Clear All", #selector(AppDelegate.menuClear), "k")
        }
        add("Window") { m in
            item(m, "Minimize", #selector(NSWindow.performMiniaturize(_:)), "m")
            item(m, "Zoom", #selector(NSWindow.performZoom(_:)))
        }
        NSApp.windowsMenu = main.items.last?.submenu
        return main
    }
}
