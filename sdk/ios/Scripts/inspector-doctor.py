#!/usr/bin/env python3
"""Checks a debug app's Info.plist for what the Inspector needs on a physical device, and prints the fix.

    python3 Scripts/inspector-doctor.py path/to/Info.plist

The keys are only needed in Debug. Put them in a debug-only plist or xcconfig (see README) so Release does not carry them.
"""
import plistlib, sys

KEY_USAGE = "NSLocalNetworkUsageDescription"
KEY_SERVICES = "NSBonjourServices"
SERVICE = "_proxyinspect._tcp"

def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__); return 2
    with open(sys.argv[1], "rb") as f:
        plist = plistlib.load(f)
    problems = []
    if not str(plist.get(KEY_USAGE, "")).strip():
        problems.append(f"missing {KEY_USAGE}: add a short reason, for example <string>Connects to the Proxy desktop app to show this app's network calls.</string>")
    if SERVICE not in (plist.get(KEY_SERVICES) or []):
        problems.append(f"{KEY_SERVICES} does not contain {SERVICE}: add <array><string>{SERVICE}</string></array>")
    if not problems:
        print("Info.plist has what the Inspector needs for Wi-Fi discovery.")
        return 0
    print("Info.plist needs changes for a physical device on Wi-Fi (the Simulator does not need them):")
    for p in problems:
        print("  -", p)
    return 1

if __name__ == "__main__":
    sys.exit(main())
