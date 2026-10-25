import Foundation
import Testing
@testable import NekoAPI
@testable import NekoApp
import AppKit

@Suite struct ComposerResponseTests {
    @Test func searchIsCaseInsensitiveAndOrdered() {
        let r = BodySearch.ranges(in: "Item item ITEM items", query: "item")
        #expect(r.map(\.location) == [0, 5, 10, 15])
        #expect(r.allSatisfy { $0.length == 4 })
    }

    @Test func searchHandlesEmptyAndMissing() {
        #expect(BodySearch.ranges(in: "abc", query: "").isEmpty)
        #expect(BodySearch.ranges(in: "", query: "a").isEmpty)
        #expect(BodySearch.ranges(in: "abc", query: "z").isEmpty)
    }

    @Test func searchDoesNotOverlapAndIsCapped() {
        #expect(BodySearch.ranges(in: "aaaa", query: "aa").count == 2)
        let big = String(repeating: "a", count: 10_000)
        #expect(BodySearch.ranges(in: big, query: "a", limit: 50).count == 50)
    }

    @Test func searchUsesUTF16Offsets() {
        let r = BodySearch.ranges(in: "😀 needle", query: "needle")
        #expect(r.first?.location == 3)
    }

    @Test func steppingWraps() {
        #expect(BodySearch.step(0, by: -1, count: 5) == 4)
        #expect(BodySearch.step(4, by: 1, count: 5) == 0)
        #expect(BodySearch.step(2, by: 1, count: 5) == 3)
        #expect(BodySearch.step(0, by: 1, count: 0) == 0)
    }

    @Test func summaryText() {
        #expect(BodySearch.summary(current: 0, count: 0, query: "") == "")
        #expect(BodySearch.summary(current: 0, count: 0, query: "x") == "No results")
        #expect(BodySearch.summary(current: 2, count: 12, query: "x") == "3 of 12")
    }

    @Test func linesAreBoundedAndCountTheRest() {
        let text = (1...10).map(String.init).joined(separator: "\n")
        let r = BodyText.lines(text, maxLines: 4)
        #expect(r.lines.map(String.init) == ["1", "2", "3", "4"])
        #expect(r.more == 6)
        let all = BodyText.lines(text, maxLines: 100)
        #expect(all.lines.count == 10 && all.more == 0)
    }

    @Test func linesKeepBlankLinesAndBoundCharacters() {
        #expect(BodyText.lines("a\n\nb", maxLines: 10).lines.map(String.init) == ["a", "", "b"])
        let long = String(repeating: "x", count: 1000) + "\n" + String(repeating: "y", count: 1000) + "\nz"
        let r = BodyText.lines(long, maxLines: 10, maxChars: 500)
        #expect(r.lines.count == 1)
        #expect(r.more == 2)
    }

    @Test func testSummary() {
        let s = TestSummary(passed: 2, total: 3)
        #expect(s.line == "2 of 3 passed" && s.badge == "2/3" && !s.allPassed && s.failed == 1)
        #expect(TestSummary(passed: 3, total: 3).line == "All 3 passed")
        #expect(TestSummary(passed: 1, total: 1).line == "1 test passed")
        #expect(TestSummary(passed: 0, total: 0).line == "" && !TestSummary(passed: 0, total: 0).allPassed)
    }

    @Test func fileNames() {
        #expect(bodyFileName(kind: "json", contentType: "application/json") == "response.json")
        #expect(bodyFileName(kind: "image", contentType: "image/png") == "response.png")
        #expect(bodyFileName(kind: "text", contentType: "application/xml") == "response.xml")
        #expect(bodyFileName(kind: "binary", contentType: "") == "response.txt")
    }
}

