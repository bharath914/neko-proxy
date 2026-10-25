import Foundation
import Testing
@testable import NekoAPI
@testable import NekoApp

@MainActor
@Suite struct ComposerModelTests {
    private func req(_ id: String, _ collection: String) -> ComposedRequest {
        .with { $0.id = id; $0.collection = collection; $0.method = "GET"; $0.url = "http://x/\(id)" }
    }

    @Test func treeNestsFoldersAndCountsRequests() {
        let tree = ComposerModel.tree(
            collections: ["Shop", "Shop/Orders", "Shop/Orders/Admin", "Auth"],
            requests: [req("a", "Shop"), req("b", "Shop/Orders"), req("c", "Shop/Orders/Admin"), req("d", "Auth")]
        )
        #expect(tree.map(\.name) == ["Shop", "Auth"])
        let shop = tree[0]
        #expect(shop.folders.map(\.path) == ["Shop/Orders"])
        #expect(shop.folders[0].folders[0].name == "Admin")
        #expect(shop.requests.map(\.id) == ["a"])
        #expect(shop.count == 3)
    }

    @Test func tabsOpenFocusAndClose() {
        let m = ComposerModel()
        #expect(m.tabs.count == 1)
        let first = m.tab
        // An untouched blank tab is reused.
        m.newRequest()
        #expect(m.tabs.count == 1 && m.tab === first)
        m.edit { $0.url = "http://x/1" }
        m.newRequest()
        #expect(m.tabs.count == 2 && m.active == 1)
        m.select(0)
        #expect(m.tab === first)
        m.nextTab(1)
        #expect(m.active == 1)
        m.close(1)
        #expect(m.tabs.count == 1 && m.tab === first)
        m.close(0)
        #expect(m.tabs.count == 1 && m.tab.draft.url.isEmpty)
    }

    @Test func dirtyFollowsEdits() {
        let m = ComposerModel()
        #expect(!m.dirty)
        m.edit { $0.url = "http://x" }
        #expect(m.dirty)
    }
}
