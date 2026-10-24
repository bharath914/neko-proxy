# Feature backlog

Long-term backlog. Phase 1 scope (Linux only, capture core and inspector) is defined in `phase-1.md` and is narrower than the M1 and M2 items below. Everything not in `phase-1.md` is deferred.

Milestones refer to the build order in `CLAUDE.md` (M1 capture core, M2 desktop app, M3 rules and MCP, M4 phone pairing, M5 reach).

## Capture and protocols
- HTTP/1.1, HTTP/2, WebSocket frame viewer (M1)
- HTTPS interception with per-install CA (M1)
- SOCKS5 and transparent mode (M1)
- Certificate pinning detection, tunnel and flag (M1)
- SSE viewer (M2)
- gRPC and GraphQL views (M5)
- HTTP/3 (QUIC) (M5)
- Per-app capture, filter by app or process (M4)
- Frida script generator for pinning bypass, Android and iOS (M5)

## Inspection and debugging
- Fast flow table, 100k+ rows, filter and full-text search (M2)
- Header, body, timing viewers: JSON, form, images, protobuf (best effort, or with schemas) (M2)
- Timing waterfall: DNS, connect, TLS, TTFB, download (M2)
- Replay, edit and resend, bulk replay (M3)
- Flow diff, request vs request and session vs session (M3)
- Auto-detected issues: slow calls, duplicate calls, large payloads, missing cache headers, N+1 patterns (M5)

## Rules, mocks and testing
- Mock response, map local, map remote, rewrite, throttle (M3)
- Breakpoints on request and response with edit and resume (M3)
- Mock from a captured flow in one click (M3)
- Scenario mocks: fail 1 in N, delay, flaky network (M3)
- Record and replay a full session for offline work (M5)
- Export mocks to WireMock, MSW, Mockoon (M5)
- Generate tests from traffic: Postman, curl, k6 (M5)
- Schema drift detection against OpenAPI (M5)

## Developer experience
- Copy as curl, fetch, Dio, Retrofit (M2)
- Generate models in Dart, Kotlin, Swift, TypeScript from responses (M5)
- OpenAPI spec generated from traffic (M5)
- Command palette and keyboard-first navigation (M2)
- JS or Lua scripting hooks on request and response (M5)

## Team and CI
- Headless CLI with assertions, for example fail the build if any call exceeds 2 s (M1)
- Git-friendly YAML rules and config (M3)
- HAR import and export (M1)
- Shareable session links, self-hostable (M5)
- Team mock server from shared rules (M5)

## Security
- Redaction in the core before MCP and export (M3)
- Sensitive-data scanner for tokens, PII, secrets in cleartext (M5)
- Basic checks: insecure headers, cleartext HTTP (M5)

## Mobile SDK (see `docs/sdk.md`)
- Certificate page at `http://neko.local`: per-platform steps, PEM, DER and Apple profile downloads, live trust check (done)
- QR pairing (M4)
- Android SDK: route the app's traffic, trust the CA at runtime in debug builds (M4)
- iOS SDK, same for URLSession (M4)
- Tag flows by app, build and device (M4)
- Pinning override in debug builds (M5)
- Shake to report: attach recent flows to a bug report (M5)
- Android VPN mode and on-device capture (M5)

## AI layer (MCP)
- Read tools: list, search, get, diff, export HAR (M3)
- Approved write tools: create mock, add breakpoint, replay, start and stop capture (M3)
- Compact summarized output to save tokens (M3)
- Example prompts to document: "why is this API returning 401", "generate Dart models from these responses", "mock a 500 on /payments and tell me how the app behaves" (M3)
- Post-session anomaly summaries (M5)