@MainActor
@Suite struct ComposerResponseViewTests {
    private func json() -> Neko_Api_V1_BodyView {
        .with {
            $0.kind = .json; $0.contentType = "application/json"; $0.decodedSize = 40
            $0.text = "{\n  \"name\": \"Neko\",\n  \"tags\": [\"a\", \"b\"]\n}"
            $0.json = [
                .with { $0.depth = 0; $0.foldEnd = 4; $0.tokens = [.with { $0.text = "{"; $0.kind = .punct }] },
                .with { $0.depth = 1; $0.foldEnd = -1; $0.tokens = [.with { $0.text = "\"name\""; $0.kind = .key }, .with { $0.text = ": "; $0.kind = .punct }, .with { $0.text = "\"Neko\""; $0.kind = .string }] },
                .with { $0.depth = 0; $0.foldEnd = -1; $0.tokens = [.with { $0.text = "}"; $0.kind = .punct }] },
            ]
        }
    }

    private func host(_ v: NSView, width: CGFloat = 800, height: CGFloat = 400) {
        v.frame = NSRect(x: 0, y: 0, width: width, height: height)
        v.layoutSubtreeIfNeeded()
    }

    @Test func bodyViewerBuildsEveryKind() {
        var bodies: [Neko_Api_V1_BodyView] = [json()]
        bodies.append(.with { $0.kind = .html; $0.text = "<p>hi</p>"; $0.source = [.with { $0.tokens = [.with { $0.text = "<p>"; $0.kind = .srcTag }] }] })
        bodies.append(.with { $0.kind = .text; $0.text = "a\nb\nc" })
        bodies.append(.with { $0.kind = .binary; $0.note = "12 bytes" })
        bodies.append(.with { $0.kind = .form; $0.form = [.with { $0.name = "a"; $0.value = "1" }] })
        bodies.append(.init())
        for style in [BodyViewer.Style.card, .pane] {
            for b in bodies {
                let v = BodyViewer(body: b, style: style)
                host(v)
                #expect(v.fittingSize.width >= 0)
            }
        }
    }

    @Test func searchFindsAndStepsInTheDisplayedText() throws {
        let state = BodyViewState()
        state.query = "neko"
        let v = BodyViewer(body: json(), state: state, style: .pane)
        host(v)
        let tv = try #require(allSubviews(v).compactMap { $0 as? NSTextView }.first)
        #expect(BodySearch.ranges(in: tv.string, query: "neko").count == 1)
        state.mode = .raw
        let v2 = BodyViewer(body: json(), state: state, style: .pane)
        host(v2)
        #expect(state.mode == .raw)
    }

    @Test func foldingChangesTheShownLines() throws {
        let state = BodyViewState()
        let v = BodyViewer(body: json(), state: state)
        host(v)
        let before = try #require(allSubviews(v).compactMap { $0 as? NSTextView }.first).string
        state.folds = [0]
        let v2 = BodyViewer(body: json(), state: state)
        host(v2)
        let after = try #require(allSubviews(v2).compactMap { $0 as? NSTextView }.first).string
        #expect(after.count < before.count)
    }

    @Test func paneShowsEachPhase() {
        let model = ComposerModel()
        let pane = ResponsePane(model: model)
        host(pane)
        model.tab.sending = true
        pane.reload(); host(pane)
        model.tab.sending = false
        model.tab.sendError = "connection refused"
        model.tab.sent = .with { $0.flowID = 7; $0.error = "connection refused" }
        pane.reload(); host(pane)
        model.tab.sendError = nil
        model.tab.sent = .with { $0.status = 200; $0.tests = [.with { $0.name = "ok"; $0.passed = true }] }
        for i in 0..<5 {
            model.tab.responseSection = i
            pane.reload(); host(pane)
        }
        model.tab.responseSection = 3
        pane.reload()
        #expect(model.tab.responseSection == 3)
        model.tab.responseSection = 99
        pane.reload()
        #expect(model.tab.responseSection == 4)
    }

    private func allSubviews(_ v: NSView) -> [NSView] { v.subviews + v.subviews.flatMap(allSubviews) }
}
