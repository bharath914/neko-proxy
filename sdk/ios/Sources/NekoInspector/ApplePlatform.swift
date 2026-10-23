// UNVERIFIED: written without a Mac. Everything in this file is compiled only on Apple platforms, so it has
// never been built or run. See sdk/ios/README.md and docs/sdk/docs/e2e.md. Expect compile errors to fix on first build.
#if DEBUG && canImport(Network)
import Foundation
import Network
import Security
import os
#if canImport(UIKit)
import UIKit
#endif

// MARK: Keychain storage

/// The pairing and the install id live in the Keychain, this device only, readable after first unlock.
final class KeychainStorage: PairingStorage {
    private let service = "dev.nekoproxy.inspector"

    private func read(_ account: String) -> Data? {
        let q: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrAccount as String: account,
                                kSecReturnData as String: true, kSecMatchLimit as String: kSecMatchLimitOne]
        var out: CFTypeRef?
        return SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess ? out as? Data : nil
    }

    private func write(_ account: String, _ data: Data) {
        let base: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrAccount as String: account]
        SecItemDelete(base as CFDictionary)
        var add = base
        add[kSecValueData as String] = data
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        SecItemAdd(add as CFDictionary, nil)
    }

    func installId() -> String {
        if let d = read("install-id"), let s = String(data: d, encoding: .utf8), s.count >= 8 { return s }
        let id = UUID().uuidString.lowercased()
        write("install-id", Data(id.utf8))
        return id
    }

    func load() -> Pairing_? {
        guard let d = read("pairing"), let o = try? JSONSerialization.jsonObject(with: d) as? [String: Any],
              let t = (o["token"] as? String).flatMap({ Data(hex: $0) }), let f = (o["fp"] as? String).flatMap({ Data(hex: $0) }),
              t.count == 32, f.count == 32 else { return nil }
        return Pairing_(token: t, fingerprint: f, lastHost: o["host"] as? String ?? "", lastPort: o["port"] as? Int ?? 0, desktopName: o["name"] as? String ?? "")
    }

    func save(_ p: Pairing_) {
        let o: [String: Any] = ["token": p.token.hex, "fp": p.fingerprint.hex, "host": p.lastHost, "port": p.lastPort, "name": p.desktopName]
        if let d = try? JSONSerialization.data(withJSONObject: o) { write("pairing", d) }
    }

    func clear() {
        let q: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrAccount as String: "pairing"]
        SecItemDelete(q as CFDictionary)
    }
}

// MARK: Network.framework transport

/// A blocking, pinned TLS connection over `NWConnection`. No ATS exceptions are needed: ATS does not apply to raw
/// Network.framework connections (to verify on a device).
final class NetworkConnection: Connection {
    private let conn: NWConnection
    private let q = DispatchQueue(label: "dev.nekoproxy.inspector.net")
    let fingerprint: Data

    init(conn: NWConnection, fingerprint: Data) { self.conn = conn; self.fingerprint = fingerprint }

    func send(_ data: Data) throws {
        let sem = DispatchSemaphore(value: 0)
        var err: Error?
        conn.send(content: data, completion: .contentProcessed { e in err = e; sem.signal() })
        if sem.wait(timeout: .now() + 15) == .timedOut { throw TransportError("send timed out") }
        if let e = err { throw e }
    }

    func receive(exactly count: Int, timeout: TimeInterval?) throws -> Data {
        var out = Data()
        while out.count < count {
            let sem = DispatchSemaphore(value: 0)
            var chunk: Data?, err: Error?, done = false
            conn.receive(minimumIncompleteLength: 1, maximumLength: count - out.count) { d, _, isComplete, e in
                chunk = d; err = e; done = isComplete; sem.signal()
            }
            if let t = timeout {
                if sem.wait(timeout: .now() + t) == .timedOut { throw TransportError("receive timed out") }
            } else { sem.wait() }
            if let e = err { throw e }
            if let c = chunk, !c.isEmpty { out.append(c) } else if done { throw TransportError("closed") }
        }
        return out
    }

    func close() { conn.cancel() }
}

