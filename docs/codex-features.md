# Core and Linux feature handoff

Worktree: /Users/gani/Projects/proxy-codex, branch codex/traffic-tools.
The primary checkout is on rename-neko and was left unchanged. Nothing was
pushed and no PR was opened. No desktop-macos source was edited by this work.

| Task | Result and test note |
|---|---|
| 1 SOCKS5 | Existing listener detects SOCKS5 and enters the CONNECT pipeline; [note](socks5.md). |
| 2 SSE | Bounded live snapshots, persistence, event decoding and virtualized Linux viewer; [note](sse.md). |
| 3 HAR import | neko/nekod import-har and additive API, fresh shared IDs and round-trip tests; [note](har-import.md). |
| 4 Protocol views | Schema-free protobuf field numbers, uncompressed gRPC frames and GraphQL request views; [note](protocol-decode.md). |
| 5 Scenarios | Every-N failures, jitter, flaky upstream simulation, YAML reload, editor and approved MCP tool; [note](scenarios.md). |
| 6 Export/codegen | Postman, k6, WireMock, MSW, Mockoon, OpenAPI, four model languages and Dio; [note](export-codegen.md). |
| 7 Issues | Core worker flags, sensitive scanner, search and Linux rendering; [note](detected-issues.md). |
| 8 MCP | Redacted export/issue/SDK/catalog reads and app-approved writes; [note](mcp-traffic-tools.md). |
| 9 Packaging | Linux tar/deb/AppDir/AppImage workflow and portable installer; [note](linux-packaging.md). |

Each note records tests, failures corrected, proto changes and limits. Initial
SOCKS5/SSE source edits were accidentally included by the concurrent macOS
commits 1ea37b3 and 84fa464 in the shared checkout. Work moved to an isolated
worktree after that collision. Task 1 tests/docs are in 01c1376; task 2 note in
36ed3af. Later commits on codex/traffic-tools keep the feature steps separate.

## Proto handoff for Claude

All changes are additive. Regenerate Swift bindings from this branch's schema
with desktop-macos/gen-proto.sh after integrating it. Existing generated Swift
files were not modified; new oneof cases may need handling in exhaustive app
switches. macOS implementation and verification remain with Claude.

- BodyView.events = 16 and SseEvent.
- BodyView.protocol = 17, ProtocolView and DecodedField.
- ClientMessage.import_har = 44; ServerMessage.imported_har = 28;
  ImportHarRequest and ImportHarResponse.
- Rule.scenario = 15 and ScenarioAction.
- FlowDetail.dio = 30; FlowDetail.issues = 31.
- FlowSummary.issue_labels = 28; issue flag bits 14 through 19.

No fields or numbers were removed or renumbered. Live snapshots add only an
internal Rust FlowEvent variant, with no further wire change.

## Final verification

- cargo build -p neko-cli -p neko-desktop: passed (existing dead-code warnings).
- cargo test --workspace: 290 passed, 0 failed.
- NEKO_SHOT_DIR=/tmp/neko-final-shots cargo test --workspace includes 44 desktop
  render tests and PNGs; live SSE and stored snapshot regression tests pass.
- Six Python Linux packaging tests and shell syntax checks passed.
- Fresh executable smoke test: neko -> nekod HAR import/API/persistence/Postman
  export passed against an isolated daemon with file CA storage and SDK/MCP off.
- Generated Postman/OpenAPI documents validate; Swift and TypeScript type-check;
  JavaScript syntax checks pass. Mockoon CLI served the expected recorded JSON.
- Release network benchmark: 0.022 ms p50 proxy overhead; 126,008 requests/s for
  5 s, 64 connections, local origin. This excludes disk persistence.
- Release store benchmark: 100k flows ingested in 2.3 s; searches 8.4–35.7 ms.

Final safety fixes bound export metadata and decoded aggregates before artifact
construction and redact malformed JSON, SSE payloads and plaintext credentials.
The API preserves existing directory permissions; its socket remains private.
The Linux installer follows the renamed bharath914/neko-proxy repository.

Remaining verification: real Linux ELF builds, dpkg/RPM installation and AppImage
launch/display smoke tests need a Linux host. Tests here used inert ELF fixtures
and an explicit AppImage command test double, not fabricated release packages.
Dart/Kotlin compilers and WireMock runtime were unavailable. Compressed gRPC and
gRPC-Web are not decoded. Schemas/models and issue flags are bounded best-effort
inferences; per-feature notes describe their sampling limits. Cold start, idle
memory and actual desktop first paint were not re-benchmarked.
