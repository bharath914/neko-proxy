import AppKit
import NekoAPI

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
        // Don't start with the search field focused.
        DispatchQueue.main.async { c.window?.makeFirstResponder(nil) }
        NSApp.activate(ignoringOtherApps: true)
        Task { await c.model.start() }
        if let dir = ProcessInfo.processInfo.environment["NEKO_LIVE_CTL"] { startLiveControl(dir, c) }
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

    /// Debug aid for scripted runs: `NEKO_LIVE_CTL=/dir` makes the app read one command
    /// per line from /dir/cmd (section:rules, select:5, tab:2, new-rule:4, approve:allow, resume, abort, shot:name),
    /// run it, and delete the file. Screenshots land in /dir/<name>.png.
    private func startLiveControl(_ dir: String, _ c: MainWindowController) {
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { _ in
            MainActor.assumeIsolated {
                let file = dir + "/cmd"
                guard let text = try? String(contentsOfFile: file, encoding: .utf8) else { return }
                try? FileManager.default.removeItem(atPath: file)
                for line in text.split(separator: "\n") {
                    let parts = line.split(separator: ":", maxSplits: 1).map(String.init)
                    let arg = parts.count > 1 ? parts[1] : ""
                    switch parts[0] {
                    case "section":
                        let names = ["inspector", "breakpoints", "rules", "devices", "settings"]
                        if let i = names.firstIndex(of: arg), let s = Section(rawValue: i) { c.root.setSection(s) }
                    case "select": if let id = UInt64(arg) { c.root.list.select(id: id) }
                    case "tab": if let i = Int(arg) { c.root.selectTab(i) }
                    case "new-rule": if let i = Int(arg), let k = RuleKind(rawValue: i) { c.root.setSection(.rules); c.root.rules.startNew(k) }
                    case "approve":
                        let m = c.model.admin
                        if let a = m.approvals.first { Task { await m.decide(a.id, allow: arg == "allow") } }
                    case "resume", "abort":
                        let m = c.model.admin
                        if let p = m.paused.first {
                            if parts[0] == "abort" { Task { await m.abort(p.flowID) } }
                            else { Task { await m.resume(BreakpointEdit(p).resume(p)) } }
                        }
                    case "shot":
                        guard let v = c.window?.contentView, let rep = v.bitmapImageRepForCachingDisplay(in: v.bounds) else { break }
                        v.cacheDisplay(in: v.bounds, to: rep)
                        try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: "\(dir)/\(arg).png"))
                    default: break
                    }
                }
            }
        }
    }

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
        add("Neko") { m in
            item(m, "About Neko", #selector(NSApplication.orderFrontStandardAboutPanel(_:)))
            m.addItem(.separator())
            item(m, "Install CA Certificate…", #selector(AppDelegate.menuInstallCA))
            item(m, "Copy Terminal Proxy Settings", #selector(AppDelegate.menuCopyEnv))
            m.addItem(.separator())
            item(m, "Hide Neko", #selector(NSApplication.hide(_:)), "h")
            item(m, "Quit Neko", #selector(NSApplication.terminate(_:)), "q")
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
