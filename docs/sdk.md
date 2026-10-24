# Mobile SDK (early design notes; superseded)

> Built differently from this draft: see `docs/sdk/` (the plan), `docs/sdk-quickstart.md`, and `docs/sdk/docs/DEVIATIONS.md`. The SDK talks to its own TLS ingest port (8897) and pairs with a six-digit code instead of routing traffic through the proxy. This page is kept for the reasoning only.


Replaces the Android and iOS companion apps. Instead of a separate app the person pairs and installs, the developer adds a small debug-only library to the app they are already building. Draft: the shape below is the plan to push back on before any code is written.

## Why an SDK instead of apps

- The pain in phone debugging is per app: Android apps ignore user CAs, iOS needs a profile plus a trust toggle, pinned apps refuse the CA. An SDK inside the app can fix all three for that app without touching device settings.
- No store listing, no second UI to build. The inspector stays on the desktop.
- Works on a simulator, an emulator or a real device with the same code.

## What it does

| Job | Android (Kotlin) | iOS (Swift) |
|---|---|---|
| Find the proxy | pair by QR or `host:port:token`, or LAN discovery | same |
| Route this app's traffic | OkHttp `ProxySelector`, `HttpURLConnection` default | `URLSessionConfiguration` proxy dictionary |
| Trust the CA, debug builds only | runtime trust manager seeded with the CA, no `network_security_config` edit | server trust handling for the CA, no profile install |
| Verify the CA | compare against `ca_sha256` from the pairing payload | same |
| Pinning | optional debug override so pinned calls are readable | same |
| Tag flows | app id, build, device name, session label | same |

Release builds get a no-op artifact: the dependency can ship in the manifest and do nothing.

## Core changes it needs

- Pairing payload `{ host, port, token, ca_sha256 }` from `pair.begin()` (already in the design), plus LAN binding while pairing.
- The cert page (`http://neko.local`) already serves the CA as PEM, DER and an Apple profile, and answers `/status.json` and an HTTPS `/trust-check`. The SDK can reuse both for fetching the CA and confirming trust.
- A way to attach tags to flows. Open: a request header the core strips (`X-Neko-Tag`), or a small side channel on the client API.

## Order

1. Android SDK (Kotlin, OkHttp first).
2. iOS SDK (Swift, URLSession).
3. Later: Flutter and React Native wrappers, a JVM artifact, on-device capture.

## Open questions

- Package names and distribution (Maven Central, Swift Package Manager).
- Tagging mechanism, above.
- Whether the SDK ever shows UI (a shake menu) or stays headless.