struct NetworkTransport: Transport {
    func connect(host: String, port: Int, pinned: Data?, timeout: TimeInterval) throws -> Connection {
        let seen = Locked<Data?>(nil), mismatch = Locked<Data?>(nil)
        let tls = NWProtocolTLS.Options()
        let verifyQueue = DispatchQueue(label: "dev.nekoproxy.inspector.verify")
        sec_protocol_options_set_verify_block(tls.securityProtocolOptions, { _, trust, complete in
            let secTrust = sec_trust_copy_ref(trust).takeRetainedValue()
            guard let chain = SecTrustCopyCertificateChain(secTrust) as? [SecCertificate], let leaf = chain.first else { complete(false); return }
            let fp = Data(SHA256.hash(SecCertificateCopyData(leaf) as Data))
            seen.value = fp
            if let p = pinned, p != fp { mismatch.value = fp; complete(false); return }
            complete(true)
        }, verifyQueue)
        let tcp = NWProtocolTCP.Options()
        tcp.noDelay = true
        tcp.connectionTimeout = Int(timeout.rounded(.up))
        let params = NWParameters(tls: tls, tcp: tcp)
        guard let p = NWEndpoint.Port(rawValue: UInt16(port)) else { throw TransportError("bad port") }
        let conn = NWConnection(host: NWEndpoint.Host(host), port: p, using: params)
        let ready = DispatchSemaphore(value: 0)
        var failure: Error?
        conn.stateUpdateHandler = { s in
            switch s {
            case .ready: ready.signal()
            case .failed(let e), .waiting(let e): failure = e; ready.signal()
            case .cancelled: ready.signal()
            default: break
            }
        }
        conn.start(queue: DispatchQueue(label: "dev.nekoproxy.inspector.net"))
        if ready.wait(timeout: .now() + timeout + 1) == .timedOut { conn.cancel(); throw TransportError("connect timed out") }
        if let m = mismatch.value, let expected = pinned { conn.cancel(); throw FingerprintMismatch(expected: expected, saw: m) }
        if let e = failure { conn.cancel(); throw e }
        guard let fp = seen.value else { conn.cancel(); throw TransportError("no certificate seen") }
        return NetworkConnection(conn: conn, fingerprint: fp)
    }
}

// MARK: Platform

final class ApplePlatform: Platform {
    let storage: PairingStorage = KeychainStorage()
    let transport: Transport = NetworkTransport()
    private let logger = Logger(subsystem: "dev.nekoproxy.inspector", category: "Inspector")
    private func info(_ k: String) -> String { Bundle.main.object(forInfoDictionaryKey: k) as? String ?? "" }

    var appId: String { Bundle.main.bundleIdentifier ?? "unknown" }
    var appName: String { info("CFBundleDisplayName").isEmpty ? info("CFBundleName") : info("CFBundleDisplayName") }
    var appVersion: String { info("CFBundleShortVersionString") }
    var build: String { info("CFBundleVersion") }
    var deviceName: String {
        #if canImport(UIKit)
        return UIDevice.current.name
        #else
        return Host.current().localizedName ?? "Mac"
        #endif
    }
    var deviceModel: String {
        var u = utsname(); uname(&u)
        return withUnsafeBytes(of: &u.machine) { String(decoding: $0.prefix { $0 != 0 }, as: UTF8.self) }
    }
    var osVersion: String { ProcessInfo.processInfo.operatingSystemVersionString }

    func localCandidates() -> [Candidate] {
        #if targetEnvironment(simulator)
        return [Candidate(host: "localhost", port: 0, kind: "simulator")]
        #else
        return []
        #endif
    }

