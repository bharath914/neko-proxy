# Protocol, pairing and discovery

The SDK does not ride on the proxy. It opens its own connection to an ingest port on the desktop core and pushes events. The core identifies SDK traffic because it arrives on that channel, from an authenticated session. It never has to guess.

## Transport

- TLS over TCP. Default port 8898. Separate from the proxy port (8899) and from the app-to-core Unix socket.
- Framing: 4-byte big-endian length, then one protobuf `Envelope`. Maximum frame 1 MiB. Bodies are sent in chunks of at most 64 KiB.
- Why not WebSocket: no HTTP upgrade, no third-party WebSocket library on either client. Android uses `SSLSocket`. iOS uses Network.framework. Rust uses `tokio-rustls`. All dependency-free on the client side.
- Compression: body chunks may be deflate-compressed (`encoding` field). Deflate is available on both platforms without extra dependencies.
- The ingest port is off by default. The desktop enables it with the "Accept SDK connections" toggle. While enabled, it listens on the LAN interface.

## Server certificate

- On first run the core generates a self-signed ECDSA certificate for ingest (`rcgen`), stored in the data dir. It is separate from the proxy CA.
- Clients cannot verify it through a CA. They pin its SHA-256 fingerprint during pairing instead.
- Hostname verification is skipped (connections are by IP). Pinning replaces it.

## Messages

Sketch of `proto/sdk.proto`. Field numbers and details are the implementer's to finalize. Keep a `protocol_version` and make unknown fields safe to ignore.

```proto
syntax = "proto3";
package sdk.v1;

message Envelope {
  oneof msg {
    Hello hello = 1;             // client -> server
    HelloAck hello_ack = 2;      // server -> client
    PairRequest pair_request = 3;
    PairChallenge pair_challenge = 4;
    PairReveal pair_reveal = 5;
    PairResult pair_result = 6;
    Config config = 7;           // server -> client
    CallStart call_start = 8;
    BodyChunk body_chunk = 9;
    ResponseStart response_start = 10;
    CallComplete call_complete = 11;
    CallFailed call_failed = 12;
    BodyFetch body_fetch = 13;   // server -> client
    BodyFetchResult body_fetch_result = 14;
    Ping ping = 15;
    Pong pong = 16;
    Error error = 17;
  }
}

message Hello {
  uint32 protocol_version = 1;
  string sdk_name = 2;           // "android" | "ios"
  string sdk_version = 3;
  string app_id = 4;             // package name or bundle id
  string app_name = 5;
  string app_version = 6;
  string build = 7;
  string device_install_id = 8;  // random id kept in app-private storage
  string device_name = 9;
  string device_model = 10;
  string os_version = 11;
  int64 device_time_ms = 12;
  bytes token = 13;              // empty if not paired
}

message HelloAck {
  enum State { AUTHENTICATED = 0; PAIRING_REQUIRED = 1; DENIED = 2; INGEST_DISABLED = 3; }
  State state = 1;
  string desktop_name = 2;
  string session_id = 3;
  int64 server_time_ms = 4;
}

message CallStart {
  string call_id = 1;            // UUIDv7 made by the SDK
  int64 started_at_ms = 2;       // device clock, corrected by offset in the core
  string method = 3;
  string url = 4;
  repeated Header headers = 5;
  string http_version = 6;       // best effort
  string client_stack = 7;       // "okhttp" | "ktor" | "urlsession" | "alamofire"
  string route_template = 8;     // e.g. /cart/{id}/coupon, empty if unknown
  string name = 9;               // from @Inspect or inspect()
  string owner = 10;
  string tag = 11;
  string screen = 12;
  bool request_body_truncated = 13;
  uint64 request_body_size = 14; // 0 if unknown
}

message BodyChunk {
  string call_id = 1;
  enum Part { REQUEST = 0; RESPONSE = 1; }
  Part part = 2;
  uint32 seq = 3;
  bytes data = 4;
  Encoding encoding = 5;         // NONE | DEFLATE
  bool last = 6;
  bool truncated = 7;            // cap reached, no more bytes follow
}

message ResponseStart {
  string call_id = 1;
  uint32 status = 2;
  repeated Header headers = 3;
  string http_version = 4;
  uint64 response_body_size = 5; // 0 if unknown
}

message Timings {                // all in ms, -1 if unknown
  int32 dns = 1; int32 connect = 2; int32 tls = 3;
  int32 request_sent = 4; int32 ttfb = 5; int32 download = 6;
}

message CallComplete {
  string call_id = 1;
  int64 ended_at_ms = 2;
  Timings timings = 3;
  bool from_cache = 4;
  bool response_body_available = 5;  // false if skipped or not yet sent
  bool request_body_available = 6;
}

message CallFailed {
  string call_id = 1;
  string kind = 2;               // timeout | dns | tls | cancelled | io | other
  string message = 3;
  Timings timings = 4;
}

message Config {                 // desktop -> SDK, per device and app
  bool capture_enabled = 1;
  repeated string host_allow = 2;
  repeated string host_deny = 3;
  uint32 body_cap_bytes = 4;     // default 1 MiB
  uint32 eager_body_bytes = 5;   // default 64 KiB; larger bodies are sent on BodyFetch
  enum Redaction { STRICT = 0; DEFAULT = 1; NONE = 2; }
  Redaction redaction = 6;       // DEFAULT unless the user reveals values
  repeated string extra_redact_keys = 7;
}

message BodyFetch { string call_id = 1; BodyChunk.Part part = 2; }
```

