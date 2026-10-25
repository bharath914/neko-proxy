# Protocol detail panes

Core BodyView.protocol returns render-ready rows for protobuf, gRPC and GraphQL.
Linux renders these in the body detail pane. Protobuf without schemas shows
field numbers and wire values (varint, fixed32/64, text, nested bytes); numeric
signedness, enum names and packed repeated types cannot be inferred. Nested
message interpretation is best effort, with hexadecimal fallback for bytes.
gRPC uncompressed frames are decoded independently; compressed frames and
gRPC-Web report/fall back rather than guessing. GraphQL documents and JSON
requests (including batches, operationName and variables) are recognized;
GraphQL responses remain in the existing JSON tree.

Bounds: 256 KiB input preview, 2000 rows, 4096 characters per string, 8 nested
levels, 100 GraphQL batch operations. Malformed and incomplete input reports a
note and preserves any successfully decoded rows. No content is executed.

Validation: cargo test --workspace passed; parser tests cover field numbers,
framing, malformed varints, bounded arbitrary bytes and GraphQL; API test
verifies stored gRPC becomes rows. NEKO_SHOT_DIR=/tmp/neko-protocol-shots
cargo test -p neko-desktop: 43 passed. Inspected grpc-fields.png.
No failing tests in the final runs. Performance targets not re-benchmarked.

Proto additions: BodyView.protocol = 17, ProtocolView and DecodedField.
Regenerate Swift with desktop-macos/gen-proto.sh. Existing fields unchanged.
