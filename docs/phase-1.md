# Phase 1: capture core and Linux app

Target: Linux only (x86_64 and aarch64, glibc distros such as Ubuntu 22.04+, Fedora, Arch). No mobile, macOS or Windows.

## Ships

### 1. `proxyd` core (Rust)
- HTTP/1.1 and HTTP/2 forward proxy with CONNECT
- HTTPS interception with a per-install CA and cached leaf certs
- Pinning detection: when the client rejects the minted cert (unknown_ca or bad_certificate alert), tunnel the connection from then on for that host and flag the flow as pinned
- WebSocket upgrades pass through (frame capture is deferred)
- Store: SQLite in WAL mode, zstd blobs for bodies, full-text index
- API over a Unix domain socket, WebSocket + protobuf: `subscribe(filter)`, `get_flow(id, parts)`, `search(query, range)`, `capture.start/stop/status`, `export(har, ids)`, `clear`
- CLI: `proxyd run`, `proxyd flows`, `proxyd export --har`, `proxyd ca path|print`

### 2. Linux desktop app (native Rust, egui)
- Inspector screen from `docs/desktop-prototype.html`: filter bar with All / Errors / Slow chips, virtualized flow table, detail pane with Overview, Request, Response and Timing tabs, JSON viewer, copy as curl, record/pause, clear
- Settings screen: CA install helper, launch-browser-through-proxy
- Starts and supervises `proxyd` as a child process and talks to it over the socket
- Only the Inspector and Settings screens are built. Rules, Breakpoints and Devices in the prototype are out of scope

### 3. Packaging
- Portable tarball and a `.deb`, built by `packaging/build.sh`

## Not in Phase 1

Rules (mock, map, rewrite, throttle), breakpoints, replay, diff, MCP server, phone pairing and companion apps, macOS, Windows, HTTP/3, SOCKS5, transparent mode, WebSocket / SSE / gRPC viewers, scripting, AI features.

Keep the seams so these slot in later without touching the data path:
- The `flow` pipeline must expose request and response interceptor hooks, with an empty chain for now
- The API is the only way the app talks to the core
- Do not create the `rules` or `mcp` crates yet

## Linux specifics

- Listen on `127.0.0.1:8899` by default. API socket at `$XDG_RUNTIME_DIR/proxy/api.sock`, mode 0600. Data in `$XDG_DATA_HOME/proxy` (default `~/.local/share/proxy`)
- CA: `ca.pem` and `ca.key` in the data dir. Store the key in libsecret when available, otherwise a file with mode 0600
- Trust helpers. Never run sudo silently: show the command, let the user confirm, then run it
  - Debian/Ubuntu: `sudo cp ca.pem /usr/local/share/ca-certificates/proxy-ca.crt && sudo update-ca-certificates`
  - Fedora: `sudo cp ca.pem /etc/pki/ca-trust/source/anchors/proxy-ca.pem && sudo update-ca-trust`
  - Arch: `sudo trust anchor --store ca.pem`
  - Chrome, Chromium, Edge (NSS): `certutil -d sql:$HOME/.pki/nssdb -A -t "C,," -n proxy-ca -i ca.pem`
  - Firefox: per-profile NSS database, or enable `security.enterprise_roots.enabled`
- Launch helpers
  - Chromium family: `--proxy-server=127.0.0.1:8899` with a throwaway `--user-data-dir`
  - Firefox: temporary profile with proxy prefs set
  - Terminal: copyable env snippet with `HTTP_PROXY`, `HTTPS_PROXY`, `SSL_CERT_FILE`, `CURL_CA_BUNDLE`, `NODE_EXTRA_CA_CERTS`
- Do not change system proxy settings automatically
- Worth investigating: Chromium's `--ignore-certificate-errors-spki-list` as a zero-install trust path for the launch helper. Verify it works with the minted chain before relying on it

## Repo to create now

```
proxy/
├─ core/crates/{net,tls,flow,store,decode,api,cli}
├─ proto/api.proto
├─ desktop/                       Rust, egui
└─ docs/
```

## Task list (in order)

1. Scaffold the Cargo workspace, `proto/api.proto`, and the `desktop` crate
2. Define the API messages in `api.proto` and generate Rust code
3. `tls`: CA generation, key storage, leaf cert minting and cache
4. `net` + `flow`: HTTP/1.1 proxy with CONNECT and HTTPS interception, flow model, empty interceptor chain
5. HTTP/2 support and timing capture (DNS, connect, TLS, TTFB, download)
6. Pinning detection and tunnel fallback
7. `store`: SQLite schema, blob store, FTS index, async writes off the data path
8. `decode`: gzip, br, zstd, JSON, form bodies, image detection
9. `api`: Unix socket WebSocket server, subscriptions with 16 ms batching, the calls listed above
10. `cli`: `run`, `flows`, `export --har`, `ca`
11. App shell: start and supervise `proxyd`, API client, view state
12. Flow table (virtualized, paged), filter bar and chips, search
13. Detail pane: tabs, JSON viewer, timing waterfall, copy as curl
14. Settings: CA install helper, browser launch helpers, env snippet
15. Benchmarks and packaging: write results to `docs/bench.md`, build tarball and `.deb`

## Acceptance

- `curl -x http://127.0.0.1:8899 --cacert ca.pem https://example.com` appears in the app within 100 ms
- After running the CA helper, Chromium launched through the helper browses a real HTTPS site with no certificate errors and flows appear
- HTTP/2 flows are captured with correct timings
- With 10,000 flows loaded, scrolling stays smooth and a filter returns in under 100 ms
- HAR export imports cleanly into Chrome DevTools
- Connections from clients that reject the minted cert are tunneled and flagged as pinned
- `cargo test` passes, and there is a fuzz target for the HTTP parsing path
- `docs/bench.md` records results against these targets: core cold start under 50 ms, proxy overhead under 2 ms per request, 5,000 req/s sustained, core idle under 40 MB, app first paint under 1 s, app idle under 250 MB
