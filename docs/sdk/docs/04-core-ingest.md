# Core: SDK ingest (Rust)

New crate `core/crates/sdk`. It runs inside `proxyd`, next to the proxy listener and the app API.

## Crates and dependencies

Suggested: `tokio`, `tokio-rustls`, `rcgen`, `prost`, `mdns-sd`, `sha2`, `rand`, `flate2`, `subtle` (constant-time compares). Pick current versions, no pinned versions are implied here.

## Module layout

| Module | Job |
|---|---|
| `listener` | TLS accept loop on the ingest port. Enabled and disabled at runtime |
| `cert` | Generate and load the self-signed ingest certificate, expose its SHA-256 fingerprint |
| `frame` | Read and write length-prefixed `Envelope` frames, enforce the size limit |
| `session` | Per-connection state machine: hello, pairing, authenticated, closing |
| `pairing` | Commit-reveal nonces, code derivation, pending prompts, token issue |
| `devices` | Persistent device and app registry, token hashes, per-device config, revoke |
| `assembler` | Turn call events into flow records and body blobs, handle order and loss |
| `config` | Build and push `Config` to sessions |
| `discovery` | Advertise `_proxyinspect._tcp` with TXT records while enabled |
| `adb` | Watch for Android devices and run `adb reverse`, report status |
| `merge` | Derive merged flows from SDK and proxy records |
| `bin/sdk-test-core` | Dev binary used by every SDK's integration tests |

## Storage

Add to the existing SQLite schema (names are suggestions):

```
sdk_device(device_id, install_id, name, model, os_version, first_seen, last_seen, revoked)
sdk_app(device_id, app_id, app_name, token_hash, paired_at, last_seen, config_json)
flow(... source, device_id, app_id, tag, name, owner, screen, route_template, client_stack, sdk_call_id, merged_with)
```

- `flow.source` is `proxy`, `sdk` or `merged`. Keep original records and derive the merged view, so un-merging is possible.
- SDK bodies go through the same blob store and the same `decode` crate as proxy bodies.
- Writes to SQLite and blobs happen on a separate task, never on the session reader.
- Full-text index includes SDK fields: name, tag, owner, screen, route template.

## Session handling

- Handshake timeout 10 s. Pairing prompt timeout 60 s. Idle timeout 60 s with ping at 15 s.
- Each session has a bounded inbound queue to the assembler. When the assembler or store is slow, drop body chunks beyond the per-call cap first, mark the flow `truncated`, and count drops. Never block the socket reader indefinitely.
- Limits, all configurable constants: max frame 1 MiB, max in-flight calls per session 256, max body bytes per call 8 MiB regardless of what the SDK sends, max sessions 8, max events per second per session, max header count and size.
- Take `device_id` and `app_id` from the session, never from event payloads.
- Convert the device clock to core time using the offset estimated from `Hello.device_time_ms` and re-estimated from pings.
- Close the session on protocol violations (bad length, unknown envelope before auth, duplicate hello) and log the reason.

## Pairing and tokens

- Tokens: 256 random bits, stored as SHA-256 hash, compared in constant time.
- Pending prompts are published to the app through the devices stream. Allow, deny and revoke come back through the API.
- Rate limit prompts per install id and globally, and ignore repeats while one is pending.
- Denied devices are remembered until the user clears them, with a long reconnect backoff communicated via `DENIED`.

## Discovery and adb

- Advertise only while ingest is enabled. Stop advertising when disabled.
- `adb`: look for the binary in `PATH`, then in `ANDROID_HOME/platform-tools`. If missing, report `adb_unavailable` in status and continue. Poll `adb devices` every few seconds while enabled. For each device in state `device`, ensure the reverse exists. Never run `adb` commands other than `devices` and `reverse`.

## Merge

Implement the rules in `03-protocol.md`. Keep it a pure function over two candidate records, with unit tests for: exact match, close timing, same URL different bodies, redirects, retries, clock skew, and no-device-IP cases.

## API additions (app to core, in `api.proto`)

- `sdk.set_enabled(bool)`, `sdk.status()` returning enabled, port, fingerprint, adb status, discovery status
- `devices.subscribe()`: stream of device list changes and pending pair prompts (with code)
- `devices.allow(pending_id)`, `devices.deny(pending_id)`, `devices.revoke(device_id, app_id)`, `devices.forget(device_id)`
- `devices.set_config(device_id, app_id, config)`
- `flows.fetch_body(flow_id, part)`: triggers `BodyFetch` for lazy bodies and streams the result
- Flow messages gain the new fields (`source`, `device_id`, `app_id`, `name`, `tag`, `owner`, `screen`, `route_template`, `client_stack`)

## CLI additions

- `proxyd sdk enable|disable|status`
- `proxyd devices list|revoke <id>`
- `proxyd flows` gains a `--source sdk|proxy|merged` filter

## `sdk-test-core`

A dev binary that is a real ingest server without the proxy or the app:

```
cargo run -p sdk --bin sdk-test-core -- --port 8898 --record out.jsonl --auto-allow
```

- `--auto-allow` approves pairing automatically and prints the code
- `--record` writes every received event as JSON lines, so SDK tests can assert on what arrived
- `--config config.json` pushes a fixed `Config`
- `--fetch-lazy` automatically requests lazy bodies
- Used by the Android sample tests, the iOS tests, and CI

## Rust tests

- Unit: frame parsing, pairing code derivation, token handling, assembler ordering and loss, merge
- Integration with a fake SDK client written in Rust: pairing, token reuse, revoke, normal flows, chunked bodies, lazy fetch, reconnect, duplicate and out-of-order chunks, session drop mid-call
- Hostile input: oversized frames, floods, slowloris, bad envelopes before auth, huge header counts
- Fuzz targets for the frame reader and the envelope decoder
