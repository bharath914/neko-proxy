import Foundation

/// Subsequence match for the command palette: every query character must appear in
/// order. Higher is better; nil means no match. Prefix and word-start hits and
/// consecutive runs score more, so "rec" finds "Record" before "Rules screen".
public func fuzzyScore(_ query: String, _ text: String) -> Int? {
    let q = Array(query.lowercased().filter { !$0.isWhitespace })
    if q.isEmpty { return 0 }
    let t = Array(text.lowercased())
    var qi = 0, score = 0, run = 0
    var prev: Character = " "
    for (i, c) in t.enumerated() {
        if qi < q.count, c == q[qi] {
            run += 1
            score += 1 + run * 2
            if i == 0 { score += 8 }
            else if !prev.isLetter && !prev.isNumber { score += 5 }
            qi += 1
        } else {
            run = 0
        }
        prev = c
    }
    guard qi == q.count else { return nil }
    return score - t.count / 8
}
