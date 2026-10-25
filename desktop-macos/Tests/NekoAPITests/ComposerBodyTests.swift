import Foundation
import Testing
@testable import NekoAPI

@Suite struct ComposerBodyTests {
    // MARK: JSON

    @Test func prettyKeepsKeyOrderAndLiterals() {
        let out = JSONFormat.pretty(#"{"z":1,"a":[true,null,{"k":"v"}],"n":-1.50e+3,"e":{},"l":[]}"#)
        #expect(out == """
        {
          "z": 1,
          "a": [
            true,
            null,
            {
              "k": "v"
            }
          ],
          "n": -1.50e+3,
          "e": {},
          "l": []
        }
        """)
    }

    @Test func prettyKeepsStringsVerbatim() {
        let src = #"{"s":"tab\t \u00e9 \"q\" é 😀"}"#
        let out = JSONFormat.pretty(src)
        #expect(out == "{\n  \"s\": \"tab\\t \\u00e9 \\\"q\\\" é 😀\"\n}")
    }

    @Test func minifyRoundTrips() {
        let src = "{\n  \"a\": [1, 2, 3],\n  \"b\": {\"c\": \"d e\"}\n}"
        #expect(JSONFormat.minified(src) == #"{"a":[1,2,3],"b":{"c":"d e"}}"#)
        #expect(JSONFormat.pretty(JSONFormat.minified(src)!) == JSONFormat.pretty(src))
    }

    @Test func topLevelScalarsAreValid() {
        #expect(JSONFormat.pretty(" 42 ") == "42")
        #expect(JSONFormat.pretty("\"x\"") == "\"x\"")
        #expect(JSONFormat.pretty("null") == "null")
    }

    @Test func rejectsInvalidJSON() {
        for bad in ["", "{", "{\"a\":}", "{'a':1}", "[1,]", "{\"a\":1,}", "01", "1.", "tru", "{\"a\" 1}", "[1] x", "\"a\nb\"", "\"\\x\"", "-", "{\"a\":1}}"] {
            #expect(JSONFormat.pretty(bad) == nil, "\(bad)")
            #expect(JSONFormat.error(bad) != nil, "\(bad)")
        }
    }

    @Test func errorsNamePosition() {
        #expect(JSONFormat.error("{\n  \"a\": ?\n}")?.contains("line 2, column 8") == true)
        #expect(JSONFormat.error("{\"a\":")?.contains("end of input") == true)
    }

    @Test func validationSkipsEmptyAndPlaceholders() {
        #expect(JSONFormat.validationMessage("  \n") == nil)
        #expect(JSONFormat.validationMessage("{\"id\": {{id}}}") == nil)
        #expect(JSONFormat.validationMessage("{\"id\": 1}") == nil)
        #expect(JSONFormat.validationMessage("{\"id\": }") != nil)
    }

    @Test func deepNestingFailsInsteadOfCrashing() {
        let deep = String(repeating: "[", count: 2000)
        #expect(JSONFormat.error(deep) != nil)
    }

    // MARK: tinting

    private func kinds(_ text: String, _ l: BodySyntax.Language) -> [(String, BodySyntax.Kind)] {
        let u = Array(text.utf16)
        return BodySyntax.tokens(text, l).map { (String(decoding: u[$0.location..<$0.location + $0.length], as: UTF16.self), $0.kind) }
    }

    @Test func jsonTokens() {
        let t = kinds(#"{"a": [1, true, "x"]}"#, .json)
        #expect(t.contains { $0 == ("\"a\"", .key) })
        #expect(t.contains { $0 == ("1", .number) })
        #expect(t.contains { $0 == ("true", .literal) })
        #expect(t.contains { $0 == ("\"x\"", .string) })
    }

    @Test func variablesAreMarkedInEveryLanguage() {
        for l in [BodySyntax.Language.plain, .json, .xml, .javascript, .graphql] {
            #expect(kinds("a {{host}} b", l).contains { $0 == ("{{host}}", .variable) }, "\(l)")
        }
    }

    @Test func xmlTokens() {
        let t = kinds("<a href=\"x\">&amp;<!-- c --></a>", .xml)
        #expect(t.contains { $0 == ("<a", .tag) })
        #expect(t.contains { $0 == ("href", .attr) })
        #expect(t.contains { $0 == ("\"x\"", .value) })
        #expect(t.contains { $0 == ("&amp;", .entity) })
        #expect(t.contains { $0 == ("<!-- c -->", .comment) })
        #expect(t.contains { $0 == ("</a", .tag) })
    }

    @Test func tokenizersNeverCrashOnTruncatedInput() {
        for l in [BodySyntax.Language.json, .xml, .javascript, .graphql] {
            for s in ["\"abc", "<a b=\"", "<!-- x", "/* x", "`a", "{{", "\\", "<", "&", "-"] {
                _ = BodySyntax.tokens(s, l)
            }
        }
    }

    // MARK: modes

    @Test func bodyModesMapKinds() {
        #expect(BodyMode(.json) == .raw && BodyMode(.xml) == .raw && BodyMode(.text) == .raw)
        #expect(BodyMode(.form) == .urlEncoded && BodyMode(.formData) == .formData)
        #expect(BodyMode(.none) == .none && BodyMode(.binary) == .binary && BodyMode(.graphql) == .graphql)
        #expect(BodyMode.raw.kind(raw: .xml) == .xml)
        #expect(BodyMode.raw.kind(raw: .form) == .json)
        #expect(BodyMode.urlEncoded.kind(raw: .xml) == .form)
        #expect(BodyMode.allCases.map(\.rawValue) == Array(0..<6))
    }

    // MARK: assertions

    @Test func menusFollowTheProtoOrder() {
        #expect(AssertionSchema.kinds.map(\.rawValue) == Array(0..<6))
        for k in AssertionSchema.kinds {
            let raw = AssertionSchema.operators(for: k).map(\.rawValue)
            #expect(raw == raw.sorted(), "\(k)")
            #expect(AssertionSchema.operators(for: k).contains(AssertionSchema.defaultOp(for: k)))
        }
    }

    @Test func operatorsPerKind() {
        typealias Op = Neko_Api_V1_AssertOp
        #expect(AssertionSchema.operators(for: .status) == [.equals, .notEquals, .lessThan, .greaterThan])
        #expect(AssertionSchema.operators(for: .time) == [.equals, .lessThan, .greaterThan])
        #expect(AssertionSchema.operators(for: .size) == [.equals, .lessThan, .greaterThan])
        #expect(!AssertionSchema.operators(for: .header).contains(Op.lessThan))
        #expect(!AssertionSchema.operators(for: .body).contains(Op.notExists))
        #expect(AssertionSchema.operators(for: .json).count == 9)
    }

    @Test func changingKindFixesOperatorAndTarget() {
        let a = Neko_Api_V1_Assertion.with { $0.kind = .json; $0.target = "a.b"; $0.op = .exists }
        let h = AssertionSchema.changing(a, to: .header)
        #expect(h.kind == .header && h.target == "a.b" && h.op == .exists)
        let t = AssertionSchema.changing(a, to: .time)
        #expect(t.op == .lessThan && t.target.isEmpty)
        let o = AssertionSchema.changing(Neko_Api_V1_Assertion.with { $0.expected = "5"; $0.op = .equals }, to: Neko_Api_V1_AssertOp.exists)
        #expect(o.expected.isEmpty)
    }

    @Test func describesAssertions() {
        #expect(AssertionSchema.describe(.with { $0.kind = .status; $0.op = .equals; $0.expected = "200" }) == "Status equals 200")
        #expect(AssertionSchema.describe(.with { $0.kind = .json; $0.target = "data.id"; $0.op = .exists }) == "JSON data.id exists")
        #expect(AssertionSchema.describe(.with { $0.kind = .time; $0.op = .lessThan; $0.expected = "500" }) == "Response time is less than 500 ms")
    }

    @Test func presetsAreValid() {
        for p in AssertionSchema.presets {
            #expect(AssertionSchema.operators(for: p.assertion.kind).contains(p.assertion.op), "\(p.title)")
        }
    }
}
