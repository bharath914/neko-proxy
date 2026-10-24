import Foundation

/// Fields of the Map Local editor. The core validates; this only converts text.
public struct LocalForm: Equatable {
    public struct Invalid: Error { public let message: String
        public init(message: String) { self.message = message } }

    public var id = ""
    public var enabled = true
    public var host = ""
    public var port = ""
    public var path = ""
    public var query = ""
    public var file = ""
    public var status = "200"
    public var contentType = ""

    public init() {}

    public init(_ m: Neko_Api_V1_MapLocalRule) {
        id = m.id; enabled = m.enabled; host = m.host
        port = m.port == 0 ? "" : String(m.port)
        path = m.path; query = m.query; file = m.file
        status = m.status == 0 ? "200" : String(m.status)
        contentType = m.contentType
    }

    public func toRule() throws -> Neko_Api_V1_MapLocalRule {
        let h = host.trimmingCharacters(in: .whitespaces)
        if h.isEmpty { throw Invalid(message: "Enter the host to match, for example api.example.com.") }
        let f = file.trimmingCharacters(in: .whitespaces)
        if f.isEmpty { throw Invalid(message: "Choose the file to serve.") }
        var p: UInt32 = 0
        if !port.trimmingCharacters(in: .whitespaces).isEmpty {
            guard let v = UInt32(port.trimmingCharacters(in: .whitespaces)), v <= 65535 else { throw Invalid(message: "Port must be a number from 1 to 65535.") }
            p = v
        }
        var s: UInt32 = 0
        if !status.trimmingCharacters(in: .whitespaces).isEmpty {
            guard let v = UInt32(status.trimmingCharacters(in: .whitespaces)), (100...599).contains(v) else { throw Invalid(message: "Status must be a number from 100 to 599.") }
            s = v
        }
        return .with {
            $0.id = id; $0.enabled = enabled; $0.host = h; $0.port = p
            $0.path = path.trimmingCharacters(in: .whitespaces)
            $0.query = query.trimmingCharacters(in: .whitespaces)
            $0.file = f; $0.status = s
            $0.contentType = contentType.trimmingCharacters(in: .whitespaces)
        }
    }
}

/// The SSL proxying editor's working copy: hosts are one per line.
public struct SslDraft: Equatable {
    public var listed: Bool
    public var hosts: String
    public var exclude: String

    public init(_ s: Neko_Api_V1_SslSettings) {
        listed = s.mode == .sslListed
        hosts = s.hosts.joined(separator: "\n")
        exclude = s.exclude.joined(separator: "\n")
    }

    static func lines(_ s: String) -> [String] {
        s.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
    }

    public var settings: Neko_Api_V1_SslSettings {
        .with {
            $0.mode = listed ? .sslListed : .sslAll
            $0.hosts = Self.lines(hosts)
            $0.exclude = Self.lines(exclude)
        }
    }
}
