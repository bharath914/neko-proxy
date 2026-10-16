# Testing and benchmarks

## Shared test tool: `sdk-test-core`

A dev binary from the core `sdk` crate that acts as a real ingest server without the proxy or app.

```
cargo run -p sdk --bin sdk-test-core -- --port 8898 --record out.jsonl --auto-allow
```

- `--auto-allow` approves pairing and prints the code
- `--record` writes each received event as JSON lines
- `--config config.json` pushes a fixed `Config`
- `--fetch-lazy` requests lazy bodies automatically
- `--deny` and `--revoke-after <n>` exercise those paths

Both SDKs' integration tests start it, run calls through a local test HTTP server, then assert on the recorded JSON lines.

## Local test HTTP server

A small server (any language) used by sample-app tests and benchmarks, with routes for: small JSON, large JSON, binary, gzip-encoded, redirects, slow responses, errors, streaming (chunked and SSE), request echo, and an auth challenge.

## Core (Rust)

- Unit: frames, pairing math, token handling, assembler, merge, redaction at export
- Integration with a Rust fake SDK client: pairing, reuse, revoke, normal flows, chunked bodies, lazy fetch, reconnect, duplicate and out-of-order chunks, session drop mid-call, config push
- Hostile input: oversized frames, floods, slow senders, bad envelopes before auth, excessive headers
- Fuzz: frame reader, envelope decoder

## Android

JVM tests (no device):
- OkHttp interceptor with MockWebServer: bodies, caps, truncation, redirects, retries, failures, streaming, gzip, redaction, route template extraction, `@Inspect`, event listener composition with a user listener
- Ktor with MockEngine: request and response capture, streaming, metadata attributes, OkHttp-engine deduplication
- Core: queue overflow, ring buffer eviction, reconnect backoff, pairing client logic against `sdk-test-core`
- Gradle plugin with TestKit: dependency wiring per variant, instrumentation applied once, manual interceptor not duplicated, builds without OkHttp, configuration cache
- No-op parity: a test that compares public API of real and no-op modules and fails on any difference
- Release verification task: builds the sample in release and fails if real SDK classes are on the classpath or in the dex

Emulator or device tests (local-only if no emulator is available, and say so in the README):
- Sample app end to end against `sdk-test-core` and against the real Linux app
- Emulator via `10.0.2.2`, USB via `adb reverse`, Wi-Fi via mDNS
- App Startup initialization with no developer code

## iOS (macOS)

- XCTest unit tests: framing, pairing client logic, redaction, queue, ring buffer, Bonjour parsing
- Integration with `sdk-test-core`: Simulator sample app, URLSession (shared and custom session, async/await, completion handler, redirect, auth challenge, cancellation), Alamofire (request, upload, error), screen tracking
- Release verification: build the sample in Release, scan the binary for SDK symbols and the service name, fail if found
- Physical device check on Wi-Fi including the Local Network permission prompt (manual checklist, recorded in the README)

## End to end with the real Linux app

Checklist run in M5 and recorded in `docs/e2e.md`:
1. Android emulator with only the Gradle plugin. First request appears in the app within 1 s of the first request
2. Retrofit call with `@Inspect` shows name, owner and route template
3. Ktor call including a streaming response
4. USB phone with `adb reverse`, Wi-Fi off
5. Phone on Wi-Fi discovered through mDNS, pairing codes match, revoke forces re-pair
6. iOS Simulator with `Inspector.start()`, URLSession and Alamofire calls
7. iOS device on Wi-Fi with the Local Network prompt
8. A cert-pinned app shows full flows
9. A device using both SDK and proxy shows one merged flow
10. Release builds of both samples contain no SDK code

## Benchmarks (`docs/sdk-bench.md`)

Harness in each sample app plus the local test server. For each scenario record p50 and p99 latency, CPU, memory, bytes sent to the desktop and dropped events.

Scenarios:
1. Baseline: no SDK
2. SDK present, no desktop connected
3. SDK connected, small JSON (1 KB), 50 requests per second
4. SDK connected, 100 KB JSON
5. SDK connected, 5 MB download (cap and skip behaviour)
6. Burst of 1,000 requests in 1 s (queue overflow behaviour)
7. Streaming response of 60 s
8. Desktop slow or stalled (backpressure, no effect on app latency)

Compare against the targets in `08-performance-security.md` and document any gap with a plan.
