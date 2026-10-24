import Foundation
import SwiftProtobuf

/// Rules saved to a file and loaded back: the protobuf JSON of the Rules answer
/// (rules, Map Local and SSL proxying), without the parts that belong to this
/// machine (pinned hosts, MCP write mode).
public enum RulesBundle {
    public struct Invalid: Error, LocalizedError {
        public let message: String
        public var errorDescription: String? { message }
    }

    public struct Contents {
        public var rules: [Neko_Api_V1_Rule]
        public var mapLocal: [Neko_Api_V1_MapLocalRule]
        public var ssl: Neko_Api_V1_SslSettings?
    }

    public static func export(_ r: Neko_Api_V1_RulesResponse) throws -> Data {
        var b = r
        b.pinnedHosts = []
        b.mcpWrites = .init()
        var o = JSONEncodingOptions()
        o.preserveProtoFieldNames = true
        let compact = try b.jsonString(options: o)
        // Re-indent so the file diffs well in git.
        let any = try JSONSerialization.jsonObject(with: Data(compact.utf8))
        return try JSONSerialization.data(withJSONObject: any, options: [.prettyPrinted, .sortedKeys])
    }

    /// Ids are cleared so importing never overwrites a rule that happens to share one.
    public static func parse(_ data: Data) throws -> Contents {
        var o = JSONDecodingOptions()
        o.ignoreUnknownFields = true
        let r: Neko_Api_V1_RulesResponse
        do { r = try Neko_Api_V1_RulesResponse(jsonUTF8Data: data, options: o) } catch {
            throw Invalid(message: "This is not a Neko rules file.")
        }
        if r.rules.isEmpty && r.mapLocal.isEmpty && !r.hasSsl {
            throw Invalid(message: "The file has no rules in it.")
        }
        return Contents(
            rules: r.rules.map { var x = $0; x.id = ""; return x },
            mapLocal: r.mapLocal.map { var x = $0; x.id = ""; return x },
            ssl: r.hasSsl ? r.ssl : nil)
    }
}
