# Performance and security

## Performance rules

These are design requirements. Targets are goals to verify in M5, not measurements.

1. **Capture only while someone is listening.** When no desktop is connected or capture is off, the cost per request is one volatile or atomic flag read. No body tee, no allocation.
2. **Never wait on the desktop.** The request path does a flag check, a small snapshot, a bounded copy of body bytes, and a queue push. Serialization, compression, redaction of large bodies and socket writes happen on one background thread or queue.
3. **Bounded everything.** Bounded send queue (drop oldest event when full, count drops), bounded ring buffer (about 5 MB), body cap 1 MB by default, in-flight call limit.
4. **Skip what is not useful.** Do not capture `image/*`, `video/*`, `audio/*` or large binary bodies. Record size and content type only, with a truncated flag.
5. **Compress and defer.** Deflate body chunks on the wire. Send bodies up to 64 KB eagerly. Keep larger bodies in the ring buffer and send them only on `BodyFetch`.
6. **No on-device persistence.** Nothing about captured traffic is written to disk or a database. Only the token, fingerprint and install id are stored.
7. **Cheap startup.** Initialization registers callbacks and reads stored pairing. Discovery and connection start lazily on a background thread.
8. **Quiet radio.** Scan mDNS or Bonjour only while disconnected, in short windows, with backoff 2 s, 5 s, 15 s, 60 s. Keep-alive pings only every 15 s of idleness.
9. **No per-request logging.** One log line per connection event.

Targets to verify:
- Disconnected: no measurable change in request latency or allocation versus no SDK
- Connected: under 1 ms added on the calling thread per request for bodies up to the cap, around 2% CPU under steady load on a mid-range device, bounded memory
- Desktop: ingest handles at least 2,000 events per second from one session without dropping, and 8 sessions concurrently

Known tradeoff: lazy bodies are lost if the app is killed or the buffer evicts them before the user opens the flow.

## Security model

Threats and controls:

| Threat | Control |
|---|---|
| Captured data sniffed on the LAN | TLS on the ingest channel |
| Rogue desktop on the same network receives traffic | Never send call data before pairing. Pairing needs a person to match a 6-digit code on both screens. After pairing the SDK pins the server fingerprint |
| Man in the middle during pairing | Commit-reveal nonces and a code bound to the fingerprint each side saw. A mismatch shows different codes |
| Brute-force or grinding of the code | Client nonce is committed before the server nonce is revealed. Limit pairing attempts and prompts |
| Stolen token | 256-bit random, only its hash stored on the desktop, scoped to device and app, revocable. Stored in app-private storage or Keychain on the device |
| Hostile device crashes or stalls the core | Frame and field size limits, per-session rate limits, bounded queues, session caps, timeouts, fuzz targets |
| Prompt flooding on colleagues' desktops | No automatic pairing when several desktops are visible. Per-install prompt cooldown. Pending cap |
| SDK shipped in a release build | Real SDK only in debug variants. No-op in release with an identical API. Automated verification of the release artifact |
| Secrets leaving the device | Default redaction in the SDK before sending. Revealing is a per-device choice on the desktop with a warning |
| Secrets reaching AI tools or exports later | Core applies redaction again at export and in MCP (Phase 3). Captured content is treated as untrusted data |
| Prompt injection from captured bodies | Captured content is data, never instructions, in any AI-facing path (Phase 3) |
| Captured content corrupting the desktop UI | Render bodies as text or through safe viewers. Never execute or load content from flows |

Requirements:
- The ingest listener is off by default, listens on the LAN only while enabled, and the desktop shows when it is on.
- Tokens are compared in constant time.
- Certificate pinning on the client uses SHA-256 of the DER certificate.
- The ingest certificate is regenerated only on explicit user action. If regenerated, all pairings must be redone and the desktop must say so.
- No third-party analytics or network calls from the SDK. It talks only to the paired desktop.
- The SDK must not read or send anything beyond network calls it observes plus the device and app metadata in `Hello`.

## Security review (M5, required before release)

A written review in `docs/security-review.md` covering:
- The pairing protocol (commit-reveal, code derivation, fingerprint binding, replay, downgrade)
- Token storage and revocation
- Fuzz results on the frame reader and envelope decoder
- Verification that release builds on both platforms contain no SDK code
- Review of redaction coverage and defaults
- A list of residual risks

Because the pairing design is custom, get it reviewed by someone with security experience before publishing the SDKs.
