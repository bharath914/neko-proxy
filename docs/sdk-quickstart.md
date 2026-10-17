# SDK quick start

See your app's network calls in the desktop app. No proxy, no certificate, no Wi-Fi settings. Debug builds only.

## Desktop (once)

1. `cargo run --release -p proxy-desktop`, or run `proxyd run --sdk` and use the CLI.
2. Open **Devices**, turn on **Accept SDK connections**. (The listener is off every time the app starts, on purpose.)

## Android (one line)

```kotlin
// app/build.gradle.kts
plugins {
    id("com.android.application")
    id("dev.proxy.inspector")      // placeholder id
}
```

Run the app on an emulator, a USB phone (needs `adb` on this computer) or a phone on the same Wi-Fi.
The Devices screen shows a pairing request with a six-digit code. The same code is in Logcat (`Inspector`) and in a toast.
If they match, press **Allow**. Calls appear in the Inspector with an `SDK` badge, the screen, and (for Retrofit) the route template.

Name calls if you like: `@Inspect(name = "Apply coupon", owner = "payments")` on a Retrofit method, or `inspect(name = ...)` on a Ktor request.

Problems: `./gradlew inspectorDoctor`, then `sdk/android/README.md`.

## iOS

Not built yet. The plan is `Inspector.start()`, see `docs/sdk/docs/07-ios-sdk.md`.

## Headless

```
proxyd run --sdk
proxyd devices list
proxyd devices allow <id>      # after checking the code matches the app's log
proxyd flows --source sdk
```

## Try it without an app

`cargo run -p sdk --bin fake-device -- --calls 100` pairs a pretend phone and streams varied calls.
