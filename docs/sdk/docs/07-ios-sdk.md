# iOS SDK

Covers URLSession and Alamofire. Swift. Debug builds only. Needs macOS and Xcode to build and test.

Versions are not decided. Check current Swift, Xcode, iOS and Alamofire releases, choose a support matrix, and record it in `sdk/ios/README.md`. "Verify" below means check current Apple and library documentation, do not rely on memory.

## Package layout

Swift Package at `sdk/ios/`:

```
Package.swift
Sources/
├─ NekoInspector/                 core: transport, session, queue, redaction, discovery, pairing, URLSession capture
├─ NekoInspectorAlamofire/        EventMonitor and request modifiers (depends on Alamofire)
└─ NekoInspectorAutoStart/        tiny ObjC target that starts the SDK when linked
Tests/
Sample/                            SwiftUI app using URLSession and Alamofire
```

- Alamofire support is a separate product so apps without Alamofire do not pull it in.
- Same public API in release builds, compiled as stubs (see "Debug only").

## Public API

```swift
public enum Inspector {
    public static func start()
    public static func start(host: String, port: Int)       // manual desktop
    public static func start(desktopName: String)           // choose a desktop by name
    public static func setScreen(_ name: String)
}

// NekoInspectorAlamofire
public final class InspectorMonitor: EventMonitor { public init() }
extension Request {
    @discardableResult
    public func inspect(_ name: String, owner: String = "", tag: String = "", template: String = "") -> Self
}
```

## Transport (NekoInspector)

- Network.framework `NWConnection` with TLS options. Use the verify block to implement the same rules as Android: when paired, accept only the stored SHA-256 fingerprint; when pairing, accept for the handshake and compute the code from the fingerprint actually seen.
- Length-prefixed protobuf frames per `03-protocol.md`. Use a small protobuf implementation. Evaluate SwiftProtobuf's size and decide; record the choice. Avoid forcing a heavy dependency on app developers.
- ATS does not apply to raw Network.framework connections, so no `NSAppTransportSecurity` exceptions are needed. Verify.
- Single serial `DispatchQueue` for sending. Bounded queue and a ring buffer for lazy bodies (default 5 MB).
- Store the token and fingerprint in the Keychain (`kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`). Store a random install id the same way.
- Reconnect backoff 2 s, 5 s, 15 s, 60 s cap. Browse for Bonjour services only while disconnected.

## Discovery and permissions

- Bonjour with `NWBrowser` for `_nekoinspect._tcp`, reading the TXT records.
- Simulator: try `localhost:8898` first.
- Physical device on Wi-Fi: Bonjour.
- The debug app's Info.plist needs:
  - `NSLocalNetworkUsageDescription` with a short reason
  - `NSBonjourServices` containing `_nekoinspect._tcp`
- iOS shows a Local Network permission prompt the first time. If denied, the SDK logs a clear message with the fix and stops browsing.
- An SPM package cannot edit the app's Info.plist. Ship a `inspector-doctor` command (a Swift script or a build tool plugin command, verify what SwiftPM supports) that checks the plist and prints the exact keys to add. Document using a debug-only plist or xcconfig so release builds do not carry the keys.
- iOS over USB is not supported in v1.

## Capturing URLSession traffic

This is the risky part. Run spike S3 before choosing. Candidates:

A. `URLProtocol` injection (Netfox-style). Register the protocol for the shared session and inject it into `URLSessionConfiguration.default` and `.ephemeral` by swizzling the `protocolClasses` getter. The protocol re-issues the request internally. Known problems to evaluate: changes to delegate callback behaviour, authentication challenges, redirect handling, cache behaviour, double timing, and sessions created with a custom `protocolClasses`.

B. Delegate proxying (Pulse-style). Wrap or swizzle the creation of `URLSession` and tasks so the SDK observes delegate callbacks (`didReceive data`, `didCompleteWithError`, `didFinishCollecting metrics`) without re-issuing requests. Evaluate coverage of the async/await APIs (`URLSession.data(for:)`), Combine publishers, tasks created with completion handlers, and sessions with `nil` delegates.

