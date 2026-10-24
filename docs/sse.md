# SSE viewer

Responses labeled `text/event-stream` are captured under the existing per-flow
body cap. Stored response bytes are decoded in the core into events returned in
`BodyView.events`; Linux renders event names, IDs, data and retry intervals.
Raw bytes remain available in the ordinary text representation. No captured
markup is executed. The parser supports BOM, CR/LF/CRLF, comments, multiline
data, persistent IDs, empty data and digit-only retry values. Incomplete final
events are not dispatched. Preview limits: 512 KiB text and 20,000 events.

This uses the existing completed/disconnected-flow persistence lifecycle;
ongoing SSE responses do not yet publish live event snapshots. Parsing is
materialized from stored bytes, rather than duplicating event payloads in SQL.

Validation: `cargo test --workspace` passed. `NEKO_SHOT_DIR=/tmp/neko-sse-shots
cargo test -p neko-desktop`: 42 passed. Inspected `sse-events.png`.
Parser tests cover framing and limits; API test verifies persisted event data.
The first screenshot fixture compile failed with E0433/E0609 (wrong import
and detail field); corrected before both successful runs.
Proto additions: SseEvent and BodyView.events = 16. Run
`desktop-macos/gen-proto.sh` for Swift bindings. No existing fields changed.
