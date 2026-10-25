import Foundation

/// History entries grouped by calendar day, newest first.
public struct HistoryGroup: Equatable {
    public var title: String
    /// Indexes into the history array, in their original order.
    public var indices: [Int]
    public init(title: String, indices: [Int]) { self.title = title; self.indices = indices }
}

public enum HistoryGrouping {
    public static func groups(sentMs: [Int64], now: Date = Date(), calendar: Calendar = .current, locale: Locale = .current) -> [HistoryGroup] {
        let f = DateFormatter()
        f.locale = locale
        f.calendar = calendar
        f.setLocalizedDateFormatFromTemplate("MMM d, yyyy")
        let today = calendar.startOfDay(for: now)
        var out: [HistoryGroup] = []
        var lastKey: String?
        for (i, ms) in sentMs.enumerated() {
            let title: String
            if ms <= 0 {
                title = "Earlier"
            } else {
                let date = Date(timeIntervalSince1970: Double(ms) / 1000)
                let day = calendar.startOfDay(for: date)
                let diff = calendar.dateComponents([.day], from: day, to: today).day ?? 0
                if diff == 0 { title = "Today" }
                else if diff == 1 { title = "Yesterday" }
                else { title = f.string(from: date) }
            }
            if title == lastKey, !out.isEmpty { out[out.count - 1].indices.append(i) }
            else { out.append(HistoryGroup(title: title, indices: [i])); lastKey = title }
        }
        return out
    }

    /// `/v1/me?x=1` from a full URL, or the text itself when it does not parse.
    public static func pathAndHost(_ url: String) -> (host: String, path: String) {
        guard let c = URLComponents(string: url), let host = c.host else { return ("", url) }
        var p = c.percentEncodedPath.isEmpty ? "/" : c.percentEncodedPath
        if let q = c.percentEncodedQuery, !q.isEmpty { p += "?" + q }
        return (host + (c.port.map { ":\($0)" } ?? ""), p)
    }
}
