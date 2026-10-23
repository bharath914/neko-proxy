import AppKit

MainActor.assumeIsolated {
    let app = NSApplication.shared
    if let dir = ProcessInfo.processInfo.environment["PROXY_SHOT_DIR"] {
        app.setActivationPolicy(.accessory)
        Fixtures.run(into: dir)
        exit(0)
    }
    let delegate = AppDelegate()
    app.delegate = delegate
    app.setActivationPolicy(.regular)
    app.run()
}
