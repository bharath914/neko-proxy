# SSE viewer

Responses labeled `text/event-stream` are captured under the existing per-flow
body cap. Stored response bytes are decoded in the core into events returned in
`BodyView.events`; Linux renders event names, IDs, data and retry intervals.
Raw bytes remain available in the ordinary text representation. No captured
markup is executed. The parser supports BOM, CR/LF/CRLF, comments, multiline
data, persistent IDs, empty data and digit-only retry values. Incomplete final
events are not dispatched. Preview limits: 512 KiB text and 20,000 events.

Live SSE snapshots are persisted at most twice a second while new bytes arrive.
Copying runs on a blocking worker and previews stop at 256 KiB per direction;
the final capture continues under the ordinary body cap. Persistence finishes
before the live update is announced, so the UI can fetch event data immediately.
A bounded preview never consumes the final captured body. Parsing is materialized
from stored bytes rather than duplicating event payloads in SQL. Linux offers
Events/Raw and Decoded/Raw controls for specialized views.

Validation: `cargo test --workspace` passed. `NEKO_SHOT_DIR=/tmp/neko-sse-shots
cargo test -p neko-desktop`: 42 passed. Inspected `sse-events.png`.
Parser tests cover framing and limits; API test verifies persisted event data.
The first screenshot fixture compile failed with E0433/E0609 (wrong import
and detail field); corrected before both successful runs.
Proto additions: SseEvent and BodyView.events = 16. Run
`desktop-macos/gen-proto.sh` for Swift bindings. No existing fields changed.

Follow-up integration validation: network test receives a live event while the
upstream connection is deliberately held open, then verifies the final body.
API test verifies a persisted snapshot stays inflight until final completion;
capture test verifies preview limits preserve final bytes. Internal Rust
FlowEvent::Snapshot is additive; no new proto changes for live snapshots.
The follow-up workspace run passed 284 tests, including 44 desktop render tests.
SSE rows are virtualized; full event data can be copied or opened in Raw mode.
SSE text is always interpreted as UTF-8, regardless of a misleading charset.
During timing-helper extraction one compile run failed with
`error: expected \`;\`, found \`Timings\``; corrected before the successful run.
