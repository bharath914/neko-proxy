# Desktop app changes (Linux, Compose Multiplatform)

The app talks to the core only through the API. All logic that can live in Rust stays there. The app renders.

## Devices screen (new)

Adapt the Devices screen in `reference/desktop-prototype.html`. That prototype was drawn for proxy pairing with a QR code. Replace the QR flow with this one.

Layout, top to bottom:
1. Header row: "Accept SDK connections" switch, status line (port, fingerprint short form, "Listening on 192.168.1.24:8898"), adb status chip ("adb ready", "adb not found").
2. Pending requests (only when present), most prominent: device name and model, app id, and the 6-digit code in large monospace. Buttons Allow and Deny. A note: "Confirm the same code appears in the app's log or banner."
3. Paired devices table: device, app, SDK (android or ios and version), connection (USB, Wi-Fi, emulator), status (connected, idle, denied), last seen, flows count.
4. Row actions: Revoke, and a per-device settings drawer.

Per-device settings drawer:
- Capture on or off
- Host allow and deny lists
- Body cap and eager body size
- Redaction: Default, Strict, None. Choosing None shows a warning that tokens and passwords will appear in the inspector and in exports
- Extra keys to redact

Connection hints panel (collapsed by default): emulator, USB, Wi-Fi, and what to do if discovery fails. Include the exact manual `host:port`.

Empty state when nothing is paired: explain the three setups with a code snippet for adding the SDK, and keep the toggle visible.

## Inspector changes

- Source badge on each row: SDK, Proxy, Merged
- Device and app filter chips, populated from connected devices
- New optional columns: Name, Screen, Route template. Off by default except Name when present
- Overview tab shows SDK context: name, owner, tag, screen, app version, build, client stack, route template
- Request and Response tabs: lazy bodies show a loading state and then the body, or "body no longer available"
- Merged flows get an App and Wire toggle at the top of the detail pane. App shows SDK context and headers as the app saw them. Wire shows proxy-observed headers, TLS and timings
- Timings tab: use SDK timings when present. If both exist, show both labelled
- "Redacted" badge on values replaced by the SDK, with a link to the device's redaction setting
- Search matches name, tag, owner, screen and route template
- Copy as curl uses the final headers available. For redacted values it inserts a clear placeholder, not the string `[redacted]` silently

## State and structure

- `shared` module: add models for devices, pending prompts, SDK config. View-models subscribe to the devices stream and expose immutable state
- Keep lists paged as in Phase 1. SDK flows must not bypass paging
- No parsing of SDK messages in Kotlin. The app only sees API messages

## Acceptance

- Toggling ingest on and off updates status and starts or stops advertising within 1 s
- A pending prompt appears within 500 ms of the SDK's pair request and shows the same code the SDK logs
- Revoke disconnects the device and moves it to the denied or removed state immediately
- Changing a device's config reaches the SDK before its next request
- Large lazy bodies load on demand without freezing the UI
- 10,000 mixed SDK and proxy flows scroll smoothly and filter in under 100 ms
