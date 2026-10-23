# Security review: in-app SDK

Written by the author of the code, so it checks the design against its own threat table, not independently.
**The pairing design is custom and must be reviewed by someone with security experience before the SDKs are published.**

Scope: `core/crates/sdk` (ingest), `sdk/android/*`, and the desktop's Devices screen. iOS is not built (see `docs/sdk/docs/e2e.md`).

## 1. Pairing protocol

Design: commit-reveal over a pinned self-signed TLS channel, a six-digit code compared by a person, then a 256-bit token.

| Property | How it holds | Evidence |
|---|---|---|
| A man in the middle between the SDK and the real desktop is noticed | The code is `H(fp || client_nonce || server_nonce)` where each side uses the fingerprint it actually saw. A relay that terminates TLS itself presents its own certificate, so the SDK's code differs from the real desktop's. | Rust `a_mismatched_certificate_shows_a_different_code`; the derivation is identical in Rust and Kotlin (known answer `730832`). |
| The attacker cannot grind a certificate or nonce so both codes match | The client commits `SHA-256(client_nonce)` before it sees the server nonce, and reveals after. The relay must choose its nonce toward the SDK before it learns the client nonce, so it cannot search for a collision of the six digits. Success chance per attempt is about 1 in 1,000,000. | Design; `verify_commit` enforced in `session::pair` (violation otherwise). |
| Repeated attempts | One open prompt per install, 20 s cooldown after a deny or timeout, at most 5 open prompts, 60 s expiry. | `pairing::tests::prompts_limit_duplicates_cap_and_cooldown`, `ingest::deny_is_remembered...`. |
| Replay of an old exchange | Both nonces are fresh per attempt and bound into the code; the token is issued only after a person allows that attempt. | Design. |
| Downgrade | There is one protocol version in use; an unsupported version gets an `Error` and a close. There is no unauthenticated fallback and no plaintext mode. Pinned clients refuse any other certificate. | `unsupported_protocol_versions_get_a_clear_error`. |
| A person approves the wrong thing | The prompt shows device name, model, app id, connection and the code. Nothing stops a rogue desktop *the user chooses to trust* from showing a matching code: pairing proves "the same endpoint on both screens", not "my endpoint". | **Residual risk R1.** |

Weak points to ask a reviewer about: (a) six digits is a UX choice; an online MITM gets about 1e-6 per attempt, and the prompt rate limits bound attempts, but the gap to a stronger code is real. (b) The code is derived from SHA-256, not an HMAC or KDF; nothing here needs a secret, but a reviewer may prefer a labelled construction. (c) The client nonce is attacker-supplied from the desktop's side; it is only hashed, never parsed.

## 2. Token storage and revocation

- 32 random bytes from the OS RNG (`getrandom`, `SecureRandom`). The desktop stores only SHA-256 of it (`devices.rs`, test `only_the_hash_is_stored`) in `sdk-devices.db`, a separate file from the flow store so wiping traffic cannot unpair devices and an export cannot include hashes.
- Compared in constant time (`subtle::ConstantTimeEq`), against a fixed-size value even when no hash exists.
- Scoped to (install id, app id). `check_token_*` tests cover other app, other install, revoked, denied.
- Revoke deletes the hash and closes the live session immediately (`revoke_drops_the_session_and_forces_a_new_pairing`). The SDK then gets `PAIRING_REQUIRED`.
- On the device: a file in `noBackupFilesDir`, owner-only permissions. Not Keystore-encrypted. A rooted device or an app-data dump can read it. The token authenticates only to the paired desktop's ingest; it does not grant anything else. **Residual risk R2.**
- Replacing the ingest certificate revokes every pairing (`regenerate_certificate`), and the desktop asks for confirmation.

## 3. The ingest listener as hostile input surface

Everything arriving on the ingest port is hostile until authenticated, and mostly after.

| Control | Where |
|---|---|
| Off by default, never persisted across restarts, listens on the LAN only while on | `Sdk::set_enabled`, `nekod run --sdk`; DEVIATIONS 12 |
| 1 MiB frame cap, checked before the body is read | `frame::read_envelope` |
| Slowloris: 10 s handshake deadline, 15 s per frame, 60 s idle | `Limits`, test `hostile_input_closes_the_connection...` |
| Pre-auth connection caps: 16 total, 4 per address | `Gate`, test `pre_authentication_connections_are_capped_per_address` |
| 8 authenticated sessions | `too_many_sessions_are_refused` |
| Event rate limit per session (token bucket, closes a session that stays over for 5 s) | `session.rs`, `floods_are_dropped_and_counted...` |
| In flight calls 256, body 8 MiB per call and direction, headers 200 and 64 KiB, URL 8 KiB, text fields 512 bytes | `assembler.rs` and its unit tests |
| Inflate bombs: chunk limit 256 KiB | `deflate_chunks_inflate_and_bombs_are_refused` |
| Out of order, duplicate and gapped chunks dropped and counted | `duplicate_and_out_of_order_chunks...` |
| No call data before `AUTHENTICATED`; server-to-client messages from a client are violations | `no_call_data_is_accepted_before_authentication`, `hostile_input...` |
| `device_id`, `app_id`, `source` come from the session, never the payload | `assembler::call_start` (`who` argument) |
| A slow or stalled device cannot stall the core or other sessions | bounded outbound queue (dropped when full), inline non-blocking assembly, per-session tasks |
| Fuzzing | `core/crates/sdk/fuzz`: `frame_reader` (about 2.0M runs) and `envelope_decoder` into the assembler (about 3.5M runs), 60 s each, no crashes. Run with `--sanitizer none` because this toolchain has no ASAN runtime (DEVIATIONS 14): panics and hangs are caught, memory errors in safe Rust are not possible, `unsafe` is not used in the crate. |

