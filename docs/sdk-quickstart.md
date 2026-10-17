# SDK quick start

See your app's network calls in the desktop app. No proxy, no certificate, no Wi-Fi settings. Debug builds only.

## Desktop

Start the app (`cargo run --release -p proxy-desktop`) or `proxyd run`. SDK connections are on from the start, and the choice is remembered.

The phone icon in the top bar says whether an app can connect right now: green ready, amber needs you, red not working, grey off. Click it for the checks and the fix for each one that fails (port in use, USB debugging not approved on the phone, `adb` missing or without udev permission, firewall blocking Wi-Fi phones). `proxyd sdk status` prints the same.

## Android (one line)

```kotlin
// app/build.gradle.kts
plugins {
    id("com.android.application")
    id("dev.proxy.inspector")      // placeholder id
}
```

Run the app on an emulator, a USB phone (needs `adb` on this computer) or a phone on the same Wi-Fi.
An emulator, simulator or USB phone connects on its own: a toast says so and the calls appear in the Inspector with an `SDK` badge, the screen, and (for Retrofit) the route template. This is the **Accept emulators, simulators and USB phones automatically** switch on Devices, on by default.
A phone on Wi-Fi still asks. The Devices screen shows a pairing request with a six-digit code, the same code is in Logcat (`Inspector`) and in a toast, and you press **Allow** if they match. The **Phones on Wi-Fi** switch turns the network side off (this computer only).

Name calls if you like: `@Inspect(name = "Apply coupon", owner = "payments")` on a Retrofit method, or `inspect(name = ...)` on a Ktor request.

Problems: `./gradlew inspectorDoctor`, then `sdk/android/README.md`.

## iOS

Not built yet. The plan is `Inspector.start()`, see `docs/sdk/docs/07-ios-sdk.md`.

## Headless

```
proxyd run                      # SDK connections are on by default; --sdk forces them on for one run
proxyd sdk set lan off          # this computer only (no Wi-Fi phones)
proxyd sdk set auto-accept-local off   # every device asks
proxyd sdk status               # health and the fix for anything wrong
proxyd devices list
proxyd devices allow <id>      # after checking the code matches the app's log
proxyd flows --source sdk
```

## Try it without an app

`cargo run -p sdk --bin fake-device -- --calls 100` pairs a pretend phone and streams varied calls.
