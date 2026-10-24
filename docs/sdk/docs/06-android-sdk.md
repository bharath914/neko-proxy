# Android SDK

Covers OkHttp, Retrofit and Ktor. Kotlin. Debug builds only.

Versions are not decided. Check current releases of OkHttp, Retrofit, Ktor, Android Gradle Plugin, Kotlin and Android API levels, choose a support matrix, and record it in `sdk/android/README.md`. Everything that says "verify" below means check current official docs, do not rely on memory.

## Modules

```
sdk/android/
├─ core/            transport, session, queue, ring buffer, config, redaction, discovery, pairing, screen tracking
├─ okhttp/          interceptor, EventListener, Retrofit Invocation reader
├─ ktor/            client plugin
├─ annotations/     @Inspect (tiny, same artifact in every build type)
├─ noop/            identical public API, does nothing
├─ gradle-plugin/   variant wiring, bytecode instrumentation, manifest overlay, doctor task
└─ sample/          app using Retrofit and Ktor, plus tests
```

- Keep `core` plain Kotlin/JVM where possible. Android-specific code (NSD, storage paths, lifecycle callbacks, App Startup) goes in a thin layer so most logic is unit-testable on the JVM.
- The `annotations` artifact has no logic and ships in all build types. A missing RUNTIME annotation class at runtime is harmless, but keep it available so code compiles in release.
- Public API names are shared between `core`/`okhttp`/`ktor` and `noop`: `Inspector`, `InspectorInterceptor`, `InspectorPlugin`, and so on. A parity test enforces it (see `09-testing-benchmarks.md`).

## Public API

```kotlin
object Inspector {
    fun configure(block: InspectorConfig.() -> Unit)   // optional
    fun setScreen(name: String)                        // manual screen name
    fun trackNavigation(navController: NavController) // optional helper in a separate artifact if it needs Navigation
    fun desktop(name: String)                          // choose a desktop by name when several are found
}
class InspectorInterceptor : Interceptor
val InspectorPlugin: ClientPlugin<...>                  // Ktor
annotation class Inspect(val name: String = "", val owner: String = "", val tag: String = "")
```

Everything else is internal.

## Auto start

- Use AndroidX App Startup (an `Initializer` merged through the library manifest), or a `ContentProvider`, so the SDK starts before `Application.onCreate` with no code from the developer. Verify the current recommended mechanism.
- Startup work must be tiny: register lifecycle callbacks, create the queue object, read stored pairing. Discovery and connection start lazily on a background thread after first use or after a short delay.

## Transport (core module)

- `SSLSocket` with a custom `X509TrustManager`:
  - Paired: accept only the stored SHA-256 fingerprint.
  - Pairing: accept any certificate for the handshake, and compute the code from the fingerprint actually seen.
- Length-prefixed protobuf frames as in `03-protocol.md`. Use the protobuf-lite Kotlin runtime or hand-rolled encoding. Keep the dependency small and avoid pulling a second protobuf runtime into apps. Evaluate size impact and record it.
- This client must never be captured by the SDK itself. It uses sockets directly, not OkHttp, so there is nothing to instrument. If any helper uses OkHttp, mark that client so instrumentation skips it.
- One dedicated background thread for sending, one bounded queue, one ring buffer for lazy bodies (default 5 MB total).
- Reconnect backoff: 2 s, 5 s, 15 s, 60 s cap. Scan mDNS only while disconnected and only in short windows.
- Store the token and fingerprint in a file under `noBackupFilesDir` (or EncryptedFile if it adds no meaningful dependency). Store a random `device_install_id` the same way.

## OkHttp and Retrofit (okhttp module)

Use an application interceptor for the main capture, and an `EventListener` for timings.

Why application interceptor: it sees one logical call with the decompressed body and the app's view of the request. Redirects and retries are visible through the EventListener and, if needed, a network interceptor can be added later for wire-level headers. The proxy covers the wire view.

Interceptor behaviour:
1. If capture is off or no desktop is connected, call `chain.proceed(request)` and return. Cost: one volatile read.
2. Build `CallStart`: method, URL, headers (redacted per config), route template, name, owner, tag, screen.
3. Request body: if it is a `RequestBody` with known small size and replayable, tee into a capped buffer while it is written. Wrap the `RequestBody`, do not read it twice. One-shot and duplex bodies are recorded as metadata only. Files and streams above the cap are truncated.
4. Proceed. On exception, emit `CallFailed` and rethrow.
5. Response body: wrap the response `ResponseBody` source so bytes are copied into a capped buffer as the app reads them. Never call `peekBody` on large bodies. Skip capture of `image/*`, `video/*`, `audio/*` and large `application/octet-stream`, recording size only. Streaming and event-stream responses emit chunks as they arrive.
6. When the app finishes or closes the body, emit `CallComplete` with timings. If the app never reads the body, emit `CallComplete` when the call ends and mark the body unavailable.

`EventListener` for timings: record dnsStart/End, connectStart/End, secureConnectStart/End, requestHeadersStart, responseHeadersStart, responseBodyEnd and callEnd/callFailed, and build `Timings`. If the user already has an `EventListener.Factory`, compose with it: call theirs first, then ours, and never change their behaviour.