Flow assembly rules in the core:
- A flow exists from `CallStart`. It shows live as "in progress".
- `ResponseStart` sets status and headers. `CallComplete` finalizes. `CallFailed` finalizes with an error.
- If the session drops mid-call, mark the flow `incomplete` after a grace period.
- Chunks are ordered by `seq` per call and part. Out-of-order or duplicate chunks are dropped and counted.
- The core never takes `device_id`, `app_id` or `source` from a payload. They come from the authenticated session.

## Session lifecycle

1. SDK connects to a candidate, completes TLS, sends `Hello` (with `token` if it has one for this server fingerprint).
2. Core replies `HelloAck`:
   - `AUTHENTICATED`: token is valid for this device and app. Core sends `Config`. Events may flow.
   - `PAIRING_REQUIRED`: continue with pairing below. No events are sent until it completes.
   - `DENIED`: the user denied or revoked. SDK backs off for a long interval.
   - `INGEST_DISABLED`: SDK backs off and retries on the normal schedule.
3. Keep-alive: `Ping` and `Pong` every 15 s of idleness, session dies after 60 s of silence.
4. Reconnect with backoff. The SDK keeps buffering into its ring buffer while disconnected, within the memory cap.

## Pairing

First connect from a device and app the core has not seen. The goal is that a person confirms two screens show the same code, which stops a rogue desktop or a man in the middle on the same network.

1. SDK sends `PairRequest` containing `commit = SHA-256(client_nonce)`. It does not reveal the nonce yet.
2. Core replies `PairChallenge` with `server_nonce` and shows the Allow prompt on the desktop (device name, model, app id).
3. SDK sends `PairReveal` with `client_nonce`. Core checks it matches the commit.
4. Both sides compute `code = first 6 decimal digits of SHA-256(server_cert_fingerprint || client_nonce || server_nonce)`, using the fingerprint each side actually saw.
5. Desktop displays the code. SDK logs it (Logcat tag `Inspector`, `os_log` on iOS) and shows a debug banner.
6. The person compares and clicks Allow. Core sends `PairResult` with a random 256-bit `token`. The core stores only its hash.
7. SDK stores the token and the server fingerprint in app-private storage. Android: a file in `noBackupFilesDir` or equivalent excluded from backup. iOS: Keychain, `AfterFirstUnlockThisDeviceOnly`.
8. From now on the SDK connects only to a server whose fingerprint matches the stored one.

If a man in the middle presents its own certificate, the two codes differ and the person denies. Committing to the client nonce first prevents an attacker from grinding a certificate to make codes match. This design must go through the security review in `08-performance-security.md` before release.