Not covered: connection-level DoS from many distinct addresses (the caps are per address and global); a LAN attacker can fill the 16 pre-auth slots and starve a real phone for the handshake timeout. **Residual risk R3**, mitigated by the toggle being off by default.

## 4. Release builds contain no SDK

- Gradle plugin: release (non-enabled) variants get `inspector-noop` only.
- Verified on real APKs by `PluginTest` (AGP 9.4.1 and 8.13.1): the release dex contains no `Ldev/nekoproxy/inspector/internal/`, no `_nekoinspect`, no `InspectorOkHttp` call, and no `assets/inspector.properties`.
- `./gradlew verifyNoInspectorInRelease` runs the same scan on any project and fails the build.
- The no-op has the same public API, enforced by `ApiParityTest` (checked to fail when a method is removed).
- iOS: not built.

## 5. Redaction

Default on the device (`Redactor.kt`, `RedactorTest`): Authorization, Proxy-Authorization, Cookie, Set-Cookie, headers containing api-key, apikey, secret, auth, or ending in -token; query parameters and JSON and form body values under password, token, access_token, refresh_token, id_token, secret, client_secret, api_key, authorization, otp, pin, cvv, cvc, card_number, ssn, and the per-device extra keys. A JSON body without a content type is treated as JSON. STRICT sends only a safe list of headers and no bodies. NONE is a per-device choice on the desktop, with a warning, never the default.

Gaps: (a) redaction is by key name, so a secret under an unusual key, in a free text field, in a non-JSON/form body, or in a URL path segment is not hidden. (b) Binary and other content types are not inspected. (c) The scanner never fails on malformed JSON but stops redacting where the text stops making sense. (d) Redaction state is inferred from `changed`, so `request_redacted` and `response_redacted` are both set when either side changed something. **Residual risk R4.** The core applies its own redaction again at export and in MCP, so a missed value still gets the proxy-side rules, which are value-shaped (token-like), not only key-shaped.

## 6. Other

- No third-party network calls: the SDK talks only to the paired desktop. No analytics.
- No on-device persistence of traffic. Lazy bodies live in a 5 MB in-memory ring and are lost on process exit.
- Captured content is treated as data in the desktop: bodies render through the existing safe viewers; the SDK fields (name, tag, screen, route) are clipped to 512 bytes and rendered as text.
- The SDK reads nothing beyond the calls it observes plus the device and app metadata in `Hello` (model, OS version, device name, app id and version). The device name can be personal ("Asha's iPhone"); the user sees it in the prompt.

## 7. Residual risks

| Id | Risk | Mitigation or acceptance |
|---|---|---|
| R1 | A user can approve a rogue desktop that shows the same code. | The prompt names the desktop on the device side; a pinned pairing never re-pairs silently. Accepted. |
| R2 | Token on the device is not hardware-protected. | Debug builds only; token is only good for the paired desktop. Accepted. |
| R3 | Pre-auth slot exhaustion from the LAN. | Toggle off by default; caps and timeouts. Accepted. |
| R4 | Key-name redaction misses unusual secrets. | Per-device extra keys; STRICT; core re-redacts at export and in MCP. Accepted. |
| R5 | Six-digit code strength. | Rate limits; **ask the reviewer**. |
| R6 | `sdk-test-core` and `fake-device` accept any certificate and auto-approve. | Dev tools; never built into `nekod`'s default path (the test client is in the library but unused by `nekod`). |
| R7 | Fuzzing ran without a sanitizer. | See DEVIATIONS 14. |

## 8. Fuzz and test status at the time of writing

Rust: 40 sdk unit tests, 20 ingest integration tests, 3 `sdk-test-core` tests, 2 golden-frame tests, 15 API tests. Kotlin: wire codec against the Rust golden frames, engine, redactor, OkHttp, Ktor, noop parity, six Gradle plugin builds. All passing when this was written.
