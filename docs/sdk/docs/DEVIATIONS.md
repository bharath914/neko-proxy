# Deviations from the plan

Where the repo differed from `02-core-assumptions.md`, or a decision changed. Newest last.

| # | Plan said | Repo / decision | Impact |
|---|---|---|---|
| 1 | Reserved nullable flow fields `source`, `device_id`, `app_id`, `tag`, `route_template` exist | They do not. Only `tags` exists. | C2 adds all SDK columns in a schema v2 migration (`PRAGMA user_version` 1 to 2). |
| 2 | Ingest port 8898 | The MCP server already listens on `127.0.0.1:8898` (Phase 2, registered by users with `claude mcp add`). | SDK ingest default port is **8897**. All docs, SDK candidates (`10.0.2.2:8897`, `localhost:8897`, `adb reverse tcp:8897`) use it. |
| 3 | Desktop app is Gradle / Compose Multiplatform under `app/` | Desktop is native Rust (egui) in `desktop/`. | D1 to D4 are egui screens in `desktop/src/ui/`. The "shared module" view-model idea maps to `desktop/src/model.rs` + `backend.rs`. |
| 4 | App talks to the core on `$XDG_RUNTIME_DIR/proxy/api.sock` | Same socket, but the hub/API types live in `core/crates/api` and `proto/api.proto` (package `proxy.api.v1`). | Device calls are added to that proto and crate. |
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
