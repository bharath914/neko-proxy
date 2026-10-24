import Foundation
import Testing
@testable import NekoAPI

@Suite struct EditTests {
    @Test func headersRoundTripAndSkipJunk() {
        let h = textToHeaders("accept: a/b\n\nnonsense\n : x\nx-n:  v:w ")
        #expect(h.map(\.name) == ["accept", "x-n"])
        #expect(h[1].value == "v:w")
        #expect(headersToText(h) == "accept: a/b\nx-n: v:w")
    }

    @Test func rewriteEditsParse() throws {
        let e = try textToEdits("set X-Env: staging\nADD A: 1\nremove Cookie")
        #expect(e.map(\.op) == [.set, .add, .remove])
        #expect(e[0].value == "staging" && e[2].name == "Cookie")
        #expect(editsToText(e) == "set X-Env: staging\nadd A: 1\nremove Cookie")
        #expect(throws: RuleForm.Invalid.self) { try textToEdits("frobnicate x") }
        #expect(throws: RuleForm.Invalid.self) { try textToEdits("set nocolon") }
    }

    @Test func mockFormMakesRuleAndBack() throws {
        var f = RuleForm(kind: .mock)
        f.host = "api.example.com"; f.method = "POST"; f.path = "/v1/checkout"
        f.status = "503"; f.delayMs = "300"; f.headers = "content-type: text/plain"; f.body = "down"
        let r = try f.toRule()
        #expect(r.mock.status == 503 && r.mock.delayMs == 300 && r.matcher.method == "POST")
        #expect(describeAction(r) == "Answer 503, after 300 ms")
        #expect(describeMatch(r.matcher) == "POST api.example.com/v1/checkout")
        #expect(RuleForm(r).status == "503")
    }

    @Test func formValidation() {
        var f = RuleForm(kind: .throttle)
        #expect(throws: RuleForm.Invalid(message: "Enter a host (use * for any host)")) { try f.toRule() }
        f.host = "*"; f.delayMs = "abc"
        #expect(throws: RuleForm.Invalid(message: "Delay must be a number")) { try f.toRule() }
    }

    @Test func breakpointSendsOnlyChanges() {
        var p = PausedFlow()
        p.flowID = 7; p.phase = .phaseRequest; p.method = "POST"; p.path = "/v1/login"
        p.headers = textToHeaders("a: 1"); p.body = Data("{}".utf8)
        var e = BreakpointEdit(p)
        #expect(!e.changed(p))
        e.path = "/v2/login"; e.body = "{\"x\":1}"
        let r = e.resume(p)
        #expect(r.path == "/v2/login" && !r.hasMethod && !r.replaceHeaders && r.hasBody)
        var bin = p; bin.body = Data([0xff, 0xfe])
        #expect(BreakpointEdit(bin).binary)
    }
}
