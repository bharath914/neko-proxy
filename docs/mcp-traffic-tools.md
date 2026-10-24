# MCP traffic, SDK and catalog tools

Read-only tools added:
- export_traffic(kind, ids/query): redacted artifact inline, no file writes.
- list_issues / scan_sensitive_data: stored core flags, no captured secret values.
- route_catalog(query): SDK route templates, or numeric/UUID inferred paths,
  grouped by origin/method; count, statuses, errors and mean latency.
- sdk_status / list_sdk_devices: listener/settings/counters and device/app facts.
- list_sdk_flows / get_sdk_flow: redacted SDK/merged flows and app context.

Writes added: save_export(kind, ids/query), fetch_sdk_body(id, part), and
set_sdk_enabled(enabled). Each uses the existing Ask/Allow/Read-only policy;
Ask waits for the app before performing the operation. The old export_har
also now needs approval because it creates a file. Saved files use exclusive
creation and mode 0600 inside the data directory's exports folder. Inline
artifacts are reads. create_scenario was added in task 5 through the same gate.

No pairing codes, credentials, device tokens or raw lazy-body bytes are exposed.
Fetched bodies go through the ordinary redacted get_flow representation.
Captured content is labeled as untrusted. Reads use a final structured redaction
pass, including header tuples/objects, embedded JSON, encoded query keys and
short credentials in human-readable rule summaries. Token-shaped path segments
are scrubbed. Error text is scrubbed too.

Bounds: 1 MiB tool result, 10k export/catalog source flows, 32 MiB source bodies,
500 catalog routes, 200 flow rows/page, 1000 device rows. Narrow the query when
limits are reached. Catalog statistics are an observed sample, not a definitive
service contract. Numeric/UUID route inference is heuristic. No proto changes.

Validation: cargo test --workspace passed, including real (disabled) SDK
instances, source context/catalog reads, artifact redaction, file permissions,
app approval and read-only rejection. Existing authentication/origin tests pass.
Initial compile failed E0425 after export handler replacement accidentally
removed helper functions; restored them. A redaction regression then failed:
`rule_reads_redact_short_credentials_and_bad_arguments_fail`;
`test result: FAILED. 10 passed; 1 failed`.
The emitted summary contained `api.test/*?%61pi_key=short-query` (fixture data).
Summary redaction was corrected; the final full run passes. No performance
re-benchmark or new desktop UI changes for this task.

Final body-redaction tests also cover truncated JSON, SSE JSON payloads,
plaintext Cookie headers and cookie-array values. Export loading budgets include
metadata, and decompression is bounded before creating any artifact. No further
proto changes.
