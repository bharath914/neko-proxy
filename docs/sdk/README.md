# In-app SDK: build plan for Claude Code

Plan for one feature only: a debug-time SDK (Android and iOS) that streams an app's network calls to the desktop inspector with no proxy, certificate or Wi-Fi proxy setup.

This folder assumes the Phase 1 proxy core and Linux desktop app already exist. If they do not, build Phase 1 first.

## Using this folder

Option A, inside the proxy repo (recommended):
1. Copy this folder to `docs/sdk/` in the repo.
2. Add this line to the repo's root `CLAUDE.md`: `In-app SDK work: follow docs/sdk/CLAUDE.md and docs/sdk/docs/10-tasks.md.`
3. Start Claude Code in the repo and say: "Start the SDK plan at milestone M0."

Option B, standalone: open this folder in Claude Code. Paths to the proxy repo are then described in `docs/02-core-assumptions.md`, and Claude Code will ask where the repo is.

## Reading order

1. `docs/01-overview.md` goals, scope, developer experience, milestones
2. `docs/02-core-assumptions.md` what the plan expects from the existing core
3. `docs/03-protocol.md` wire protocol, pairing, discovery
4. `docs/04-core-ingest.md` Rust side
5. `docs/05-desktop-app.md` Linux app changes
6. `docs/06-android-sdk.md`
7. `docs/07-ios-sdk.md`
8. `docs/08-performance-security.md`
9. `docs/09-testing-benchmarks.md`
10. `docs/10-tasks.md` the ordered task list with acceptance checks

`docs/reference/desktop-prototype.html` is the clickable Phase 1 UI prototype. Its Devices screen was drawn for proxy pairing and needs adapting (see `05-desktop-app.md`).

## Environment notes

- Milestones M0 (except spike S3), M1, M2, M3 and M5 run on Linux.
- Milestone M4 (iOS) and spike S3 need macOS with Xcode.
- Android emulator tests need an emulator or device. If none is available, JVM tests still run and the emulator tests are marked local-only in the README.
