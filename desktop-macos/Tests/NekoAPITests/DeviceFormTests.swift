import Foundation
import Testing
@testable import NekoAPI

@Suite struct DeviceFormTests {
    private func device() -> Neko_Api_V1_Device {
        .with {
            $0.deviceID = "i1"; $0.appID = "com.x"; $0.deviceName = "Pixel 8"
            $0.config = .with { $0.captureEnabled = true; $0.bodyCapBytes = 1 << 20; $0.eagerBodyBytes = 64 << 10; $0.hostDeny = ["ads.test"] }
        }
    }

    @Test func untouchedFormMatchesTheDevice() {
        let f = DeviceForm(device())
        #expect(f.config == device().config && !f.dirty)
        #expect(f.title == "Pixel 8 · com.x")
    }

    @Test func listsSplitOnLinesCommasAndSpaces() {
        var f = DeviceForm(device())
        f.allow = "api.test, *.cdn.test\n  other.test "; f.extraKeys = "ssn\nnational_id"
        #expect(f.config.hostAllow == ["api.test", "*.cdn.test", "other.test"])
        #expect(f.config.extraRedactKeys == ["ssn", "national_id"] && f.dirty)
    }

    @Test func sizesAreClampedAndEagerNeverExceedsTheCap() {
        var f = DeviceForm(device())
        f.bodyCapKB = "100000"; f.eagerKB = "100000"
        #expect(f.config.bodyCapBytes == 8 << 20 && f.config.eagerBodyBytes == 8 << 20)
        f.bodyCapKB = "0"
        #expect(f.config.bodyCapBytes == 1024)
    }

    @Test func noneRedactionIsFlagged() {
        var f = DeviceForm(device())
        #expect(!f.revealsSecrets)
        f.redaction = .redactNone
        #expect(f.revealsSecrets)
    }
}