Rules:
- The core ignores repeat `PairRequest`s from the same install while one is pending. Pending prompts expire after 60 s. At most 5 pending at once.
- Revoking a device in the desktop app deletes its token hash and drops the session.
- The SDK never sends call data before `AUTHENTICATED`.

## Discovery

Candidate order tried by the SDK:
1. Last known good host for a paired server
2. Emulator and simulator: `10.0.2.2:8898` on Android, `localhost:8898` on iOS simulator
3. mDNS service `_nekoinspect._tcp`, advertised by the core while ingest is enabled. TXT records: `v=1`, `fp=<first 16 hex chars of the cert fingerprint>`, `name=<desktop name>`
4. Manual `host:port` from configuration (Gradle plugin extension on Android, `Inspector.start(host:)` on iOS)

When several desktops are discoverable (an office network):
- If the SDK has a stored pairing, it connects only to the matching fingerprint.
- If it has none and sees exactly one desktop, it pairs with that one.
- If it sees several, it does not pair automatically. It logs the list and shows a debug picker, or uses a configured `desktopName`.

USB:
- The core watches for `adb` devices while ingest is enabled and runs `adb -s <serial> reverse tcp:8898 tcp:8898` for each, so the device reaches the desktop at `localhost:8898` with no Wi-Fi involved. The Android SDK therefore also tries `localhost:8898`.
- iOS over USB is not supported in v1. The connection direction is reversed on iOS (the host connects to the device). Use the simulator or Wi-Fi.

| Setup | Same Wi-Fi needed | Mechanism |
|---|---|---|
| Android emulator | No | `10.0.2.2` |
| Android phone over USB | No | `adb reverse` |
| iOS simulator | No | `localhost` |
| Phone on same Wi-Fi | Yes | mDNS |
| Wi-Fi that blocks device-to-device traffic | Yes, fails | Manual host:port will also fail. Use USB or a different network |
| Remote device or cellular | No | Out of scope: needs a relay |

## Config push

The core sends `Config` after authentication and whenever the desktop changes it for that device and app: capture on or off, host allow and deny lists, body cap, eager body threshold, redaction level. Defaults: capture on, cap 1 MiB, eager 64 KiB, redaction `DEFAULT`.

Redaction levels:
- `STRICT`: headers except a short safe list are redacted, and bodies are not sent.
- `DEFAULT`: Authorization, Cookie, Set-Cookie, proxy-authorization, API-key-like headers, and JSON body keys such as `password`, `token`, `access_token`, `refresh_token`, `secret`, `otp`, `pin`, `cvv`, `card_number` are replaced with `[redacted]`. Extra keys come from `extra_redact_keys`.
- `NONE`: nothing is redacted. Chosen per device in the Devices screen, with a warning. Never the default.

Redaction happens in the SDK, before data leaves the device.

## Lazy body fetch

Bodies at or below `eager_body_bytes` are sent in `BodyChunk`s right away. Larger bodies are kept in the SDK ring buffer and announced by `CallComplete.*_available = false`. When the user opens such a flow, the core sends `BodyFetch` and the SDK replies with chunks. If the SDK no longer has the body (evicted or app restarted), it answers with `truncated = true` and no data, and the desktop shows "body no longer available".

## Merge with proxy traffic

When a device also uses the proxy, the same call arrives twice. The core merges them into one flow with two views: App (SDK context) and Wire (real timings, redirects, TLS).

- Match key: same device, method, normalized URL (scheme, host, path, sorted query), start time within 500 ms, and request body hash when both sides have one.
- Optional exact match: a debug-only `X-Inspect-Id` header added by the SDK. Off by default because it changes requests.
- Which device does a proxied connection belong to? Match by client IP when the device's session came from the same IP. If it cannot be determined, do not merge.
- A merged flow keeps both sources. Fields from the SDK (screen, name, tag, route template) win for context. Fields from the proxy (timings, TLS, redirects, wire headers) win for the wire view.
- Merging must be reversible: unmerged records remain stored, and the merged view is derived.
