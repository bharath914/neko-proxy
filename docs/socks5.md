# SOCKS5

The existing proxy listener (default 127.0.0.1:8899) accepts HTTP and SOCKS5.
Use `curl --socks5-hostname 127.0.0.1:8899 --cacert <data-dir>/ca.pem https://example.com`.
SOCKS5 CONNECT supports IPv4, IPv6 and domain targets, with no authentication.
UDP ASSOCIATE and BIND are rejected. Negotiation is capped at 15 seconds and
uses fixed buffers (255-byte maximum domain/method list).

After negotiation, traffic uses exactly the CONNECT interception path: TLS,
pinning detection, SSL policy, rules, capture caps and opaque tunnel fallback.
Like HTTP CONNECT, success acknowledges the proxy tunnel before upstream TLS
and connection setup; upstream failures close it and are recorded by the core.
Plain non-TLS streams are opaque tunnels, as with CONNECT.

Validation: `cargo test -p neko-net socks5`: 2 passed; TLS capture with IP and
hostname targets, unsupported authentication and UDP rejection.
`cargo test --workspace`: passed after fixing the Linux desktop integration
test's Linux-only socket path and honoring proxy use despite NO_PROXY.
Initial failure: `backend::e2e::supervises_core_and_reads_traffic`: `timed out
waiting for the core` (40 desktop tests passed, 1 failed).
No proto changes. Performance targets have not been re-benchmarked.

Shared-checkout note: the implementation was included by the concurrent
macOS commit 1ea37b3; the subsequent SOCKS5 commit carries tests and this note.
