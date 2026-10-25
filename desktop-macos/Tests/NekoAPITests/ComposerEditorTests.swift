import Foundation
import Testing
@testable import NekoAPI

@Suite struct ComposerEditorTests {
    private func kv(_ n: String, _ v: String = "", off: Bool = false) -> Neko_Api_V1_KeyValue {
        .with { $0.name = n; $0.value = v; $0.disabled = off }
    }

    @Test func tokensFindVariablesAndPathVars() {
        let t = ComposerEdit.tokens(in: "{{base_url}}/v1/:id/items/{{ page }}?x=:no")
        #expect(t.map(\.name) == ["base_url", "id", "page"])
        #expect(t.map(\.kind) == [.variable, .pathVariable, .variable])
        #expect(t[0].range == NSRange(location: 0, length: 12))
        #expect(t[1].range.length == 3)
    }

    @Test func schemeAndPortAreNotPathVars() {
        #expect(ComposerEdit.tokens(in: "http://localhost:8080/a").isEmpty)
        #expect(ComposerEdit.tokens(in: "{{a:b}}").map(\.kind) == [.variable])
    }

    @Test func knownVariablesFollowEnvironmentGlobalsAndCollectionChain() {
        let state = Neko_Api_V1_ComposerState.with {
            $0.activeEnvironment = "Staging"
            $0.environments = [
                .with { $0.name = "Staging"; $0.vars = [kv("base_url"), kv("off", off: true)] },
                .with { $0.name = "Prod"; $0.vars = [kv("prod_only")] },
            ]
            $0.globals = [kv("locale")]
            $0.configs = [
                .with { $0.name = "Shop"; $0.vars = [kv("shop_var")] },
                .with { $0.name = "Shop/Orders"; $0.vars = [kv("order_var")] },
                .with { $0.name = "Other"; $0.vars = [kv("other_var")] },
            ]
        }
        let k = ComposerEdit.knownVariables(state, collection: "Shop/Orders")
        #expect(k == ["base_url", "locale", "shop_var", "order_var"])
        #expect(ComposerEdit.resolves("$guid", known: k))
        #expect(!ComposerEdit.resolves("prod_only", known: k))
    }

    @Test func authComesFromTheNearestConfigThatSetsOne() {
        let configs: [Neko_Api_V1_CollectionConfig] = [
            .with { $0.name = "Shop"; $0.auth = .with { $0.kind = .bearer } },
            .with { $0.name = "Shop/Orders"; $0.auth = .with { $0.kind = .inherit } },
            .with { $0.name = "Shop/Orders/Admin"; $0.auth = .with { $0.kind = .basic } },
        ]
        #expect(ComposerEdit.authSource(collection: "Shop/Orders", configs: configs)?.path == "Shop")
        #expect(ComposerEdit.authSource(collection: "Shop/Orders/Admin", configs: configs)?.kind == .basic)
        #expect(ComposerEdit.authSource(collection: "Nope", configs: configs) == nil)
        #expect(ComposerEdit.inheritSummary(collection: "Shop/Orders", configs: configs) == "Using Bearer Token from Shop.")
        #expect(ComposerEdit.inheritSummary(collection: "Nope", configs: configs) == "No auth is set on the collection.")
        #expect(ComposerEdit.inheritSummary(collection: "", configs: configs).contains("not in a collection"))
    }

    @Test func chainIsDeepestFirst() {
        #expect(ComposerEdit.chain("A/B/C") == ["A/B/C", "A/B", "A"])
        #expect(ComposerEdit.chain("").isEmpty)
    }

    @Test func headerSuggestionsAndCounts() {
        #expect(ComposerEdit.headerSuggestions(prefix: "content-t") == ["Content-Type"])
        #expect(ComposerEdit.headerSuggestions(prefix: "Accept").contains("Accept-Encoding"))
        #expect(ComposerEdit.headerSuggestions(prefix: "").isEmpty)
        #expect(ComposerEdit.enabledCount([kv("a"), kv("b", off: true), kv("")]) == 1)
        #expect(ComposerEdit.fileName("/tmp/dir/receipt.pdf") == "receipt.pdf")
    }
}
