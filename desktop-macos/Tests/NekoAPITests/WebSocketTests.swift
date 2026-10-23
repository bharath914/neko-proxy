import Foundation
import Testing
@testable import NekoAPI

@Suite struct WebSocketTests {
    @Test(arguments: [0, 5, 125, 126, 70_000])
    func maskedRoundTrip(size: Int) throws {
        let payload = Data((0..<size).map { UInt8($0 & 0xFF) })
        var dec = WSDecoder()
        let wire = WSCodec.encode(.binary, payload, mask: [1, 2, 3, 4])
        var frames: [WSFrame] = []
        if size < 200 {
            // One byte at a time exercises partial frames.
            for b in wire { frames += try dec.feed(Data([b])) }
        } else {
            frames = try dec.feed(wire)
        }
        #expect(frames == [WSFrame(opcode: .binary, payload: payload)])
    }

    @Test func fragmentedMessageIsJoined() throws {
        var dec = WSDecoder()
        // Unmasked server-style frames: first fragment (fin=0), ping, final fragment.
        let wire = Data([0x02, 0x02, 0xAA, 0xBB, 0x89, 0x00, 0x80, 0x01, 0xCC])
        let frames = try dec.feed(wire)
        #expect(frames == [
            WSFrame(opcode: .ping, payload: Data()),
            WSFrame(opcode: .binary, payload: Data([0xAA, 0xBB, 0xCC])),
        ])
    }

    @Test func continuationWithoutStartFails() {
        var dec = WSDecoder()
        #expect(throws: (any Error).self) { try dec.feed(Data([0x80, 0x00])) }
    }
}
