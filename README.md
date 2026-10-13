# Proxy

Fast, simple, open-source HTTP/HTTPS debugging proxy for Linux. A Rust capture core (`proxyd`) and a native desktop app.

## Run it

```bash
# once: Rust (https://rustup.rs). Nothing else to install.
cargo run --release -p proxy-desktop        # builds the core and the app, starts both
```

The app starts `proxyd` for you and stops it on exit. Then:

1. **Settings > Certificate**: run the install command shown for your distro (it asks before running anything), so HTTPS can be decrypted.
2. **Settings > Open a browser through the proxy**, or paste the snippet from **Terminal and tools**.
3. Traffic appears in **Inspector**.

Try the interface without a core: `cargo run --release -p proxy-desktop -- --demo --live`.

## Headless

```bash
cargo build --release -p proxy-cli
target/release/proxyd run                   # proxy on 127.0.0.1:8899, API on $XDG_RUNTIME_DIR/proxy/api.sock
target/release/proxyd flows                 # list captures (also: flows 'status:5xx host:example.com')
target/release/proxyd export --har -o out.har
target/release/proxyd ca path               # certificate to trust
curl -x http://127.0.0.1:8899 --cacert "$(target/release/proxyd ca path)" https://example.com
```

## Package

```bash
packaging/build.sh      # dist/proxy-<version>-linux-<arch>.tar.gz and dist/proxy_<version>_<arch>.deb
```

## Develop

```bash
cargo test --workspace                                   # core, API, app model, headless UI renders
PROXY_SHOT_DIR=/tmp/shots cargo test -p proxy-desktop    # writes a PNG of every screen
cargo +nightly fuzz run --fuzz-dir core/fuzz http_parse  # needs: cargo install cargo-fuzz
```

Layout: `core/crates/*` Rust core, `proto/api.proto` client API, `desktop/` app, `docs/` design and benchmarks.
Design notes: `docs/desktop-design.md`.
