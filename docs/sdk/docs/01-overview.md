# Overview

## Goal

Add one dependency, or one line, to a debug build. Run the app. Every network call appears on the Linux desktop app with full detail. No proxy, no certificate install, no Wi-Fi proxy settings.

The SDK complements the proxy, it does not replace it. Both feed the same core and flow model. The proxy stays the answer for apps you do not own, browsers, curl, backends and release builds.

## Why an SDK as well as a proxy

- Certificate-pinned apps work, because the SDK sees plaintext above TLS.
- No per-device setup. Nothing to install or trust on the phone.
- Each flow carries context a proxy cannot know: current screen, build, a human-readable name, owner, and the exact route template.
- Exact route templates feed the later route catalog (Phase 3) without guessing from traffic.
- Timings come from the client stack itself: OkHttp `EventListener` on Android, `URLSessionTaskMetrics` on iOS.

## Non-goals

- Flutter, web and other SDKs (later phase)
- Route catalog and MCP tools (Phase 3). This plan must capture and store the fields they need: `route_template`, `name`, `tag`, `owner`, `screen`
- Remote devices over the internet (needs a relay)
- WebSocket frame capture (calls are recorded, frames are not)
- Release builds
- Persisting captured traffic on the device

## Target developer experience

Android, zero code:

```kotlin
// app/build.gradle.kts
plugins { id("dev.proxy.inspector") }   // placeholder id, name TBD
```

Android, one line (OkHttp or Retrofit):

```kotlin
OkHttpClient.Builder().addInterceptor(InspectorInterceptor()).build()
```

Android, one line (Ktor):

```kotlin
val client = HttpClient(CIO) { install(InspectorPlugin) }
```

Android, optional annotation on a Retrofit interface:

```kotlin
interface CartApi {
    @Inspect(name = "Apply coupon", owner = "payments")
    @POST("cart/{id}/coupon")
    suspend fun applyCoupon(@Path("id") id: String, @Body body: Coupon): Cart
}
```

iOS, one line:

```swift
import ProxyInspector

@main struct MyApp: App {
    init() { Inspector.start() }
    var body: some Scene { WindowGroup { ContentView() } }
}
```

iOS, Alamofire:

```swift
let session = Session(eventMonitors: [InspectorMonitor()])
AF.request(url).inspect("Apply coupon", owner: "payments", template: "/cart/{id}/coupon")
```

iOS, zero code: link the `ProxyInspectorAutoStart` product and it starts itself.

Names above are placeholders. The final product name, Maven group, Gradle plugin id and Swift package name are open (see the end of this file).

## What the user sees

1. Add the SDK. Run the app on an emulator, a USB phone, or a phone on the same Wi-Fi.
2. First time per device and app: the desktop shows an Allow prompt with a 6-digit code. The app logs the same code and shows a debug banner. Confirm they match, click Allow.
3. Flows stream into the inspector. SDK flows show a badge, the screen, the name and the route template.
4. If the device also goes through the proxy, the two views merge into one flow with App and Wire tabs.

## Milestones

| Milestone | Content | Runs on |
|---|---|---|
| M0 | Spikes that de-risk the hard parts | Linux, S3 needs macOS |
| M1 | Protocol, core ingest, merge, fake-SDK test tooling | Linux |
| M2 | Desktop app: Devices screen, Inspector changes | Linux |
| M3 | Android SDK, Gradle plugin, no-op, sample, tests | Linux, emulator optional |
| M4 | iOS SDK, sample, tests | macOS with Xcode |
| M5 | Benchmarks, security review, end-to-end acceptance, docs, publishing | Linux and macOS |

Dependencies: M1 before M2 and M3. M3 and M4 are independent of each other once M1 is done. M5 needs everything.

## Prior art to study (read, do not copy without checking licenses)

- Android: Chucker (OkHttp inspection), Sentry's Android Gradle plugin (bytecode instrumentation of OkHttp)
- iOS: Netfox (URLProtocol approach), Pulse (delegate proxying approach)
- General: how these handle body capture, streaming and redirects

## Success criteria

See the acceptance list at the end of `10-tasks.md`.

## Open decisions

- Product name, Maven group id, Gradle plugin id, Swift package and product names
- Minimum supported versions: Android minSdk, OkHttp, Retrofit, Ktor, Android Gradle Plugin, Kotlin, iOS deployment target, Swift, Alamofire. Decide after checking current releases and record the matrix in each SDK README
- Whether to offer the debug-only `X-Inspect-Id` header for exact merging with proxy traffic. Default off
- Relay design for remote devices (later)
