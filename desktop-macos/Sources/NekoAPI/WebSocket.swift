import Foundation

/// Minimal RFC 6455 framing. The core only speaks binary frames over a Unix
/// socket, so this covers masking, fragmentation and control frames and nothing else.
public enum WSOpcode: UInt8 {
    case continuation = 0, text = 1, binary = 2, close = 8, ping = 9, pong = 10
}

public struct WSFrame: Equatable {
    public var opcode: WSOpcode
    public var payload: Data
    public init(opcode: WSOpcode, payload: Data) {
        self.opcode = opcode
        self.payload = payload
    }
}

public enum WSCodec {
    /// Client frames are always masked.
    public static func encode(_ opcode: WSOpcode, _ payload: Data, mask: [UInt8]? = nil) -> Data {
        var out = Data()
        out.append(0x80 | opcode.rawValue)
        let n = payload.count
        if n < 126 {
            out.append(0x80 | UInt8(n))
        } else if n <= 0xFFFF {
            out.append(0x80 | 126)
            out.append(UInt8(n >> 8))
            out.append(UInt8(n & 0xFF))
        } else {
            out.append(0x80 | 127)
            for shift in stride(from: 56, through: 0, by: -8) {
                out.append(UInt8((UInt64(n) >> UInt64(shift)) & 0xFF))
            }
        }
        let key = mask ?? (0..<4).map { _ in UInt8.random(in: 0...255) }
        out.append(contentsOf: key)
        var masked = [UInt8](payload)
        for i in masked.indices { masked[i] ^= key[i & 3] }
        out.append(contentsOf: masked)
        return out
    }
}

/// Incremental decoder: feed it whatever `read` returned, take whole messages out.
public struct WSDecoder {
    public enum Failure: Error { case protocolViolation(String), tooLarge }
    private var buffer = Data()
    private var message = Data()
    private var messageOpcode: WSOpcode?
    private let maxMessage: Int

    public init(maxMessage: Int = 256 * 1024 * 1024) { self.maxMessage = maxMessage }

    /// Returns complete data messages and control frames in arrival order.
    public mutating func feed(_ bytes: Data) throws -> [WSFrame] {
        buffer.append(bytes)
        var frames: [WSFrame] = []
        while let (fin, op, payload, used) = try parseOne() {
            buffer.removeFirst(used)
            switch op {
            case .ping, .pong, .close:
                frames.append(WSFrame(opcode: op, payload: payload))
            case .text, .binary, .continuation:
                if op != .continuation {
                    guard messageOpcode == nil else { throw Failure.protocolViolation("new message inside a fragmented one") }
                    messageOpcode = op
                } else if messageOpcode == nil {
                    throw Failure.protocolViolation("continuation without a start")
                }
                message.append(payload)
                if message.count > maxMessage { throw Failure.tooLarge }
                if fin {
                    frames.append(WSFrame(opcode: messageOpcode!, payload: message))
                    message = Data()
                    messageOpcode = nil
                }
            }
        }
        return frames
    }

    private func parseOne() throws -> (Bool, WSOpcode, Data, Int)? {
        let b = buffer
        guard b.count >= 2 else { return nil }
        let s = b.startIndex
        let fin = b[s] & 0x80 != 0
        guard let op = WSOpcode(rawValue: b[s] & 0x0F) else { throw Failure.protocolViolation("opcode") }
        let masked = b[s + 1] & 0x80 != 0
        var len = Int(b[s + 1] & 0x7F)
        var pos = 2
        if len == 126 {
            guard b.count >= 4 else { return nil }
            len = Int(b[s + 2]) << 8 | Int(b[s + 3])
            pos = 4
        } else if len == 127 {
            guard b.count >= 10 else { return nil }
            var v: UInt64 = 0
            for i in 0..<8 { v = v << 8 | UInt64(b[s + 2 + i]) }
            guard v <= UInt64(maxMessage) else { throw Failure.tooLarge }
            len = Int(v)
            pos = 10
        }
        var key: [UInt8] = []
        if masked {
            guard b.count >= pos + 4 else { return nil }
            key = [b[s + pos], b[s + pos + 1], b[s + pos + 2], b[s + pos + 3]]
            pos += 4
        }
        guard b.count >= pos + len else { return nil }
        var payload = [UInt8](b[(s + pos)..<(s + pos + len)])
        if masked { for i in payload.indices { payload[i] ^= key[i & 3] } }
        return (fin, op, Data(payload), pos + len)
    }
}
