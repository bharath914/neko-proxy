import Foundation
import Testing
@testable import NekoAPI

@Suite struct LocalFormTests {
    @Test func validatesAndConverts() throws {
        var f = LocalForm()
        #expect(throws: LocalForm.Invalid.self) { try f.toRule() }
        f.host = " api.example.com "; f.file = "/tmp/a.json"; f.port = "8443"; f.status = ""
        let r = try f.toRule()
        #expect(r.host == "api.example.com" && r.port == 8443 && r.status == 0 && r.enabled)
        f.port = "99999"
        #expect(throws: LocalForm.Invalid.self) { try f.toRule() }
        f.port = ""; f.status = "42"
        #expect(throws: LocalForm.Invalid.self) { try f.toRule() }
    }

    @Test func roundTripsExistingRule() throws {
        let m = Neko_Api_V1_MapLocalRule.with { $0.id = "x"; $0.host = "h"; $0.file = "/f"; $0.status = 404; $0.enabled = false }
        let back = try LocalForm(m).toRule()
        #expect(back.id == "x" && back.status == 404 && !back.enabled)
    }

    @Test func sslDraftParsesLines() {
        var d = SslDraft(.init())
        #expect(!d.listed)
        d.listed = true; d.hosts = " a.com \n\n*.b.com\n"; d.exclude = "bank.com"
        let s = d.settings
        #expect(s.mode == .sslListed && s.hosts == ["a.com", "*.b.com"] && s.exclude == ["bank.com"])
        #expect(SslDraft(s) == d.normalized)
    }
}

private extension SslDraft {
    var normalized: SslDraft { SslDraft(settings) }
}
