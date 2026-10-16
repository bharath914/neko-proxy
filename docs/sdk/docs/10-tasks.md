# Task list

Work in order within each milestone. A task is done when its acceptance checks pass. Record spike outcomes in `docs/spikes/<name>.md`. Record plan differences in `docs/DEVIATIONS.md`.

Legend: **[mac]** needs macOS and Xcode. **[dev]** is best verified on a real device or emulator.

---

## M0: Spikes

Small throwaway experiments. Each ends with a short decision doc.

**S1. Pinned TLS and pairing code.** Rust server with a self-signed cert, a minimal Kotlin JVM client and a minimal Swift client **[mac]** that: connect, pin by fingerprint, run the commit-reveal exchange, and print the same 6-digit code on both sides.
- Accept: codes match on a normal run. They differ when a proxy presents a different certificate. Both clients work without third-party networking libraries.

**S2. Android bytecode instrumentation.** A Gradle plugin prototype using the AGP ASM instrumentation API that adds an interceptor to every `OkHttpClient.Builder` result in debug only, including clients created inside dependencies.
- Accept: a sample app with a Retrofit client and a library-created client both get the interceptor exactly once. Release build is untouched. Works with the configuration cache. Record the AGP and OkHttp versions tested.

**S3. iOS URLSession capture [mac].** Compare approaches A, B and C from `07-ios-sdk.md` on the test matrix.
- Accept: decision doc with a table of results, the chosen primary approach, and any behaviour changes it causes.

**S4. Ktor body capture.** Prototype response and request body capture without consuming bodies, on the OkHttp and CIO engines, including streaming.
- Accept: the app still receives the full body. Captured bytes equal the received bytes up to the cap. Works on the Ktor versions in the support matrix.

---

## M1: Protocol and core ingest (Rust)

**C1. Protocol definition.** Write `proto/sdk.proto` from the sketch in `03-protocol.md`, generate Rust and Kotlin code, and add a short protocol doc with two example sessions (fresh pairing and reconnect).
- Accept: Rust and Kotlin compile. Example sessions decode in a unit test.

**C2. Schema and flow fields.** Add the SDK fields and tables from `04-core-ingest.md`. Add a migration if the reserved fields from Phase 1 are missing.
- Accept: migrations run on an existing database. Flow queries still work. Full-text index includes SDK fields.

**C3. Listener, certificate and framing.** `cert`, `frame`, `listener`. Runtime enable and disable.
- Accept: a TLS client can connect when enabled and cannot when disabled. Oversized or malformed frames close the connection. Fingerprint is stable across restarts.

**C4. Sessions and pairing.** `session`, `pairing`, `devices`. Hello, pairing with commit-reveal, token issue and storage as hash, reconnect with token, deny, revoke, prompt limits and timeouts.
- Accept: integration tests with the Rust fake client cover every path. No call data is accepted before `AUTHENTICATED`.

**C5. Assembler and storage.** Turn events into flows and blobs. Ordering, duplicates, truncation, session drop, lazy body fetch, clock offset.
- Accept: flows appear live and finalize. Bodies decode through the `decode` crate. Dropping a session mid-call marks the flow incomplete after the grace period.

**C6. Limits and backpressure.** All limits in `04-core-ingest.md`.
- Accept: floods and slow consumers do not stall other sessions or the proxy. Drops are counted and visible in `sdk.status()`.

**C7. Discovery and adb.** mDNS advertisement, adb detection and `adb reverse`.
- Accept: service appears and disappears with the toggle. A connected Android device gets the reverse automatically. Missing adb is reported, not fatal.

**C8. Merge.** Implement merge as in `03-protocol.md`.
- Accept: unit tests listed in `04-core-ingest.md` pass. Unmerged records stay in storage.

**C9. API and CLI.** Add the app-to-core calls and CLI commands listed in `04-core-ingest.md`.
- Accept: calls work from a test client. CLI commands work.

**C10. `sdk-test-core` and fuzzing.** Build the dev binary and the fuzz targets.
- Accept: the binary supports every flag listed in `09-testing-benchmarks.md`. Fuzzers run for a fixed time without crashes. Add them to CI if CI exists.

---

## M2: Desktop app (Linux)

**D1. Devices screen.** As specified in `05-desktop-app.md`, including pending prompts with codes, paired devices, revoke and the settings drawer.
- Accept: prompt appears within 500 ms with the correct code. Revoke takes effect immediately. Config changes reach the test core.

**D2. Inspector changes.** Source badge, device and app filters, new columns, SDK context in Overview, lazy body states, redaction badge, search fields.
- Accept: with `sdk-test-core` replaying a recorded session, all fields display correctly.

**D3. Merged flow view.** App and Wire toggle and timings from both sources.
- Accept: a device using both SDK and proxy shows one row with both views.

**D4. Performance check.** 10,000 mixed flows.
- Accept: smooth scrolling and filter under 100 ms.

---

## M3: Android SDK

**A1. Core module.** Transport with pinning and pairing, session, bounded queue, ring buffer, config handling, redaction, discovery order, reconnect backoff, token storage, screen tracking, debug banner and logs, App Startup.
- Accept: pairs and streams synthetic events to `sdk-test-core`. Disconnected fast path allocates nothing (verified in a test). Queue overflow drops oldest and counts.

