# SDK wire protocol, as implemented

Normative text is `docs/sdk/docs/03-protocol.md`; this page records what the code does where that text left details open. The schema is `proto/sdk.proto`. Golden frames for the two sessions below are in `proto/sdk-fixtures.json` (the Rust test `core/crates/sdk/tests/fixtures.rs` checks them, the Kotlin and Swift tests decode the same file).

## Framing

TLS over TCP, default port **8897** (8898 is the MCP server, see `DEVIATIONS.md`). Each frame is a 4-byte big-endian length, then one `Envelope`. Length 0 and anything above 1 MiB closes the connection (the core answers `Error.FRAME_TOO_LARGE` first). An envelope with no message set is malformed.

## Details the schema does not say

- **Deflate** (`BodyChunk.encoding = DEFLATE`) is raw deflate, RFC 1951, no zlib header, and each chunk stands alone. Android: `Deflater(level, nowrap = true)`. iOS: Compression framework `COMPRESSION_ZLIB`. The core refuses a chunk that inflates past 256 KiB.
- **Pairing code**: `SHA-256(fingerprint ‖ client_nonce ‖ server_nonce)`, first 4 bytes read big-endian as an unsigned integer, modulo 1,000,000, zero padded to six digits. `fingerprint` is the SHA-256 of the DER server certificate, the one the client actually saw. Known answer: fingerprint = bytes 0..31, client nonce = 32..63, server nonce = 64..95 gives `730832`.
- **Commit**: `SHA-256(client_nonce)`. Nonces are 32 bytes.
- **Token**: 32 random bytes. The core stores only SHA-256 of it.
- **Timings** are milliseconds, `-1` unknown. `connect` is the TCP connect only (a client stack that reports connect and TLS together subtracts `tls`). `0` on `connect` means a reused connection.
- **Clock**: the core sets `offset = core_now − hello.device_time_ms`, then refines it with every `Ping` it sends: `offset = (t1 + t3)/2 − t2`. A device clock more than 24 h off is ignored and the core's receive time is used.
- **Lazy bodies**: `CallComplete.*_body_available = false` with `*_body_skipped = false` means the SDK kept the body. The core asks with `BodyFetch`; the SDK answers `BodyFetchResult` and, if available, `BodyChunk`s for the same call and part, ending with `last = true`.
- **Redaction marker**: redacted values are the literal string `[redacted]`. `CallComplete.request_redacted` / `response_redacted` say whether anything was replaced.
- **Versions**: the core accepts `MIN_PROTOCOL_VERSION..=PROTOCOL_VERSION` (both 1 today). Outside that, it sends `Error.UNSUPPORTED_VERSION` with the supported range and closes.
- **Identity** (`device_install_id`, `app_id`) must match `[A-Za-z0-9._:@-]`, 64 and 256 characters at most.

## Example 1: fresh pairing

| # | Dir | Message | Notes |
|---|---|---|---|
| 1 | → | `Hello` | `token` empty |
| 2 | ← | `HelloAck` | `PAIRING_REQUIRED` |
| 3 | → | `PairRequest` | `commit = SHA-256(client_nonce)` |
| 4 | ← | `PairChallenge` | `server_nonce`, `timeout_s = 60` |
| 5 | → | `PairReveal` | `client_nonce`. The desktop prompt appears now, with the code |
| 6 | ← | `PairResult` | after a person pressed Allow: `allowed`, `token` |
| 7 | ← | `Config` | session is authenticated |

## Example 2: reconnect, then one call

| # | Dir | Message | Notes |
|---|---|---|---|
| 1 | → | `Hello` | `token` from the first pairing |
| 2 | ← | `HelloAck` | `AUTHENTICATED` |
| 3 | ← | `Config` | |
| 4 | → | `CallStart` | |
| 5 | → | `BodyChunk` | request body, `last = true` |
| 6 | → | `ResponseStart` | |
| 7 | → | `CallComplete` | |

The hex of each frame is in `proto/sdk-fixtures.json` under `fresh_pairing` and `reconnect`.
