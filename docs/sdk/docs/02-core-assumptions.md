# What this plan assumes about the existing core

This feature is built on top of Phase 1 of the proxy project. Verify each assumption against the actual repo before starting. Record differences in `docs/DEVIATIONS.md` and adapt the plan.

## Repo and stack

- Cargo workspace under `core/crates/` with crates `net`, `tls`, `flow`, `store`, `decode`, `api`, `cli`
- `proto/api.proto` defines the app-to-core API only
- Gradle Kotlin Multiplatform project under `app/` with `shared` and `desktopApp` (JVM, Linux)
- The core runs as a sidecar process `proxyd` and the app supervises it

## Data model

- A `flow` record with at least: id, started_at, duration_ms, scheme, host, method, path, query, status, protocol, app, req_size, res_size, req_blob, res_blob, timings_json, flags, tags
- Reserved nullable fields added in Phase 1 for this feature: `source` (default `proxy`), `device_id`, `app_id`, `tag`, `route_template`. If they are missing, add them with a migration as the first core task
- SQLite in WAL mode, bodies as content-addressed zstd blobs on disk, full-text index over headers and body text
- Writes are asynchronous and off the data path

## API between app and core

- WebSocket with protobuf over a Unix domain socket at `$XDG_RUNTIME_DIR/proxy/api.sock`
- Calls: `subscribe(filter)`, `get_flow(id, parts)`, `search(query, range)`, `capture.start/stop/status`, `export(har, ids)`, `clear`
- Deltas are batched about every 16 ms
- The app only talks to the core through this API

## Pipeline

- The flow pipeline has request and response interceptor hooks (an empty chain in Phase 1). SDK flows do not pass through it, because they were not proxied

## Paths on Linux

- Data directory `$XDG_DATA_HOME/proxy` (default `~/.local/share/proxy`)
- CA files `ca.pem`, `ca.key` there. The SDK ingest uses a separate certificate, see `03-protocol.md`

## What this plan adds

- Core: `sdk` crate, `proto/sdk.proto`, new tables, new API calls, `sdk-test-core` dev binary
- App: Devices screen, Inspector changes
- New top-level `sdk/` directory containing the Android and iOS SDKs

## Target layout after this plan

```
proxy/
├─ proto/
│  ├─ api.proto               extended with device calls
│  └─ sdk.proto               new: device-to-core protocol
├─ core/crates/sdk/           new
├─ app/                       Devices screen and Inspector changes
└─ sdk/
   ├─ android/
   │  ├─ core/  okhttp/  ktor/  annotations/  noop/  gradle-plugin/  sample/
   └─ ios/
      ├─ Package.swift
      ├─ Sources/ (ProxyInspector, ProxyInspectorAutoStart, ProxyInspectorAlamofire)
      ├─ Tests/
      └─ Sample/
```
