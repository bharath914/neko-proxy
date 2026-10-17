# Compatibility, protocol versions and changelog

## Protocol version policy

- The wire protocol has an integer version (`Hello.protocol_version`). Today: 1.
- The desktop accepts `MIN_PROTOCOL_VERSION..=PROTOCOL_VERSION` (`core/crates/sdk/src/wire.rs`), so a newer desktop keeps serving older SDKs until the minimum is raised.
  The minimum is raised only for an incompatible change, and the release notes say so.
- An SDK outside the range gets `Error.UNSUPPORTED_VERSION` with the supported range and a human readable message, then the connection closes.
  The Android SDK logs it at ERROR level: "The desktop does not support this SDK's protocol version (N): ... It speaks A to B. Update the desktop app or the SDK."
  Tested in Rust (`unsupported_protocol_versions_get_a_clear_error`). The Kotlin log path is `Engine.logWireError`.
- Within one version, fields are only ever added. Unknown fields are skipped on both sides (`ProtoReader.skip`, prost).
- Field numbers are never reused or retyped (`proto/sdk.proto` header comment). The golden frames in `proto/sdk-fixtures.json` fail a test if the bytes change.

## Supported versions

| | Version | Verified how |
|---|---|---|
| Desktop and core | this repository | `cargo test --workspace` |
| Android SDK | 0.1.0, protocol 1 | JVM tests against `sdk-test-core`; real APK builds in `PluginTest`; the emulator run in `e2e.md` |
| iOS SDK | not built | |
| AGP | 9.4.1 (Gradle 9.8.1), 8.13.1 (Gradle 8.14) | `PluginTest` |
| Kotlin in the app | 2.0 to 2.4 | Built at language level 2.0; read by AGP 9's built-in Kotlin 2.2 in `PluginTest`'s projects (Java sources) and by Kotlin 2.4 in the sample |
| OkHttp | 5.5.0; 4.12 expected | 5.5.0 tested |
| Retrofit | 3.0.0 | tested |
| Ktor | 3.6.0 (CIO, OkHttp engines) | tested; uses internal APIs, re-test on upgrade |
| minSdk | 23 | build only |

## Changelog format

`CHANGELOG.md` per SDK, newest first, keep-a-changelog headings (Added, Changed, Fixed, Security). Each release line names the protocol version it speaks and the lowest desktop that accepts it. A protocol change is always a "Changed" entry at the top.

```
## 0.2.0 - 2026-11-02   (protocol 1, needs desktop 0.3 or newer)
### Added
- ...
```
