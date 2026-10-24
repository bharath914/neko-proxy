import Testing
@testable import NekoAPI

@Suite struct FuzzyTests {
    @Test func subsequenceMatchesInOrder() {
        #expect(fuzzyScore("rcd", "Record") != nil)
        #expect(fuzzyScore("dcr", "Record") == nil)
        #expect(fuzzyScore("", "anything") == 0)
    }

    @Test func prefixAndWordStartsRankHigher() throws {
        let a = try #require(fuzzyScore("rec", "Resume capture"))
        let b = try #require(fuzzyScore("rec", "Pre-recorded sessions"))
        #expect(a > b)
        let c = try #require(fuzzyScore("gr", "Go to Rules"))
        let d = try #require(fuzzyScore("gr", "Programming"))
        #expect(c > d)
    }

    @Test func caseAndSpacesAreIgnored() {
        #expect(fuzzyScore("GO TO", "go to rules") != nil)
    }
}
