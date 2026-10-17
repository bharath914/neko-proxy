#!/bin/sh
# Builds the package in Release and fails if the SDK's real code or strings are in the result.
# Linux and macOS. On macOS also pass your app's built binary: Scripts/verify-release.sh path/to/App.app/App
set -eu
cd "$(dirname "$0")/.."
swift build -c release --product ProxyInspector >/dev/null
bad=0
check() { # file
  for marker in _proxyinspect inspector-pairing "Pairing code" "inspector-io" BodyRing DropOldestQueue CallHandle ProtoWriter; do
    if strings -a "$1" | grep -q -- "$marker"; then echo "FOUND \"$marker\" in $1"; bad=1; fi
  done
}
files=$(find .build/release/ -maxdepth 1 \( -name 'ProxyInspector.o' -o -name 'libProxyInspector.a' \) -type f)
[ -n "$files" ] || { echo "no build products found to check"; exit 2; }
for f in $files "$@"; do
  [ -f "$f" ] && check "$f"
done
if [ "$bad" = 1 ]; then echo "Release build contains Inspector code"; exit 1; fi
echo "Release build of ProxyInspector contains no capture code."
