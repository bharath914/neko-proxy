import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// Streams this app's network calls to the Proxy desktop app. Debug builds only: in Release the same
/// symbols exist as empty stubs, so app code compiles unchanged.
public enum Inspector {
    /// Connects to a desktop found by Bonjour (Simulator: `localhost`). Call once, early, for example in `App.init`.
    public static func start() { startImpl(host: nil, port: nil, desktopName: nil) }
    /// A desktop at a known address, when Bonjour cannot find it.
    public static func start(host: String, port: Int) { startImpl(host: host, port: port, desktopName: nil) }
    /// Choose one desktop by name when several are on the network.
    public static func start(desktopName: String) { startImpl(host: nil, port: nil, desktopName: desktopName) }
    /// Names the screen shown on the calls made from now on. Use it for SwiftUI screens the SDK cannot see.
    public static func setScreen(_ name: String) { setScreenImpl(name) }
    /// Forget the pairing. The next connection asks for approval again. Needed after the desktop's certificate was replaced.
    public static func forgetPairing() { forgetPairingImpl() }
    /// A `URLSession` already wired for capture, for sessions the automatic capture does not see. One line changed
    /// where the app creates its session.
    public static func session(configuration: URLSessionConfiguration = .default, delegate: URLSessionDelegate? = nil, delegateQueue: OperationQueue? = nil) -> URLSession {
        sessionImpl(configuration: configuration, delegate: delegate, delegateQueue: delegateQueue)
    }
}

#if DEBUG
extension Inspector {
    static func startImpl(host: String?, port: Int?, desktopName: String?) {
        Starter.start(host: host, port: port, desktopName: desktopName)
    }
    static func setScreenImpl(_ name: String) { Runtime.engine?.screen = name }
    static func forgetPairingImpl() { Runtime.engine?.forgetPairing() }
    static func sessionImpl(configuration: URLSessionConfiguration, delegate: URLSessionDelegate?, delegateQueue: OperationQueue?) -> URLSession {
        #if canImport(Network)
        return InspectedSession.make(configuration: configuration, delegate: delegate, delegateQueue: delegateQueue)
        #else
        return URLSession(configuration: configuration, delegate: delegate, delegateQueue: delegateQueue)
        #endif
    }
}

/// Wires an engine to a platform. Tests build engines directly; the app path is Apple only.
enum Starter {
    static let settings = Locked<EngineSettings>(EngineSettings())

    static func start(host: String?, port: Int?, desktopName: String?) {
        settings.value = EngineSettings(host: host, port: port ?? Engine.defaultPort, desktopName: desktopName)
        #if canImport(Network)
        let engine = Runtime.engineBox.value ?? Engine(platform: ApplePlatform(), settings: { Starter.settings.value })
        Runtime.engineBox.value = engine
        engine.start()
        #if canImport(UIKit)
        DispatchQueue.main.async { ScreenTracker.install() }
        #endif
        #endif
    }
}
#else
extension Inspector {
    static func startImpl(host: String?, port: Int?, desktopName: String?) {}
    static func setScreenImpl(_ name: String) {}
    static func forgetPairingImpl() {}
    static func sessionImpl(configuration: URLSessionConfiguration, delegate: URLSessionDelegate?, delegateQueue: OperationQueue?) -> URLSession {
        URLSession(configuration: configuration, delegate: delegate, delegateQueue: delegateQueue)
    }
}
#endif

/// Called by `ProxyInspectorAutoStart`'s Objective-C `+load`. A no-op in Release.
@_cdecl("proxy_inspector_autostart")
public func proxyInspectorAutostart() { Inspector.start() }
