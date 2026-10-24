#!/usr/bin/env bash
# Linux release packages. See docs/linux-packaging.md for formats and prerequisites.
set -euo pipefail
exec python3 "$(dirname "$0")/linux/build.py" "$@"
