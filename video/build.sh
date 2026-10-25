#!/usr/bin/env bash
# Builds the montage video (opener + cuts of concept.html and hook.html) with subtle synthesized effects.
# Needs: node, ffmpeg, swiftc, and `npm install && npx playwright install chromium` once.
# Usage: ./build.sh [out.mp4] [crf]
set -euo pipefail
cd "$(dirname "$0")"
OUT=${1:-neko-proxy.mp4}; CRF=${2:-17}
{ sed -n '1,/^func saw/p' synth.swift | sed 's/^let dur = .*/let dur = 23.7/'; cat montage_fx.swift; sed -n '/^\/\/ ---------- Master/,$p' synth.swift | sed 's/0\.89/0.3/g'; } > montage_synth.swift
swiftc -O montage_synth.swift -o montage_synth && ./montage_synth montage.wav
node montage.mjs montage.wav "$OUT" "$CRF"
echo "wrote $OUT"
