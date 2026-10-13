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

## Install

From a release (Linux x86_64 and aarch64):

```bash
sudo apt install ./proxy_<ver>_amd64.deb        # Debian, Ubuntu
sudo dnf install ./proxy-<ver>-1.x86_64.rpm     # Fedora, RHEL, openSUSE
chmod +x Proxy-<ver>-x86_64.AppImage            # any distro, needs libfuse2 or --appimage-extract-and-run
sh packaging/install.sh                         # any distro, into ~/.local, no root; --uninstall removes it
```

Every release lists `SHA256SUMS`; `install.sh` verifies the tarball against it.

## Package and release

```bash
packaging/build.sh      # dist/: .tar.gz, .deb, .rpm (needs rpmbuild), .AppImage (needs APPIMAGETOOL=...), SHA256SUMS
```

Releases are built by `.github/workflows/release.yml` on Ubuntu 22.04 (glibc 2.35) for both architectures.
Push a tag matching the `Cargo.toml` version (`git tag v0.1.0 && git push origin v0.1.0`) and the workflow opens a
**draft** release; nothing is public until you publish it. Run the workflow by hand to get the packages as artifacts only.

## Develop

```bash
cargo test --workspace                                   # core, API, app model, headless UI renders
PROXY_SHOT_DIR=/tmp/shots cargo test -p proxy-desktop    # writes a PNG of every screen
cargo +nightly fuzz run --fuzz-dir core/fuzz http_parse  # needs: cargo install cargo-fuzz
```

Layout: `core/crates/*` Rust core, `proto/api.proto` client API, `desktop/` app, `docs/` design and benchmarks.
Design notes: `docs/desktop-design.md`.
