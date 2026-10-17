# Spike S1: pinned TLS and the pairing code

Status: **done for Rust and Kotlin (JVM and Android's `SSLSocket`); the Swift client is not run** (needs macOS).

## What was built

- Rust server: `core/crates/sdk` (`cert`, `frame`, `pairing`, `session`), a self-signed ECDSA P-256 certificate, `tokio-rustls`.
- Kotlin client: `sdk/android/core` (`Transport`, `Pairing`), plain `SSLSocket` with a custom `X509TrustManager`, no networking library.
- Both derive the code the same way and agree on a known answer (`730832`, in `core/crates/sdk/src/pairing.rs` and `WireTest.kt`).

## Results

| Check | Result |
|---|---|
| Codes match on a normal run | Yes. `EngineTest.pairsStreamsAndReconnectsWithTheStoredToken` compares the code the desktop recorded with the one the SDK logged and showed. |
| Codes differ when a different certificate is presented | Yes. `a_mismatched_certificate_shows_a_different_code` (Rust) shows the code depends on the fingerprint. |
| A paired client refuses a different certificate | Yes, at the TLS handshake, before any message is sent. `EngineTest.aDifferentCertificateIsRefusedAndNoDataIsSent`. |
| No third-party networking library | Yes: JDK `javax.net.ssl` on the client, `rustls` on the server. |
| Kotlin and Rust produce the same bytes | Yes: `proto/sdk-fixtures.json`, decoded and re-encoded byte-for-byte in `WireTest`. |

## Decisions

1. Hostname verification is skipped and the SNI name is the address. The pin replaces it.
2. TLS 1.2 and 1.3 both work with rustls defaults and Android's JSSE.
3. The code is `u32(SHA-256(fp || client_nonce || server_nonce)[0..4]) mod 1,000,000`. The modulo bias is about 0.02%, which does not matter for a code that is compared by eye.
4. Swift: the plan is `NWConnection` with `sec_protocol_options_set_verify_block`. Not verified here.
