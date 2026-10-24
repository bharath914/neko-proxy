import Foundation
import Testing
@testable import NekoAPI

@Suite struct RulesBundleTests {
    @Test func roundTripClearsIdsAndMachineState() throws {
        var r = Neko_Api_V1_RulesResponse()
        r.rules = [.with { $0.id = "r1"; $0.name = "down"; $0.enabled = true }]
        r.mapLocal = [.with { $0.id = "m1"; $0.host = "h"; $0.file = "/f"; $0.status = 404 }]
        r.ssl = .with { $0.mode = .sslListed; $0.hosts = ["a.com"] }
        r.pinnedHosts = ["pinned.com"]
        let data = try RulesBundle.export(r)
        let text = String(decoding: data, as: UTF8.self)
        #expect(!text.contains("pinned.com"))
        let c = try RulesBundle.parse(data)
        #expect(c.rules.count == 1 && c.rules[0].id.isEmpty && c.rules[0].name == "down")
        #expect(c.mapLocal[0].id.isEmpty && c.mapLocal[0].status == 404)
        #expect(c.ssl?.hosts == ["a.com"] && c.ssl?.mode == .sslListed)
    }

    @Test func rejectsJunkAndEmpty() {
        #expect(throws: RulesBundle.Invalid.self) { try RulesBundle.parse(Data("nope".utf8)) }
        #expect(throws: RulesBundle.Invalid.self) { try RulesBundle.parse(Data("{}".utf8)) }
    }
}
