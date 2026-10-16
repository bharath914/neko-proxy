# In-app SDK feature

Build the in-app SDK feature described in `docs/`. Android (OkHttp, Retrofit, Ktor) and iOS (URLSession, Alamofire), plus the core ingest and desktop app changes they need.

Read `docs/01-overview.md` first, then work through `docs/10-tasks.md` in order. Each task lists acceptance checks. Do not mark a task done until they pass.

## Scope

In scope: core `sdk` crate and `sdk.proto`, desktop app Devices screen and Inspector changes, Android SDK, iOS SDK, Gradle plugin, no-op artifacts, sample apps, tests, benchmarks.

Out of scope: Flutter and web SDKs, route catalog and MCP tools (Phase 3), relay for remote devices, WebSocket frame capture, release-build support, rules and breakpoints.

## Hard rules

- The SDK is debug-only. Release builds must contain only a no-op with the same public API. Verify this, do not assume it.
- A request must never wait on the desktop. All SDK work on the request path is a flag check, a bounded copy, and a queue push.
- With no desktop connected, do no capture work at all.
- No on-device persistence of captured traffic.
- The ingest channel is TLS only, with pinned server fingerprint after pairing. Never send captured data to an unpaired desktop.
- Everything arriving at the ingest port is hostile input: size limits, rate limits, bounded buffers, a fuzz target.
- Redact on the device by default. Revealing sensitive values is a per-device choice made on the desktop.
- Do not copy code from other open-source projects (Netfox, Pulse, Chucker, Sentry and similar) without checking its license is compatible. Reading them to learn the approach is fine.

## Working style

- Be terse. Implement without asking for approval at each step. Ask only when a decision is genuinely open and blocks progress.
- Where a doc says "verify" or "spike", do the check against current official documentation and the real library versions, and record the result in `docs/spikes/<name>.md`. Do not rely on memory for API details.
- If an assumption about the existing core is wrong, adapt, and record the difference in `docs/DEVIATIONS.md`.
- iOS work needs macOS and Xcode. If the current machine is not a Mac, skip M4, say so, and continue with the rest.
- Keep one commit per task where practical, with the task id in the message.
