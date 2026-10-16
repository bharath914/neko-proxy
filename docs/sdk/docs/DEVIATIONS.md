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
