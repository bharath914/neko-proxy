# Neko Proxy

A fast, native, open-source HTTP/HTTPS debugging proxy with a built-in API client. Capture and inspect traffic, mock and break requests, compose and test your own, and let Claude help over MCP. Rust core, native Mac app.

## Install

**macOS 13+ (Apple silicon and Intel), from Terminal:**

```bash
curl -fsSL https://raw.githubusercontent.com/bharath914/neko-proxy/main/packaging/install-mac.sh | sh
```

It downloads the latest release, checks its checksum, and puts `Neko.app` in `/Applications` (or `~/Applications`). No Gatekeeper prompt. Remove it with `... | sh -s -- --uninstall`.

Prefer a disk image? Download `Neko-<ver>-macos-universal.dmg` from [Releases](https://github.com/bharath914/neko-proxy/releases). The app is not notarized yet, so open it once via System Settings > Privacy & Security > Open Anyway.

**Linux (x86_64, aarch64), into `~/.local`, no root:**

```bash
curl -fsSL https://raw.githubusercontent.com/bharath914/neko-proxy/main/packaging/install.sh | sh
```

`.deb`, `.rpm` and AppImage packages are on the Releases page too.

## Use it

1. Open Neko. In **Settings > Certificate**, install the certificate so HTTPS can be read.
2. Browse through the proxy (**Settings > Open a browser through the proxy**) or paste the snippet from **Terminal and tools**.
3. Requests appear in **Inspector**. Select one to replay, mock or break on it.
4. Open **Compose** to build requests like in Postman: collections, environments, tests, code snippets. The Composer is macOS only for now.

## Let Claude in (MCP)

```bash
nekod mcp    # prints the `claude mcp add ...` command
```

Claude can read traffic and propose mocks and breakpoints. Every change waits for your **Allow** in the app.

## Build from source

```bash
cargo build -p neko-cli && cd desktop-macos && swift run Neko    # macOS app
cargo run --release -p neko-desktop                               # Linux app
cargo test --workspace                                            # tests (macOS: also `swift test` in desktop-macos)
```

Layout: `core/crates/*` Rust core, `proto/api.proto` client API, `desktop-macos/` and `desktop/` apps, `sdk/` in-app SDK for Android and iOS, `docs/` design notes.

## Contributing

Found a bug, a rough edge, or something missing? **Raise an issue or send a PR.** Small fixes are as welcome as big features.

Good places to start: scripting in the Composer, digest and OAuth auth, a scenario-rule editor on macOS, more Linux testing. Run `cargo test --workspace` before sending a change, and read `CLAUDE.md` for the few rules new work must keep (the UI holds no logic, the data path never waits on the UI or disk, captured bodies are untrusted).

## More

[Features](docs/features.md) · [Composer](docs/composer-postman.md) · [MCP and rules](docs/phase-2.md) · [In-app SDK](docs/sdk-quickstart.md) · [Packaging and releases](docs/linux-packaging.md), [macOS signing](docs/macos-signing.md) · [Benchmarks](docs/bench.md)

## License

Either [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option. Unless you say otherwise, any contribution you submit is dual licensed the same way.