C. Guaranteed fallback: an explicit helper `Inspector.session(configuration:delegate:)` that returns a `URLSession` already wired. One line changed where the app creates its session.

Spike S3 compares A and B on a small test matrix (shared session, custom session, async/await, completion handler, upload, download, redirect, auth challenge, cancellation, HTTP/2) and writes `docs/spikes/ios-urlsession.md` with the decision. Prefer the approach that does not change app behaviour. Always ship C.

Timings from `URLSessionTaskMetrics.transactionMetrics`: domain lookup, connect, secure connection, request start/end, response start/end. Map to `Timings`. Redirect chains appear as several transactions.

Not captured in v1: background sessions, download and upload tasks from files (record metadata only), `URLSessionWebSocketTask` frames, streams.

## Alamofire (NekoInspectorAlamofire)

- `InspectorMonitor` is an `EventMonitor`. Use request lifecycle callbacks and `request(_:didGatherMetrics:)` for timings, and the response callbacks for status, headers and data.
- Request body: read `request.request?.httpBody` when present. Stream and file uploads are metadata only.
- Response body: from the parsed response data, capped. `DataStreamRequest` is metadata only in v1.
- Per-request metadata: `.inspect(name, owner:tag:template:)` stores metadata keyed by the request id in the SDK. It returns `self` so it chains. The route template is only known if the developer provides `template:`; otherwise the desktop infers it. Verify what Alamofire exposes about the originating `URLRequestConvertible`; if the convertible is accessible in the current version, support a protocol `InspectableRoute` so a `Router` enum can supply `template` and `name` once.
- If both Alamofire monitoring and URLSession-level capture are active, the same call is seen twice. Deduplicate inside the SDK: Alamofire's record wins because it carries metadata. Match on task identifier where possible, otherwise method, URL and time window. Test it.

## Screen tracking

Swizzle `UIViewController.viewDidAppear(_:)` in debug builds to track the top view controller's name (ignore container controllers). SwiftUI screens are invisible to it, so `Inspector.setScreen(_:)` exists. Best effort.

## Debug only

- All real code is compiled only under `#if DEBUG`. In release builds the same public symbols exist as empty stubs, so app code compiles unchanged.
- Verify that SwiftPM defines `DEBUG` for the package target when the app builds in Debug and not in Release, in the way Xcode builds packages. If not reliable, provide two products (`NekoInspector` and `NekoInspectorNoop`) and document linking per configuration.
- Verification task: build the sample in Release, inspect the binary for the SDK's symbols and strings (for example the service name `_nekoinspect._tcp`), and fail if found.
- The AutoStart target must also be a no-op in release.

## Auto start (NekoInspectorAutoStart)

A small Objective-C target whose `+load` calls `Inspector.start()` in debug builds. Linking the product is enough. Verify this still works with current SwiftPM and Xcode static linking (a library's `+load` can be stripped if nothing references it; the product may need a linker flag or a referenced symbol).

## Debug banner and logs

On pairing, log the code with `os_log` (subsystem `dev.nekoproxy.inspector`) and show a small non-blocking banner in a dedicated debug `UIWindow`. After pairing, log one line per connection event, not per request.

## Performance notes specific to iOS

- Fast path when disconnected: an atomic flag check.
- Copy bodies into pooled buffers. Do not hold `Data` references that keep large response buffers alive longer than the app does.
- Do JSON redaction on the sender queue, not on the URLSession delegate queue.
- Do not touch the main thread except for the debug banner.

## Documentation to write

`sdk/ios/README.md` with a three-line quick start, the Info.plist keys, the doctor command, what is and is not captured, the Simulator/Wi-Fi table, troubleshooting (Local Network permission), and how to verify a release build contains no SDK code.

## What is not captured

- Traffic from `URLSession` configurations that bypass the chosen capture approach (decided in S3)
- Background sessions, WebSocket frames, streams
- Network calls made by system frameworks or by SDKs that use their own sockets. The proxy covers these