**A2. OkHttp module.** Interceptor with tee, event listener for timings (composed with an existing listener), failure handling, skip rules, streaming.
- Accept: JVM tests in `09-testing-benchmarks.md` pass. Bodies are never consumed or altered. Redirects and retries are represented.

**A3. Retrofit support and annotations.** `Invocation` reading, route templates, `@Inspect`, `annotations` module.
- Accept: template and metadata correct for GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, `@HTTP`, path and query params, base URL with a path prefix, and suspend functions.

**A4. Ktor module.** Plugin using the approach decided in S4. Metadata attributes and helpers. OkHttp-engine deduplication.
- Accept: tests pass on OkHttp and CIO engines, including streaming.

**A5. No-op module and parity.** Identical public API.
- Accept: the parity test passes and fails when an API differs.

**A6. Gradle plugin.** Variant wiring, instrumentation from S2, manifest overlay, ProGuard rules, extension, `inspectorDoctor`.
- Accept: TestKit tests pass. A fresh sample with only the plugin applied streams calls. Adding a manual interceptor does not duplicate captures. Release uses only the no-op.

**A7. Sample app and device checks [dev].** Retrofit and Ktor sample. Run the end-to-end items 1 to 5 of the list in `09-testing-benchmarks.md`.
- Accept: items pass, or are recorded as not run with the reason.

**A8. Android docs and publishing setup.** README quick start (three lines), support matrix, troubleshooting, publishing configuration for Maven Central and the Gradle plugin portal. Do not publish without the user's go-ahead.
- Accept: a clean project can consume the locally published artifacts following only the README.

---

## M4: iOS SDK [mac]

**I1. Core target.** Transport with pinning and pairing, session, queue, ring buffer, config, redaction, Keychain storage, Bonjour discovery, Local Network handling, debug banner, logs.
- Accept: pairs and streams synthetic events to `sdk-test-core` from the Simulator.

**I2. URLSession capture.** Implement the approach chosen in S3 plus the explicit `Inspector.session(...)` helper. Timings from `URLSessionTaskMetrics`.
- Accept: matrix from S3 passes for the chosen approach. Bodies and callbacks unchanged. Redirects represented.

**I3. Alamofire target.** `InspectorMonitor`, `.inspect(...)` modifiers, deduplication with URLSession capture.
- Accept: request, upload, error and redirect cases captured once with metadata.

**I4. Screen tracking and doctor.** `viewDidAppear` tracking, `setScreen`, and the plist doctor command.
- Accept: the doctor reports missing keys and prints the fix.

**I5. Debug-only guarantee.** `#if DEBUG` stubs (or the two-product fallback), AutoStart target.
- Accept: Release build of the sample contains no SDK symbols or the service name. Debug build starts with only the AutoStart product linked.

**I6. Sample app and device checks [dev].** SwiftUI sample with URLSession and Alamofire. Run end-to-end items 6 and 7.
- Accept: items pass, or recorded with reasons.

**I7. iOS docs and publishing setup.** README, support matrix, Info.plist instructions, troubleshooting, SwiftPM release tagging instructions. Do not tag or publish without the user's go-ahead.
- Accept: a clean Xcode project can add the package locally and follow the README.

---

## M5: Hardening and release readiness

**H1. Benchmarks.** Run all scenarios in `09-testing-benchmarks.md` on Android and iOS. Write `docs/sdk-bench.md`.
- Accept: targets in `08-performance-security.md` met, or gaps documented with a plan.

**H2. Security review.** Write `docs/security-review.md` as described in `08-performance-security.md`.
- Accept: all items covered, residual risks listed, pairing design flagged for external review.

**H3. End-to-end run.** Execute the full checklist in `09-testing-benchmarks.md` against the real Linux app and write `docs/e2e.md` with results.
- Accept: every item passes or is recorded as not run with the reason.

**H4. Compatibility matrix and versioning.** Final supported versions per SDK, protocol version policy (core must accept older SDK protocol versions), changelog format.
- Accept: the core rejects an unsupported protocol version with a clear error that the SDK logs.

**H5. Top-level docs.** A single `docs/sdk-quickstart.md` for the repo covering Android and iOS in under one page each.
- Accept: a new developer can follow it from zero to flows on the desktop.

---

## Acceptance for the whole feature

- A fresh Android app with only the Gradle plugin applied shows flows in the Linux app within 1 s of its first request, with no other code or configuration.
- A Retrofit call with `@Inspect` shows name, owner and route template.
- A Ktor client with `install(InspectorPlugin)` works, including a streaming response.
- A USB-connected Android phone works through `adb reverse` with Wi-Fi off. A phone on the same Wi-Fi is found through mDNS.
- An iOS Simulator app with `Inspector.start()` shows URLSession and Alamofire calls. A physical iPhone on Wi-Fi works after the Local Network prompt.
- Pairing shows matching 6-digit codes on both sides. Revoking forces a re-pair. A mismatched certificate is detected.
- A certificate-pinned app shows full flows.
- With no desktop connected, the SDKs do no capture work and the benchmark shows no measurable overhead.
- Release builds of both samples contain only the no-op or no SDK code, verified automatically.
- A device using both SDK and proxy shows one merged flow.
- A hostile client cannot crash or stall the core. Fuzzers run clean.
- Benchmarks meet the targets or the gaps are documented.
- Security review written. Pairing design flagged for external review.
