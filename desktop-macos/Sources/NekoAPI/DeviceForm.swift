import Foundation

/// The per-device settings drawer's working copy. Lists are one per line (commas
/// and spaces also split); sizes are in KB. Same rules as the Linux drawer.
public struct DeviceForm: Equatable {
    public var deviceID: String
    public var appID: String
    public var title: String
    public var capture: Bool
    public var allow: String
    public var deny: String
    public var bodyCapKB: String
    public var eagerKB: String
    public var redaction: Neko_Api_V1_Redaction
    public var extraKeys: String
    private let original: Neko_Api_V1_DeviceConfig

    public init(_ d: Neko_Api_V1_Device) {
        let c = d.config
        deviceID = d.deviceID
        appID = d.appID
        title = [d.deviceName.isEmpty ? d.deviceID : d.deviceName, d.appID].filter { !$0.isEmpty }.joined(separator: " · ")
        capture = c.captureEnabled
        allow = c.hostAllow.joined(separator: "\n")
        deny = c.hostDeny.joined(separator: "\n")
        bodyCapKB = String(max(1, c.bodyCapBytes / 1024))
        eagerKB = String(c.eagerBodyBytes / 1024)
        redaction = c.redaction
        extraKeys = c.extraRedactKeys.joined(separator: "\n")
        original = c
    }

    public static func split(_ s: String) -> [String] {
        s.split(whereSeparator: { $0.isNewline || $0 == "," || $0 == " " }).map(String.init)
    }

    public var config: Neko_Api_V1_DeviceConfig {
        let cap = min(max(UInt32(bodyCapKB.trimmingCharacters(in: .whitespaces)) ?? 1, 1), 8 * 1024) * 1024
        let eager = min((UInt32(eagerKB.trimmingCharacters(in: .whitespaces)) ?? 0) &* 1024, cap)
        return .with {
            $0.captureEnabled = capture
            $0.hostAllow = Self.split(allow)
            $0.hostDeny = Self.split(deny)
            $0.bodyCapBytes = cap
            $0.eagerBodyBytes = eager
            $0.redaction = redaction
            $0.extraRedactKeys = Self.split(extraKeys)
        }
    }

    public var dirty: Bool { config != original }

    /// Revealing values is the one setting that needs a warning.
    public var revealsSecrets: Bool { redaction == .redactNone }
}
