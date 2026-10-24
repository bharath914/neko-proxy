# Neko Inspector for Android

Streams your debug app's network calls to the Neko Proxy desktop app. No proxy, no certificate, no Wi-Fi settings.
Debug builds only: release builds get a no-op with the same API.

## Quick start

```kotlin
// app/build.gradle.kts
plugins {
    id("com.android.application")
    id("dev.nekoproxy.inspector")      // placeholder id, the final name is open
}
```

Run the app on an emulator, a USB phone or a phone on the same Wi-Fi. The first time, the desktop app shows an
Allow prompt with a six-digit code; the same code is in Logcat (tag `Inspector`) and in a toast. Check they match, press Allow.

That is all. The plugin adds the SDK to debug builds, the no-op to release builds, hooks every `OkHttpClient`
(including ones created inside libraries) and starts the SDK before `Application.onCreate`.

### Without the plugin

```kotlin
debugImplementation("dev.nekoproxy.inspector:inspector-android:0.1.0")
releaseImplementation("dev.nekoproxy.inspector:inspector-noop:0.1.0")
implementation("dev.nekoproxy.inspector:inspector-annotations:0.1.0")

OkHttpClient.Builder().addInterceptor(InspectorInterceptor()).build()   // OkHttp and Retrofit
HttpClient(CIO) { install(InspectorPlugin) }                            // Ktor
```

### Name your calls (optional)

```kotlin
interface CartApi {
    @Inspect(name = "Apply coupon", owner = "payments")
    @POST("cart/{id}/coupon")
    suspend fun applyCoupon(@Path("id") id: String, @Body body: Coupon): Cart
}

client.get(url) { inspect(name = "Apply coupon", owner = "payments", route = "/cart/{id}/coupon") }   // Ktor
Inspector.setScreen("Checkout")                                                                       // Compose and other screens
```

Retrofit calls get their route template (`/v1/cart/{id}/coupon`) automatically.

### Settings (all optional)

```kotlin
inspector {
    enabledBuildTypes = setOf("debug", "staging")   // default: debug
    host = "192.168.1.24"; port = 8897               // only if discovery cannot find the desktop
    desktopName = "Asha's Mac"                       // several desktops on one network
}
```

## Support matrix

| Piece | Tested | Notes |
|---|---|---|
| Android Gradle Plugin | 9.4.1, 8.13.1 | The plugin is compiled against the AGP 8.5 API (`androidComponents`, `AsmClassVisitorFactory`). AGP 9 needs Gradle 9.6 or newer. |
| Kotlin of the app | 2.2 reading the SDK, 2.4 | The SDK is built with language and API level 2.0, so any Kotlin 2.x compiler can read it. |
| minSdk | 23 | |
| compileSdk | the one your dependencies need | OkHttp 5.5 needs compileSdk 37. The SDK itself needs nothing newer than 23. |
| OkHttp | 5.5.0 (4.12 expected to work; the interceptor API is the same) | Application interceptor plus `EventListener`. |
| Retrofit | 3.0.0 | Route templates read from `Invocation`. |
| Ktor client | 3.6.0, engines CIO and OkHttp | Any engine should work: the plugin is engine independent. |
| Java | bytecode level 11 | |

## What is captured

OkHttp, Retrofit and Ktor calls: method, URL, headers, request and response bodies (up to 1 MiB each, larger ones are
kept on the device until you open the flow on the desktop), status, protocol, timings, redirects (one flow per hop),
failures, and server-sent events as they arrive.

**Not captured:** `HttpURLConnection`, Cronet, WebView, image loaders with their own stack, WebSocket frames, calls made
before the SDK starts. The proxy covers those. Images, video, audio and large binaries record size only.

## How it finds the desktop

| Setup | Needs same Wi-Fi | Mechanism |
|---|---|---|
| Android emulator | No | `10.0.2.2:8897` |
| Phone over USB | No | the desktop runs `adb reverse`, the app uses `localhost:8897` |
| Phone on the same Wi-Fi | Yes | mDNS `_nekoinspect._tcp` |
| Network that isolates devices | fails | use USB, or another network |
| Remote device, cellular | not supported | needs a relay |

## Privacy and safety

- Captured calls go only to the desktop you paired with, over TLS, pinned to its certificate. Nothing is ever sent before pairing.
- Authorization, cookies, API keys, tokens and passwords (in headers, query strings, JSON and form bodies) are replaced with `[redacted]`
  on the device. Revealing them is a per-device choice on the desktop.
- Nothing about captured traffic is written to the device. Only the pairing token and the desktop's certificate fingerprint are stored, in `noBackupFilesDir`.
- With no desktop connected, a request costs one volatile read.

## Troubleshooting

Run `./gradlew inspectorDoctor`. It prints, per variant, whether the real SDK or the no-op is used, and whether `INTERNET` is declared.

| Symptom | Fix |
|---|---|
| Nothing appears | Is "Accept SDK connections" on in the desktop app? Look at Logcat for tag `Inspector`. |
| "Several desktops found" | `Inspector.desktop("name")` or `inspector { desktopName = "..." }`. |
| "different certificate" | The desktop's certificate changed (or this is another desktop). If you replaced it on purpose, call `Inspector.forgetPairing()` and run again. |
| Phone on Wi-Fi not found | The network may isolate devices. Plug in USB (needs `adb` on the desktop). |
| Calls from a library missing | They are captured if the library uses OkHttp. Libraries with their own HTTP stack are not captured. |

## Check that release builds contain no capture code

```
./gradlew verifyNoInspectorInRelease
```

(Also covered by the plugin's own tests: the release APK's dex contains no `dev/nekoproxy/inspector/internal` classes and no `_nekoinspect` string.)

## Build and test this repository

```
cargo build -p sdk --bins              # the ingest server the tests talk to (repo root)
./gradlew test                         # JVM tests: core, okhttp, ktor, noop parity
./gradlew publishAllToLocalRepo        # build/local-repo, consumed by the sample and the plugin tests
./gradlew :gradle-plugin:test          # builds real Android apps with the plugin (AGP 8 and 9)
./gradlew -p sample :app:assembleDebug # the sample app
```

Emulator and device checks are local-only: they need an emulator or phone and a running desktop (or `sdk-test-core`).
See `docs/sdk/docs/e2e.md` for what was run and what was not.
