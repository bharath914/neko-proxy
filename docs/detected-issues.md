# Detected issues and sensitive data

The storage worker computes flags, persists them, and publishes analyzed
completion updates. Capture never waits for scanning. Linux shows an indicator
with a tooltip in the flow table and an issue list in Overview. Selected details
refresh when flags change. Deferred analysis cannot resurrect a merged wire row.

Heuristics (newly captured/imported flows, not a migration of old history):
- Slow: total duration >= 1 second.
- Large: either direction >= 1 MiB.
- Missing cache: GET 200 without Cache-Control, Expires, ETag or Last-Modified;
  SSE is excluded.
- Duplicate: same method/authority/path/query/request-body prefix within 2 s.
- Possible N+1: five distinct GETs in a normalized numeric/UUID route within 2 s.
- Sensitive: credential headers/query keys, secret JSON keys, token-shaped
  values, email addresses and Luhn-valid payment-card-looking numbers.

Search: is:issues, is:sensitive, is:duplicate, is:large, is:nplus1, is:uncached.
Flags use bits 14–19. Proto adds FlowSummary.issue_labels = 28 and
FlowDetail.issues = 31. Run desktop-macos/gen-proto.sh to regenerate Swift.

Correlation history is limited to 4096 entries, with hashed lookup keys;
body scanning and request fingerprints use only the first 64 KiB per direction.
JSON scanning stops at depth 16 / 2000 values. Findings expose labels, never
captured values. Credentials can be legitimate; these are review hints, not
proof of a leak. Partial/encrypted/binary bodies and unsampled data can be missed.
Correlation starts fresh after restart/clear and flags only later matching calls.

Validation: cargo test --workspace passed, including scanner/heuristic,
search/persistence/notification and merged-row regression tests. Screenshot
run passed 44 desktop tests; inspected detected-issues.png. No test failures.
Release store benchmark: 100k flows in 2.3 s; first page 8.4 ms, errors 10.0 ms,
slow 9.2 ms, oauth text 21.3 ms, body text 35.7 ms, combined filter 10.3 ms,
host substring 23.8 ms. All searches below the 100 ms target on this host.
Cold start, proxy latency, memory and end-to-end capture throughput were not
re-benchmarked. Command: cargo run --release -p neko-store --example bench_search.
