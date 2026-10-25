# HAR import

`neko import-har capture.har [--socket /path/api.sock]` delegates to the core
CLI (`nekod import-har` is also available). A running daemon is required.
The additive API call is ImportHarRequest(data), returning imported IDs/count.
Imports share the live capture ID generator and publish finished-flow events.
Parsing runs off the async data path. Invalid input is rejected before writing.
Records are persisted in batches of 32 to avoid the pending-body retention cap.

Limits: 64 MiB input and aggregate decoded bodies, 10,000 entries, 8 MiB per
direction, 4096 headers. HTTP(S) URLs only. Base64 content is supported; timing
values are clamped. Content-encoding/length headers are removed because HAR
exports contain decoded bodies. Original cookies, headers and bodies are
untrusted local capture data; existing export/MCP redaction applies on output.
HAR cookies arrays, postData.params-only forms and vendor extensions beyond
_app are not reconstructed. Truncated bodies carry capture flags.

Validation: cargo test --workspace passed; targeted store import tests passed
(2), including binary export/import/store round trip and malformed documents.
CLI build and both `neko import-har --help` / `nekod import-har --help` passed.
Initial compile errors: missing FlowSink trait import and a missing demo match
arm; fixed. Targeted store test exposed E0599 for OffsetDateTime::parse; added
an explicit parsing feature rather than relying on workspace feature unification.

Proto additions: ClientMessage.import_har = 44, ServerMessage.imported_har = 28,
ImportHarRequest and ImportHarResponse. Regenerate Swift using
`desktop-macos/gen-proto.sh`. No existing fields changed.