    /// Bonjour, `_nekoinspect._tcp`, only while disconnected. The Local Network permission prompt appears on first use;
    /// if denied, browsing finds nothing and a clear message is logged. The debug app's Info.plist needs
    /// NSLocalNetworkUsageDescription and NSBonjourServices (see `inspector-doctor`).
    func discover(timeout: TimeInterval) -> [Candidate] {
        let browser = NWBrowser(for: .bonjourWithTXTRecord(type: "_nekoinspect._tcp", domain: nil), using: .tcp)
        let found = Locked<[(NWEndpoint, [String: String])]>([])
        let denied = Locked<Bool>(false)
        browser.browseResultsChangedHandler = { results, _ in
            found.value = results.compactMap { r in
                if case .bonjour(let txt) = r.metadata { return (r.endpoint, txt.dictionary) }
                return nil
            }
        }
        browser.stateUpdateHandler = { s in
            if case .failed = s { denied.value = true }
            if case .waiting(let e) = s, "\(e)".contains("PolicyDenied") { denied.value = true }
        }
        browser.start(queue: DispatchQueue(label: "dev.nekoproxy.inspector.bonjour"))
        Thread.sleep(forTimeInterval: timeout)
        browser.cancel()
        if denied.value && found.value.isEmpty {
            log(.warn, "Local Network access looks denied. Allow it in Settings > Privacy & Security > Local Network, and make sure Info.plist has NSBonjourServices with _nekoinspect._tcp.")
        }
        return found.value.compactMap { endpoint, txt in
            guard let (host, port) = resolve(endpoint) else { return nil }
            return Candidate(host: host, port: port, kind: "wifi", fingerprintShort: txt["fp"], desktopName: txt["name"])
        }
    }

    /// Opens a connection to the service just long enough to learn its address.
    private func resolve(_ endpoint: NWEndpoint) -> (String, Int)? {
        let c = NWConnection(to: endpoint, using: .tcp)
        let sem = DispatchSemaphore(value: 0)
        c.stateUpdateHandler = { s in if case .ready = s { sem.signal() } else if case .failed = s { sem.signal() } }
        c.start(queue: DispatchQueue(label: "dev.nekoproxy.inspector.resolve"))
        _ = sem.wait(timeout: .now() + 1.5)
        defer { c.cancel() }
        if case .hostPort(let h, let p)? = c.currentPath?.remoteEndpoint {
            var host = "\(h)"
            if let pct = host.firstIndex(of: "%") { host = String(host[..<pct]) }
            return (host, Int(p.rawValue))
        }
        return nil
    }

    func log(_ level: LogLevel, _ message: String) {
        switch level {
        case .debug: logger.debug("\(message, privacy: .public)")
        case .info: logger.info("\(message, privacy: .public)")
        case .warn: logger.warning("\(message, privacy: .public)")
        case .error: logger.error("\(message, privacy: .public)")
        }
    }

    func notify(_ message: String) {
        #if canImport(UIKit)
        DispatchQueue.main.async { DebugBanner.show(message) }
        #endif
    }
}

#if canImport(UIKit)
/// A small non-blocking banner in a window of its own, for the pairing code.
enum DebugBanner {
    private static var window: UIWindow?

    static func show(_ text: String) {
        guard let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).first else { return }
        let w = UIWindow(windowScene: scene)
        w.windowLevel = .alert + 1
        w.isUserInteractionEnabled = false
        let label = UILabel(frame: CGRect(x: 12, y: 60, width: scene.coordinateSpace.bounds.width - 24, height: 48))
        label.text = text; label.numberOfLines = 2; label.textColor = .white; label.font = .systemFont(ofSize: 14, weight: .semibold)
        label.backgroundColor = UIColor.black.withAlphaComponent(0.85); label.layer.cornerRadius = 10; label.layer.masksToBounds = true
        label.textAlignment = .center
        w.addSubview(label)
        w.isHidden = false
        window = w
        DispatchQueue.main.asyncAfter(deadline: .now() + 8) { window?.isHidden = true; window = nil }
    }
}

/// `viewDidAppear` swizzling: the top view controller's class name is the screen. Containers are skipped.
/// SwiftUI screens are invisible to this: use `Inspector.setScreen(_:)`.
enum ScreenTracker {
    static func install() {
        guard let m = class_getInstanceMethod(UIViewController.self, #selector(UIViewController.viewDidAppear(_:))),
              let s = class_getInstanceMethod(UIViewController.self, #selector(UIViewController.inspector_viewDidAppear(_:))) else { return }
        method_exchangeImplementations(m, s)
    }
}

extension UIViewController {
    @objc func inspector_viewDidAppear(_ animated: Bool) {
        inspector_viewDidAppear(animated) // the original, after the exchange
        if self is UINavigationController || self is UITabBarController || self is UISplitViewController || self is UIPageViewController { return }
        Inspector.setScreen(String(describing: type(of: self)))
    }
}
#endif
#endif
