# Proxy Inspector for iOS

**Status: partly built, not verified on Apple platforms.** This was written on Linux, with no Mac or Xcode. The platform-neutral core
(wire codec, pairing math, redaction, queue, ring buffer, the connection engine) builds and its tests pass with `swift test` on Linux.
Everything that touches Network.framework, the Keychain, UIKit or URLSession is written but has never been compiled or run.
Treat the Apple-only files as a first draft that needs a first build on a Mac, and the end-to-end checks in `docs/sdk/docs/e2e.md` as not done.

| Piece | File | Verified |
|---|---|---|
| Protobuf codec, framing, all messages | `Proto.swift`, `Wire.swift` | Yes, Linux: byte for byte against the Rust golden frames |
| SHA-256, pairing code | `SHA256.swift` | Yes: known vectors, code `730832` equals Rust and Kotlin |
| Redaction | `Redactor.swift` | Yes, Linux tests |
| Queue, ring buffer, body copy | `Support.swift` | Yes |
| Connection engine: discovery order, pairing, token reuse, pinning, streaming, dedup | `Engine.swift`, `Recorder.swift` | Yes, Linux, against an in-memory fake desktop |
| `#if DEBUG` stubs, no SDK in Release | `Inspector.swift`, `Hook.swift` | Linux only: `Scripts/verify-release.sh` builds Release and finds no capture code (it fails on a Debug object). **Xcode's definition of `DEBUG` for package targets is unchecked.** |
| TLS transport with fingerprint pinning (`NWConnection`) | `ApplePlatform.swift` | No |
| Keychain storage, Bonjour discovery, debug banner, screen tracking | `ApplePlatform.swift` | No |
| `Inspector.session(...)` URLSession capture (approach C) | `InspectedSession.swift` | No |
| Alamofire monitor and `.inspect(...)` | `ProxyInspectorAlamofire` | Compiles on Linux; no tests |
| `ProxyInspectorAutoStart` (`+load`) | `ProxyInspectorAutoStart` | No. A static library's `+load` may need `-ObjC`. |

## What does not exist yet

- **Automatic URLSession capture** (plan spike S3). Today `Inspector.start()` connects and pairs but only sees calls made through `Inspector.session(...)`
  or an Alamofire `Session` with `InspectorMonitor`. `URLSession.shared` and sessions created elsewhere are not captured. Choosing between a `URLProtocol`
  and delegate approach needs the test matrix run on a Mac.
- Compression of body chunks (iOS sends them uncompressed; the desktop accepts that).
- iOS over USB (not supported by design in v1).

## Intended use

```swift
import ProxyInspector

@main struct MyApp: App {
    init() { Inspector.start() }
    var body: some Scene { WindowGroup { ContentView() } }
}

let session = Inspector.session()                       // URLSession already wired for capture
let af = Session(eventMonitors: [InspectorMonitor()])   // Alamofire, product ProxyInspectorAlamofire
AF.request(url).inspect("Apply coupon", owner: "payments", template: "/cart/{id}/coupon")
```

Linking the `ProxyInspectorAutoStart` product starts the SDK with no code.

## Info.plist (debug only)

On a physical device, Wi-Fi discovery needs `NSLocalNetworkUsageDescription` and `NSBonjourServices` containing `_proxyinspect._tcp`. The Simulator needs neither.
Put them in a debug-only plist or xcconfig so Release does not carry them. `python3 Scripts/inspector-doctor.py path/to/Info.plist` checks and prints the fix.

## Test it here

```
swift test                      # Linux or macOS
Scripts/verify-release.sh       # builds Release, fails if capture code is in it
```

## First things to do on a Mac

1. `swift build` for iOS and macOS destinations and fix compile errors in the Apple-only files.
2. Run the spike S3 matrix (shared session, custom session, async/await, completion handler, upload, download, redirect, auth challenge, cancellation, HTTP/2) and pick the automatic capture approach.
3. Check `DEBUG` is defined for this package in Xcode's Debug and not in Release (else ship the two-product fallback in `docs/sdk/docs/07-ios-sdk.md`), then run `Scripts/verify-release.sh` against the built app binary.
4. Pair a Simulator app and a physical iPhone (with the Local Network prompt) against the desktop.
