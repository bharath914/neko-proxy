# Deviations from the plan

Where the repo differed from `02-core-assumptions.md`, or a decision changed. Newest last.

| # | Plan said | Repo / decision | Impact |
|---|---|---|---|
| 1 | Reserved nullable flow fields `source`, `device_id`, `app_id`, `tag`, `route_template` exist | They do not. Only `tags` exists. | C2 adds all SDK columns in a schema v2 migration (`PRAGMA user_version` 1 to 2). |
| 2 | Ingest port 8898 | The MCP server already listens on `127.0.0.1:8898` (Phase 2, registered by users with `claude mcp add`). | SDK ingest default port is **8897**. All docs, SDK candidates (`10.0.2.2:8897`, `localhost:8897`, `adb reverse tcp:8897`) use it. |
| 3 | Desktop app is Gradle / Compose Multiplatform under `app/` | Desktop is native Rust (egui) in `desktop/`. | D1 to D4 are egui screens in `desktop/src/ui/`. The "shared module" view-model idea maps to `desktop/src/model.rs` + `backend.rs`. |
| 4 | App talks to the core on `$XDG_RUNTIME_DIR/neko/api.sock` | Same socket, but the hub/API types live in `core/crates/api` and `proto/api.proto` (package `neko.api.v1`). | Device calls are added to that proto and crate. |
| 5 | Proxy port 8899 | Same. | none |
| 6 | Pipeline has interceptor hooks SDK flows skip | Same (`flow::InterceptorChain`). SDK flows are published straight to the hub and store. | none |
| 7 | Device registry tables in the flow SQLite schema | The registry (`sdk_app`, token hashes) is its own file, `sdk-devices.db`. Clearing captured traffic must not unpair devices, and token hashes should not sit next to data that is exported. | `devices` module in the `sdk` crate. |
| 8 | `Config.Redaction` enum order `STRICT = 0, DEFAULT = 1, NONE = 2` | `DEFAULT = 0, STRICT = 1, NONE = 2`. proto3 gives a missing field the value 0, and the safe default must be what a missing field means. | `proto/sdk.proto`. |
| 9 | "deflate" body chunks | Raw deflate (RFC 1951), each chunk independent, see `docs/sdk/protocol.md`. Raw deflate is what iOS's Compression framework produces. | |
| 10 | The merged flow is "derived" | Both records stay stored and point at each other (`merged_with`). The listing hides the proxy half. `FlowEvent::Removed` / `FlowBatch.removed` tell live subscribers. | `flow`, `store`, `api`. |
| 11 | CLI: `devices list|revoke` | Also `devices allow|deny|forget`, so pairing works headless. | |
| 12 | Ingest toggle | Off at every start (`--sdk` turns it on), never persisted, so a restart cannot leave a LAN listener open by surprise. | |
| 13 | New SDK public API | `Inspector.forgetPairing()` on Android and iOS: after the desktop certificate is regenerated, a paired SDK refuses the new fingerprint and needs a way to start over. | |
| 14 | Fuzzing with `cargo fuzz` | No nightly toolchain or ASAN runtime here (Fedora Rust). Targets run with `RUSTC_BOOTSTRAP=1 cargo fuzz run --sanitizer none`; no memory-error detection beyond Rust's own checks. | `core/crates/sdk/fuzz`. |
| 15 | Kotlin client may use protobuf-lite or hand-rolled | Hand-rolled (`ProtoIO.kt`, `Wire.kt`, about 300 lines). No second protobuf runtime in apps. Checked byte for byte against the Rust encoder through `proto/sdk-fixtures.json`. | `sdk/android/core` |
| 16 | `Inspector.trackNavigation(NavController)` in the public API | Left out. It would need a Navigation dependency; `Inspector.setScreen(name)` covers Compose, and Activity and Fragment tracking is automatic. Add it as a separate artifact later if wanted. | Android README |
| 17 | Manual `host:port` is the last candidate | It is tried first when set. A person who typed an address wants it used. | `Engine.candidates` |
| 18 | Redirects and retries "represented" | Each redirect hop is its own flow tagged `redirect` (as the proxy shows them, so only the first hop merges). Connection retries are not shown. | `InspectorInterceptor` |
| 19 | Core is plain JVM with a thin Android layer | As planned, plus a `Platform` interface. Android code is `sdk/android/android` (initializer, NSD, toast, screens). JVM tests run the whole engine against `sdk-test-core`. | |
| 20 | Gradle plugin dependency wiring | Real SDK for enabled variants is one artifact, `inspector-android`, which exposes core, OkHttp and Ktor support. The annotations artifact is added to every variant. | plugin |
| 21 | OkHttp instrumentation patches `Builder.build()` | Patches the `OkHttpClient(Builder)` constructor instead, which also catches `OkHttpClient()` and `newBuilder().build()`. See the S2 spike. | plugin |
| 22 | Latest stable toolchain | AGP 9.4.1 needs Gradle 9.6+ (used 9.8.1) and its built-in Kotlin is 2.2, so the SDK is compiled at Kotlin language and API level 2.0 with `kotlin-stdlib:2.0.21`. OkHttp 5.5 needs compileSdk 37. See `sdk/android/README.md`. | |
| 23 | Android emulator tests | The installed Pixel_7 image (API 37.1, 16 KB pages) crashes the emulator process on this host (SIGSEGV at startup, with and without GPU acceleration). An API 35 image was installed instead. See `e2e.md`. | |
