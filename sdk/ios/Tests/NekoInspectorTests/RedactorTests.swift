import Foundation
import Testing
@testable import NekoInspector

struct RedactorTests {
    let r = Redactor(level: RedactionLevel.default, extraKeys: ["national_id"])

    @Test func sensitiveHeadersAreHidden() {
        for h in ["Authorization", "Cookie", "Set-Cookie", "Proxy-Authorization", "X-Api-Key", "X-Auth-Token", "apikey", "X-Client-Secret"] {
            #expect(r.fork().header(h, "v") == "[redacted]", "\(h)")
        }
        #expect(r.fork().header("Content-Type", "application/json") == "application/json")
    }

    @Test func jsonValuesUnderSensitiveKeysAreReplacedWhateverTheirType() {
        let x = r.fork()
        let out = x.redactJson(#"{"user":"a","password":"p w","token":123,"nested":{"refresh_token":"t","ok":true},"card_number":["4111"],"national_id":{"a":1},"keep":null}"#)
        #expect(out == #"{"user":"a","password":"[redacted]","token":"[redacted]","nested":{"refresh_token":"[redacted]","ok":true},"card_number":"[redacted]","national_id":"[redacted]","keep":null}"#)
        #expect(x.changed)
    }

    @Test func jsonScannerSurvivesGarbage() {
        let x = r.fork()
        #expect(x.redactJson("not json at all") == "not json at all")
        #expect(x.redactJson(#"{"a":"unterminated"#) == #"{"a":"unterminated"#)
        #expect(!r.fork().also { _ = $0.redactJson(#"{"a":1}"#) }.changed)
    }

    @Test func formBodiesAndUrls() {
        let x = r.fork()
        #expect(String(decoding: x.body(contentType: "application/x-www-form-urlencoded", Data("a=1&password=hunter2&b=2".utf8)), as: UTF8.self) == "a=1&password=[redacted]&b=2")
        #expect(x.url("https://x.test/p?q=1&access_token=abc#f") == "https://x.test/p?q=1&access_token=[redacted]#f")
        #expect(r.fork().url("https://x.test/p?q=1") == "https://x.test/p?q=1")
    }

    @Test func aJsonBodyWithNoContentTypeIsStillRedacted() {
        let out = r.fork().body(contentType: "", Data(#"{"password":"x"}"#.utf8))
        #expect(String(decoding: out, as: UTF8.self) == #"{"password":"[redacted]"}"#)
    }

    @Test func strictAndNone() {
        let strict = Redactor(level: RedactionLevel.strict, extraKeys: [])
        #expect(strict.fork().header("X-Request-Id", "1") == "[redacted]")
        #expect(strict.fork().header("Accept", "v") == "v")
        #expect(!strict.bodiesAllowed)
        let none = Redactor(level: RedactionLevel.none, extraKeys: [])
        #expect(none.fork().header("Authorization", "Bearer t") == "Bearer t")
    }
}

private extension Redactor { func also(_ f: (Redactor) -> Void) -> Redactor { f(self); return self } }
