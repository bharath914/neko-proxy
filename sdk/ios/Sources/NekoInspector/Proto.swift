#if DEBUG
import Foundation

/// Just enough protobuf for `proto/sdk.proto`: varints and length-delimited fields, proto3 rules
/// (zero and empty values are not written), so the bytes match the Rust side.
struct ProtoWriter {
    private(set) var data = Data()

    private mutating func varint(_ value: UInt64) {
        var v = value
        while v >= 0x80 { data.append(UInt8(v & 0x7F) | 0x80); v >>= 7 }
        data.append(UInt8(v))
    }

    private mutating func tag(_ field: Int, _ wire: Int) { varint(UInt64(field << 3 | wire)) }

    /// int32, int64, uint32, uint64, enum. A negative int32 is the 10-byte form, as protobuf specifies.
    mutating func int(_ field: Int, _ v: Int64) {
        if v == 0 { return }
        tag(field, 0); varint(UInt64(bitPattern: v))
    }
    mutating func int(_ field: Int, _ v: Int) { int(field, Int64(v)) }
    mutating func bool(_ field: Int, _ v: Bool) { if v { tag(field, 0); varint(1) } }
    mutating func bytes(_ field: Int, _ v: Data) {
        if v.isEmpty { return }
        tag(field, 2); varint(UInt64(v.count)); data.append(v)
    }
    mutating func string(_ field: Int, _ v: String) { if !v.isEmpty { bytes(field, Data(v.utf8)) } }
    /// A nested message, written even when empty.
    mutating func message(_ field: Int, _ v: Data) { tag(field, 2); varint(UInt64(v.count)); data.append(v) }
}

struct ProtoError: Error, CustomStringConvertible {
    let description: String
    init(_ d: String) { description = d }
}

struct ProtoReader {
    private let buf: [UInt8]
    private var pos = 0
    private(set) var field = 0
    private(set) var wire = 0

    init(_ d: Data) { buf = [UInt8](d) }

    mutating func next() throws -> Bool {
        if pos >= buf.count { return false }
        let t = try varint()
        field = Int(t >> 3); wire = Int(t & 7)
        if field == 0 { throw ProtoError("field number 0") }
        return true
    }

    mutating func varint() throws -> UInt64 {
        var shift: UInt64 = 0, result: UInt64 = 0
        while true {
            if pos >= buf.count || shift > 63 { throw ProtoError("truncated varint") }
            let b = buf[pos]; pos += 1
            result |= UInt64(b & 0x7F) << shift
            if b & 0x80 == 0 { return result }
            shift += 7
        }
    }

    mutating func int() throws -> Int { Int(truncatingIfNeeded: Int64(bitPattern: try varint())) }
    mutating func int64() throws -> Int64 { Int64(bitPattern: try varint()) }
    mutating func bool() throws -> Bool { try varint() != 0 }

    mutating func bytes() throws -> Data {
        let n = try varint()
        if n > UInt64(buf.count - pos) { throw ProtoError("length \(n) runs past the message") }
        let r = Data(buf[pos..<pos + Int(n)])
        pos += Int(n)
        return r
    }

    mutating func string() throws -> String { String(decoding: try bytes(), as: UTF8.self) }

    /// Skip an unknown field, so a newer peer stays readable.
    mutating func skip() throws {
        switch wire {
        case 0: _ = try varint()
        case 1: try advance(8)
        case 2: let n = try varint(); if n > UInt64(buf.count - pos) { throw ProtoError("bad length") }; pos += Int(n)
        case 5: try advance(4)
        default: throw ProtoError("unsupported wire type \(wire)")
        }
    }

    private mutating func advance(_ n: Int) throws {
        if pos + n > buf.count { throw ProtoError("truncated") }
        pos += n
    }
}
#endif