Retrofit route template and metadata:
- Retrofit attaches an `Invocation` to the request. Read it with `request.tag(Invocation::class.java)` to get the `Method`.
- From the method read the HTTP annotation: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS` (value is the relative path) and `HTTP(method, path)`. Combine with the base URL path to produce the full template, for example `/v1/cart/{id}/coupon`.
- Read `@Inspect` from the method, then the declaring class.
- Without Retrofit, the route template is empty and the desktop infers one later.
- Verify behaviour with the Retrofit and OkHttp versions in the support matrix, including Kotlin `suspend` functions and Retrofit's call adapters.

Double instrumentation: if the Gradle plugin adds the interceptor and the developer also adds `InspectorInterceptor()`, add it once. Detect an existing instance in the interceptor list.

## Ktor (ktor module)

Ktor can run on several engines. Support the OkHttp and CIO engines first, and test at least those two.

- Implement as a client plugin (`createClientPlugin`). Capture the request in an `onRequest` hook and the response in the response pipeline.
- Response body: reading it consumes it. Use the mechanism the Ktor `ResponseObserver` plugin uses, which splits the body channel so the application still receives it. Verify the current Ktor API and behaviour (Ktor 2 and 3 differ in places) with a spike (S4).
- Request body: `ByteArrayContent` and similar can be read directly. `WriteChannelContent` and `ReadChannelContent` need wrapping so bytes are copied as they are written or read. Large and unknown-size bodies record metadata only.
- Metadata: let the developer set name, owner and tag per request through request attributes, for example `attributes.put(InspectName, "Apply coupon")`. Provide small helper extensions.
- Streaming and server-sent events: emit chunks.
- Ktor on the OkHttp engine goes through an `OkHttpClient`. If the Gradle plugin instruments that client, the same call is captured twice. Deduplicate inside the SDK before sending: same method, URL and start time within 50 ms from the two stacks on the same thread of execution is one call, and prefer the Ktor record (it carries the metadata). Cover this with a test.

## Screen tracking

- Register `ActivityLifecycleCallbacks`; the resumed activity class name is the default screen.
- Also support Fragment lifecycle where available.
- Compose screens are invisible to that. Provide `Inspector.setScreen(name)` and an optional Navigation helper. Best effort, never required.

## Gradle plugin (gradle-plugin module)

Plugin id is a placeholder (`dev.nekoproxy.inspector`).

What it does:
1. Adds dependencies by variant: the real SDK for enabled build types (default: debuggable ones, `debug`), the `noop` artifact for the rest. Extension: `inspector { enabledBuildTypes = setOf("debug", "staging") }`.
2. Instruments OkHttp in enabled variants using the Android Gradle Plugin's ASM instrumentation API (`AsmClassVisitorFactory`) with a scope that includes dependencies. Patch `OkHttpClient.Builder.build()` (or the `OkHttpClient` constructor taking a Builder) in the OkHttp jar so every client gets the interceptor and event listener composition added exactly once. Verify the current AGP API and its limits in spike S2. Sentry's Android Gradle plugin is a precedent for this approach.
3. Adds a debug manifest overlay for permissions the SDK needs (INTERNET and whatever NSD and network-state access require on current Android versions, verify). No cleartext exceptions are needed because the channel is TLS over a raw socket.
4. Optionally generates an asset with a manual `host:port` or `desktopName` from the extension.
5. Adds consumer ProGuard/R8 rules for the instrumented classes.
6. `inspectorDoctor` task: prints whether the plugin is applied, which variants are instrumented, which artifact each variant uses, whether INTERNET is declared, and how to fix problems.

The plugin must be idempotent, must not break builds when OkHttp is absent, and must work with the Gradle configuration cache. Test with Gradle TestKit.

## Debug banner and logs

On pairing, log the code at `Log.w("Inspector", "Pairing code 482 913 for <desktop>")` and show a small non-intrusive banner (Toast through the application context) once. After pairing, log one line per connection event, not per request.

## No-op module

- Same package and class names, same signatures, empty bodies.
- No sockets, no discovery, no startup initializer, no lifecycle callbacks.
- Verified by a test that diffs the public API of real and no-op, and by an APK/AAR inspection task that fails if release classpaths contain real SDK classes.

## Performance notes specific to Android

- Interceptor fast path when disconnected: a single volatile boolean read.
- Header redaction and URL parsing happen on the sender thread where possible, not on the caller thread. The caller thread only snapshots what it must (method, URL, header lists, a reference to the capped buffer).
- Use pooled byte arrays for body tees to avoid GC churn.
- Do not log per request.

## Documentation to write

`sdk/android/README.md` with a three-line quick start, the version matrix, what is and is not captured, the Wi-Fi/USB/emulator table, troubleshooting (`inspectorDoctor`), and how to verify a release build contains only the no-op.

## What is not captured

- Traffic from HTTP clients that are not OkHttp or Ktor (for example `HttpURLConnection`, `Cronet`, WebView, native image loaders with their own stack, unless they use an instrumented OkHttp). The proxy covers these
- WebSocket frames
- Calls made before the SDK starts
