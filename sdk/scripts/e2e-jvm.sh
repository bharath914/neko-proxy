#!/usr/bin/env bash
# The real Android SDK engine (JVM) against a real `nekod run --sdk`: pairing through the CLI, calls through OkHttp and
# Retrofit, one call also through the proxy so the core can merge the two views. Writes the result to $OUT (default /tmp/e2e-jvm.txt).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${OUT:-/tmp/e2e-jvm.txt}"
T="$(mktemp -d)"
export NEKO_DATA_DIR="$T/data" NEKO_SOCKET="$T/api.sock"
NEKOD="$ROOT/target/debug/nekod"
python3 "$ROOT/sdk/test-server/server.py" --port 18080 >/dev/null 2>&1 & SERVER=$!
"$NEKOD" run --listen 127.0.0.1:18899 --no-mcp --sdk --sdk-port 18897 --key-storage file >"$T/nekod.log" 2>&1 & CORE=$!
trap 'kill $SERVER $CORE 2>/dev/null || true' EXIT
sleep 2
cd "$ROOT/sdk/android"
./gradlew :okhttp:e2e -Pargs="18897 http://127.0.0.1:18080/ 18899" --console=plain >"$T/sdk.log" 2>&1 & SDK=$!
# Wait for the pairing code, compare it with the one the desktop shows, allow it.
for _ in $(seq 1 60); do grep -q PAIRING_CODE "$T/sdk.log" && break; sleep 1; done
SDK_CODE="$(grep PAIRING_CODE "$T/sdk.log" | head -1 | sed 's/PAIRING_CODE //' | tr -d ' ')"
for _ in $(seq 1 20); do
  PENDING="$("$NEKOD" devices list --json)"
  echo "$PENDING" | grep -q '"code"' && break; sleep 0.5
done
DESKTOP_CODE="$(echo "$PENDING" | python3 -c 'import json,sys; print(json.load(sys.stdin)["pending"][0]["code"])')"
ID="$(echo "$PENDING" | python3 -c 'import json,sys; print(json.load(sys.stdin)["pending"][0]["id"])')"
{
  echo "sdk code:     $SDK_CODE"
  echo "desktop code: $DESKTOP_CODE"
  [ "$SDK_CODE" = "$DESKTOP_CODE" ] && echo "codes match: yes" || { echo "codes match: NO"; exit 1; }
} | tee "$OUT"
"$NEKOD" devices allow "$ID" | tee -a "$OUT"
wait $SDK || { tail -20 "$T/sdk.log"; exit 1; }
sleep 1
{
  echo "--- sdk flows"; "$NEKOD" flows --source sdk --limit 50
  echo "--- merged flows"; "$NEKOD" flows --source merged --limit 50
  echo "--- all flows (SDK + proxy halves hidden once merged)"; "$NEKOD" flows --limit 50
  echo "--- devices"; "$NEKOD" devices list
  echo "--- sdk status"; "$NEKOD" sdk status
} | tee -a "$OUT"
