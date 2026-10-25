import AppKit
import Foundation
import Testing
@testable import NekoAPI
@testable import NekoApp

@MainActor
@Suite struct ComposerSidebarTests {
    private func req(_ id: String, _ collection: String, name: String = "", method: String = "GET", url: String = "") -> ComposedRequest {
        .with { $0.id = id; $0.collection = collection; $0.name = name.isEmpty ? id : name; $0.method = method; $0.url = url }
    }

    private var cal: Calendar {
        var c = Calendar(identifier: .gregorian)
        c.timeZone = TimeZone(identifier: "UTC")!
        return c
    }

    @Test func historyGroupsByDay() {
        let now = Date(timeIntervalSince1970: 1_800_000_000)  // mid-day UTC
        let ms = { (d: TimeInterval) in Int64((now.timeIntervalSince1970 - d) * 1000) }
        let day: TimeInterval = 86_400
        let groups = HistoryGrouping.groups(sentMs: [ms(60), ms(3600), ms(day), ms(day + 5), ms(5 * day), 0],
                                            now: now, calendar: cal, locale: Locale(identifier: "en_US"))
        #expect(groups.map(\.title).prefix(2) == ["Today", "Yesterday"])
        #expect(groups[0].indices == [0, 1])
        #expect(groups[1].indices == [2, 3])
        #expect(groups[2].indices == [4] && groups[2].title.contains("2027"))
        #expect(groups[3] == HistoryGroup(title: "Earlier", indices: [5]))
    }

    @Test func splitsHostAndPath() {
        let (h, p) = HistoryGrouping.pathAndHost("https://api.example.com:8443/v1/me?x=1")
        #expect(h == "api.example.com:8443" && p == "/v1/me?x=1")
        #expect(HistoryGrouping.pathAndHost("{{base}}/x").path == "{{base}}/x")
    }

    @Test func rowsFollowExpansionAndSearch() {
        let tree = ComposerModel.tree(
            collections: ["Shop", "Shop/Orders", "Auth"],
            requests: [req("a", "Shop", name: "Me", url: "/me"), req("b", "Shop/Orders", name: "List", url: "/orders"), req("c", "Auth", name: "Login", method: "POST", url: "/login")])
        let all = SidebarTree.rows(tree: tree, collapsed: [], query: "")
        #expect(all.count == 6)
        #expect(all[0].kind == .folder(path: "Shop", name: "Shop", count: 2, expanded: true))
        #expect(all[1].depth == 1)
        let closed = SidebarTree.rows(tree: tree, collapsed: ["Shop"], query: "")
        #expect(closed.count == 3)
        // A search opens every folder above a hit and hides the rest.
        let hit = SidebarTree.rows(tree: tree, collapsed: ["Shop"], query: "orders")
        #expect(hit.map(\.depth) == [0, 1, 2])
        #expect(SidebarTree.rows(tree: tree, collapsed: [], query: "post").count == 2)
        #expect(SidebarTree.rows(tree: tree, collapsed: [], query: "nothing").isEmpty)
        // A matching folder name keeps its whole content.
        #expect(SidebarTree.rows(tree: tree, collapsed: [], query: "auth").count == 2)
    }

    @Test func pathHelpers() {
        #expect(SidebarTree.child("", "A") == "A" && SidebarTree.child("A", "B") == "A/B")
        #expect(SidebarTree.parent(of: "A/B/C") == "A/B" && SidebarTree.parent(of: "A") == "")
        #expect(SidebarTree.leaf(of: "A/B") == "B")
        #expect(SidebarTree.methodLabel("DELETE") == "DEL" && SidebarTree.methodLabel("PATCH") == "PATCH")
    }

    /// Writes the sheets and the popover to PNGs when `NEKO_SHOT_DIR` is set.
    @Test func snapshots() throws {
        guard let dir = ProcessInfo.processInfo.environment["NEKO_SHEET_DIR"] else { return }
        try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        Theme.registerFonts()
        let model = ComposerModel()
        Fixtures.composerFixture(model)
        func snap(_ view: NSView, size: NSSize, _ name: String, dark: Bool) {
            let w = NSWindow(contentRect: NSRect(origin: .zero, size: size), styleMask: [.titled], backing: .buffered, defer: false)
            w.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
            w.contentView = view
            w.setContentSize(size)
            view.layoutSubtreeIfNeeded()
            RunLoop.main.run(until: Date().addingTimeInterval(0.2))
            view.layoutSubtreeIfNeeded()
            guard let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return }
            view.cacheDisplay(in: view.bounds, to: rep)
            try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: "\(dir)/\(name)-\(dark ? "dark" : "light").png"))
        }
        for dark in [false, true] {
            let envs = EnvironmentsEditor(model: model, selecting: "Staging")
            let root = envs.sheetWindow.contentView!
            envs.sheetWindow.contentView = nil
            snap(root, size: NSSize(width: 800, height: 560), "sheet-envs", dark: dark)
            let g = EnvironmentsEditor(model: model, selecting: nil)
            let groot = g.sheetWindow.contentView!
            g.sheetWindow.contentView = nil
            snap(groot, size: NSSize(width: 800, height: 560), "sheet-globals", dark: dark)
            for (i, (path, tab)) in [("Shop API", 0), ("Shop API/Orders", 1), ("Shop API", 1), ("Shop API", 2), ("Shop API", 3)].enumerated() {
                let c = CollectionSettingsEditor(model: model, path: path)
                c.select(tab: tab)
                let r = c.sheetWindow.contentView!
                c.sheetWindow.contentView = nil
                snap(r, size: NSSize(width: 660, height: 560), "sheet-collection\(i)", dark: dark)
            }
            let q = VariablesQuickLook(model: model)
            snap(q, size: q.size, "popover", dark: dark)
        }
    }
}
