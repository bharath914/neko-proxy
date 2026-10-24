# SDK benchmarks

Run: `cargo build --release -p sdk --bins` then `./gradlew :okhttp:bench -PsdkTestCore=target/release/sdk-test-core` in `sdk/android`.
Harness: `sdk/android/okhttp/src/test/kotlin/dev/nekoproxy/inspector/Bench.kt`, the local server `sdk/test-server/server.py`, and `sdk-test-core` as the desktop.

**What this is, and is not.** These are **JVM numbers from the Linux development machine** (OkHttp 5.5.0, JDK 21, loopback, a release build of the Rust ingest server), run while other builds were also using the machine.
They measure the SDK's work on the calling thread and its CPU and memory cost relative to a baseline. They are not device numbers: no Android device or timing-faithful emulator was available (see `docs/sdk/docs/e2e.md`), and iOS was not built. CPU is process CPU time divided by wall time (all cores, so 100% is one core). Heap delta is noisy because it is read after a `System.gc()` before and none after.
Each scenario was run more than once; the numbers below are from the final run, and the earlier runs agreed within noise on every latency figure.

| Scenario | p50 ms | p99 ms | CPU | heap delta | bytes to desktop | dropped | note |
|---|---|---|---|---|---|---|---|
| 1 baseline, no SDK (1 KB, 50 req/s) | 0.93 | 1.43 | 13% | 8,169 KB | 0 | 0 | |
| 1b baseline, 100 KB | 1.83 | 2.90 | 16% | 9,587 KB | 0 | 0 | |
| 1c baseline, 5 MB | 42.52 | 46.57 | 57% | 85,553 KB | 0 | 0 | |
| 1d baseline, burst of 1,000 on 16 threads | 1.73 | 6.54 | 55% | 967 KB | 0 | 0 | |
| 2 SDK present, no desktop connected | 0.79 | 1.23 | 14% | 185 KB | 0 | 0 | |
| 3 connected, 1 KB JSON, 50 req/s | 0.82 | 1.30 | 20% | 4,860 KB | 351,025 | 0 | about 585 bytes per call |
| 4 connected, 100 KB JSON, 50 req/s | 1.75 | 3.55 | 25% | 128,557 KB | 113,726 | 0 | 100 KB body is over the 64 KB eager limit: kept on the device, not sent |
| 5 connected, 5 MB download | 48.00 | 55.64 | 82% | 151,727 KB | 3,830 | 0 | over the 1 MiB cap: kept on the device |
| 6 burst of 1,000 requests | 2.04 | 8.23 | 74% | 11,146 KB | 586,585 | 0 | |
| 7 streaming response, 10 s | 10,022 | 10,022 | 1% | 967 KB | 3,655 | 0 | 100 SSE events; the 60 s run of the plan is a manual check |
| 8 desktop stalled (ingest process stopped) | 0.87 | 1.14 | 15% | 11,106 KB | 351,002 | 0 | the app does not notice |

## Against the targets (`docs/sdk/docs/08-performance-security.md`)

| Target | Result | Verdict |
|---|---|---|
| Disconnected: no measurable change in latency or allocation | p50 0.79 ms vs 0.93 ms baseline (within noise, not slower). The allocation test (`EngineTest.whenNothingIsConnectedACallDoesNoCaptureWork`) shows under one byte per call over 100,000 calls. | Met |
| Connected: under 1 ms added on the calling thread for bodies up to the cap | 1 KB: 0.82 vs 0.93 ms. 100 KB: 1.75 vs 1.83 ms. Burst: p50 2.04 vs 1.73 ms, p99 8.23 vs 6.54 ms. 5 MB (over the cap): 48.0 vs 42.5 ms. | Met for ordinary calls. **Gap:** a body that reaches the 1 MiB cap costs about 5 ms on the calling thread (the copy as the app reads it) and the burst p99 is about 1.7 ms worse. Plan: copy into a pooled chunk list instead of a growing array, and stop copying once a body is over the cap and lazy. Not done. |
| Connected: around 2% CPU under steady load on a mid-range device | +7 points of one core at 50 req/s here (20% vs 13%), measured for the whole JVM including the benchmark's own threads. | **Gap, undetermined.** The harness cannot separate the SDK's sender thread from the rest, and a laptop is not a mid-range phone. Plan: measure on a device with a profiler (`Debug.threadCpuTimeNanos` on the `inspector-io` thread). |
| Bounded memory | Heap deltas are noisy but bounded: the 5 MB scenario peaks at about 150 MB heap in the benchmark process, of which the baseline already uses 85 MB; the SDK's own buffers are the 5 MB ring plus the 2,048 entry queue. | Met in design, not precisely measured |
| Desktop: 2,000 events/s from one session without dropping, 8 sessions | Scenario 6 sent 1,000 calls in about 0.25 s with 0 dropped. The Rust ingest tests run 8 sessions. A dedicated 2,000 events/s test against the Rust server is not written. | Partly met |
| Slow or stalled desktop does not affect the app | Scenario 8: latency unchanged with the desktop process frozen. The send queue is bounded (drop oldest, counted) and the engine never blocks the calling thread. | Met |

## Other measurements

- Rust ingest, 10,000 mixed SDK and proxy flows: every search and filter under 15 ms (`ten_thousand_mixed_flows_search_and_filter_fast`).
- Desktop, 10,000 rows: average frame 0.5 ms, device filter applied in about 12 ms (`ten_thousand_flows_scroll_and_filter_smoothly`).
- Wire size: JSON bodies over 512 bytes are raw-deflated per chunk, only when that shrinks them.
