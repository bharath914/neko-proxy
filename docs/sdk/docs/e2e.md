# End-to-end results

Run on 2026-10-17 on the Linux development machine. The checklist is from `09-testing-benchmarks.md`.
**Items that need an Android emulator or device, an iOS Simulator or device, or a certificate-pinned app were not run.** They are listed as not run, with the reason.

## What was actually run

`sdk/scripts/e2e-jvm.sh`: a real `proxyd run --sdk` (the Linux core, its API and CLI), the real Android SDK engine (`sdk/android/core` and `okhttp`, on the JVM), OkHttp and Retrofit, and `sdk/test-server/server.py`.
Output saved in `e2e-jvm-output.txt`. The desktop GUI was not driven by hand; the same API it uses (`devices.subscribe`, `device_action`, `search`) was exercised by the CLI and by `core/crates/api/tests`, and its screens are covered by headless renders (`PROXY_SHOT_DIR=... cargo test -p proxy-desktop`).

| What | Result |
|---|---|
| Pairing: SDK code equals the code the desktop shows | **Pass.** Both `100288` in that run. |
| Allow through the CLI, then calls stream | **Pass.** 4 flows. |
| Retrofit `@Inspect` shows name, owner and route template | **Pass** in the JVM tests (`retrofitRouteTemplatesAndInspectMetadata`); the flow list above shows the call. |
| A call seen by the SDK and by the proxy shows once | **Pass.** Flow 5 is `source merged`; the proxy half is hidden from the list but stored. |
| 100 KB body stays on the device until opened | **Pass** (flow 3 has no body until fetched). |

## The checklist

| # | Item | Status |
|---|---|---|
| 1 | Android emulator, only the Gradle plugin, first request within 1 s | **Not run.** The Android emulator process crashes at startup on this host (SIGSEGV with `-gpu swiftshader_indirect`, `guest`, and `off`; with the API 37.1 image and an API 35 image). The plugin build and APK contents are verified instead (below). |
| 2 | Retrofit call with `@Inspect` shows name, owner, route template | Pass on the JVM (see above). Not on a device. |
| 3 | Ktor call including a streaming response | Pass on the JVM against CIO and OkHttp engines. Not on a device. |
| 4 | USB phone with `adb reverse`, Wi-Fi off | **Not run** (no phone). The `adb reverse` logic is tested with a fake `adb` (`reverses_ready_devices_and_runs_nothing_else`); the Linux core reported `adb ready` with the real `adb`. |
| 5 | Phone on Wi-Fi found through mDNS; codes match; revoke forces re-pair | Partly. mDNS advertisement is verified with a real browse in the same host (`the_service_is_advertised...`); codes match (above); revoke tested (`revoke_drops_the_session_and_forces_a_new_pairing`). No phone. |
| 6 | iOS Simulator, URLSession and Alamofire | **Not run.** No Mac. See `sdk/ios/README.md`. |
| 7 | iOS device on Wi-Fi with the Local Network prompt | **Not run.** |
| 8 | A certificate-pinned app shows full flows | **Not run.** In principle yes, since the SDK sits above TLS; no pinned app was available. |
| 9 | Device using both SDK and proxy shows one merged flow | Pass (above), on loopback with one device. |
| 10 | Release builds contain no SDK code | Android: pass, on real APKs (`PluginTest`, AGP 9.4.1 and 8.13.1; `verifyNoInspectorInRelease`). The sample app's release APK also builds with R8. iOS: the Release stubs pass `Scripts/verify-release.sh` on Linux; the app binary on a Mac was not checked. |

## What else was verified without a device

- APK contents: debug has the real SDK and the `inspector.properties` asset; release has neither (`PluginTest`).
- `OkHttpClient.<init>(Builder)` calls `InspectorOkHttp.install` first, once, in the built APK (`dexdump`, `PluginTest`).
- The sample app (`sdk/android/sample`) builds against the locally published artifacts. It was not installed on a device.

## To finish on a machine with devices

1. `cargo run --release -p proxy-desktop`, turn on **Accept SDK connections** on the Devices screen.
2. Android: `./gradlew publishAllToLocalRepo`, then `./gradlew -p sample :app:installDebug` on an emulator or a USB phone, with `python3 sdk/test-server/server.py` running (the sample's default base URL is `http://10.0.2.2:18080/`; pass the intent extra `base` otherwise). Walk checklist items 1 to 5 and 8.
3. iOS: follow "First things to do on a Mac" in `sdk/ios/README.md`.
