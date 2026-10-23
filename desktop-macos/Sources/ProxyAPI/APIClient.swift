import Foundation
import SwiftProtobuf

public typealias ClientMessage = Proxy_Api_V1_ClientMessage
public typealias ServerMessage = Proxy_Api_V1_ServerMessage

public enum APIError: Error, LocalizedError {
    case connect(String)
    case closed
    case server(code: String, message: String)
    case unexpected

    public var errorDescription: String? {
        switch self {
        case .connect(let m): return m
        case .closed: return "The connection to the core closed."
        case .server(_, let m): return m
        case .unexpected: return "Unexpected reply from the core."
        }
    }
}

/// One WebSocket connection to `proxyd` over its Unix socket. Requests are
/// matched to replies by id; pushed events (id 0) go to `onPush`.
public final class APIClient: @unchecked Sendable {
    public let socketPath: String
    public var onPush: (@Sendable (ServerMessage) -> Void)?
    public var onClose: (@Sendable () -> Void)?

    private var fd: Int32 = -1
    private let lock = NSLock()
    private let writeLock = NSLock()
    private var nextID: UInt64 = 1
    private var pending: [UInt64: CheckedContinuation<ServerMessage, Error>] = [:]
    private var closed = false

    public init(socketPath: String) { self.socketPath = socketPath }

    deinit { if fd >= 0 { Darwin.close(fd) } }

    /// Blocking: connect and finish the upgrade. Call off the main thread.
    public func connect() throws {
        let s = socket(AF_UNIX, SOCK_STREAM, 0)
        guard s >= 0 else { throw APIError.connect("socket: \(String(cString: strerror(errno)))") }
        var one: Int32 = 1
        setsockopt(s, SOL_SOCKET, SO_NOSIGPIPE, &one, socklen_t(MemoryLayout<Int32>.size))
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let bytes = Array(socketPath.utf8CString)
        guard bytes.count <= MemoryLayout.size(ofValue: addr.sun_path) else {
            Darwin.close(s)
            throw APIError.connect("socket path is too long: \(socketPath)")
        }
        withUnsafeMutableBytes(of: &addr.sun_path) { raw in
            for (i, c) in bytes.enumerated() { raw[i] = UInt8(bitPattern: c) }
        }
        let rc = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.connect(s, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard rc == 0 else {
            let m = String(cString: strerror(errno))
            Darwin.close(s)
            throw APIError.connect("connect \(socketPath): \(m)")
        }
        let key = Data((0..<16).map { _ in UInt8.random(in: 0...255) }).base64EncodedString()
        let req = "GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: \(key)\r\nSec-WebSocket-Version: 13\r\n\r\n"
        guard Self.writeAll(s, Data(req.utf8)) else {
            Darwin.close(s)
            throw APIError.connect("handshake write failed")
        }
        var head = Data()
        var buf = [UInt8](repeating: 0, count: 4096)
        let terminator = Data("\r\n\r\n".utf8)
        var leftover = Data()
        while true {
            let n = read(s, &buf, buf.count)
            if n <= 0 {
                Darwin.close(s)
                throw APIError.connect("handshake read failed")
            }
            head.append(buf, count: n)
            if let r = head.range(of: terminator) {
                leftover = head[r.upperBound...]
                head = head[..<r.lowerBound]
                break
            }
            if head.count > 16 * 1024 {
                Darwin.close(s)
                throw APIError.connect("handshake reply too long")
            }
        }
        guard String(decoding: head, as: UTF8.self).hasPrefix("HTTP/1.1 101") else {
            Darwin.close(s)
            throw APIError.connect("core refused the WebSocket upgrade")
        }
        fd = s
        let thread = Thread { [weak self] in self?.readLoop(initial: leftover) }
        thread.name = "proxy-api-reader"
        thread.qualityOfService = .userInitiated
        thread.start()
    }

    public func close() {
        lock.lock()
        let s = fd
        lock.unlock()
        if s >= 0 { shutdown(s, SHUT_RDWR) }
    }

    /// Send a request and wait for the reply with the same id. An error reply
    /// from the core becomes `APIError.server`.
    public func request(_ build: (inout ClientMessage) -> Void) async throws -> ServerMessage {
        var msg = ClientMessage()
        build(&msg)
        return try await withCheckedThrowingContinuation { cont in
            lock.lock()
            if closed || fd < 0 {
                lock.unlock()
                cont.resume(throwing: APIError.closed)
                return
            }
            let id = nextID
            nextID += 1
            pending[id] = cont
            lock.unlock()
            var withID = msg
            withID.id = id
            guard let payload = try? withID.serializedData(), sendFrame(.binary, payload) else {
                failPending(id)
                return
            }
        }
    }

    private func failPending(_ id: UInt64) {
        lock.lock()
        let c = pending.removeValue(forKey: id)
        lock.unlock()
        c?.resume(throwing: APIError.closed)
    }

    @discardableResult
    private func sendFrame(_ op: WSOpcode, _ payload: Data) -> Bool {
        writeLock.lock()
        defer { writeLock.unlock() }
        return Self.writeAll(fd, WSCodec.encode(op, payload))
    }

    private static func writeAll(_ fd: Int32, _ data: Data) -> Bool {
        data.withUnsafeBytes { raw in
            var off = 0
            while off < raw.count {
                let n = write(fd, raw.baseAddress! + off, raw.count - off)
                if n < 0 {
                    if errno == EINTR { continue }
                    return false
                }
                off += n
            }
            return true
        }
    }

    private func readLoop(initial: Data) {
        var decoder = WSDecoder()
        var buf = [UInt8](repeating: 0, count: 64 * 1024)
        var chunk = initial
        outer: while true {
            do {
                if !chunk.isEmpty {
                    for frame in try decoder.feed(chunk) {
                        switch frame.opcode {
                        case .binary:
                            if let m = try? ServerMessage(serializedBytes: frame.payload) { dispatch(m) }
                        case .ping: sendFrame(.pong, frame.payload)
                        case .close: break outer
                        default: break
                        }
                    }
                }
            } catch {
                break
            }
            let n = read(fd, &buf, buf.count)
            if n < 0 && errno == EINTR { chunk = Data(); continue }
            if n <= 0 { break }
            chunk = Data(buf[0..<n])
        }
        lock.lock()
        closed = true
        let waiting = pending
        pending = [:]
        lock.unlock()
        for (_, c) in waiting { c.resume(throwing: APIError.closed) }
        onClose?()
    }

    private func dispatch(_ m: ServerMessage) {
        if m.id != 0 {
            lock.lock()
            let c = pending.removeValue(forKey: m.id)
            lock.unlock()
            if let c {
                if case .error(let e)? = m.body {
                    c.resume(throwing: APIError.server(code: e.code, message: e.message))
                } else {
                    c.resume(returning: m)
                }
                return
            }
        }
        onPush?(m)
    }
}
