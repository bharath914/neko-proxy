#!/bin/sh
# Regenerate Sources/NekoAPI/api.pb.swift from proto/api.proto.
# Needs protoc (the vendored one from the cargo registry works) and the
# protoc-gen-swift plugin from swift-protobuf.
set -eu
cd "$(dirname "$0")"
PROTOC="${PROTOC:-$(ls ~/.cargo/registry/src/*/protoc-bin-vendored-macos-aarch_64-*/bin/protoc | head -1)}"
swift build --product protoc-gen-swift -c release
"$PROTOC" --plugin=protoc-gen-swift=.build/release/protoc-gen-swift \
  --swift_opt=Visibility=Public \
  --swift_out=Sources/NekoAPI -I ../proto ../proto/api.proto
