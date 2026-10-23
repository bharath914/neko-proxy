# Phase 1 benchmarks

Machine: Fedora 44, 12 cores, 23 GB RAM, NVIDIA GTX 1650 (GL), Wayland. Release builds.
Reproduce: `cargo run --release -p neko-net --example bench`, `cargo run --release -p neko-store --example bench_search`,
`NEKO_BENCH=1 target/release/neko --demo` (prints the first-frame time).

| Target | Goal | Measured | Notes |
|---|---|---|---|
| Core cold start | < 50 ms | 18 ms (key in file), 29 to 49 ms (key fetched from the keyring) | process start to listeners bound; the keyring path shells out to `secret-tool` |
| Proxy overhead per request | < 2 ms | 0.035 ms p50, 0.045 ms p99 | 1 KB body, keep-alive, plain HTTP, origin and proxy in one process |
| Sustained throughput | 5,000 req/s | 60,592 req/s | 64 connections for 5 s. The store ingests about 33,000 flows/s, so recording keeps up |
| Core idle memory | < 40 MB | 12.8 MB RSS (14.3 MB after 200 requests) | |
| Search over 100k flows | < 100 ms | 0.3 to 64 ms | slowest is a body-text search; chips and operators 12 to 16 ms |
| App first paint | < 1 s | 189 to 227 ms | process launch to first frame submitted |
| App idle memory | < 250 MB | 108 to 165 MB RSS | mostly the GPU driver's share; varies per run |
| Binary sizes | | `nekod` 9.9 MB, `proxy` 11.6 MB | |

## Search at 100,000 flows

| query | matches | time |
|---|---|---|
| all, first page | 100000 | 0.3 ms |
| errors chip | 28571 | 15.9 ms |
| slow chip | 51000 | 12.5 ms |
| text: oauth | 16667 | 38.7 ms |
| text: user-42 (body) | 45 | 63.5 ms |
| method:post status:5xx | 3571 | 14.3 ms |
| host substring: example.org | 20000 | 44.7 ms |

## Not measured here

- Scroll smoothness with 10,000 flows on a real display. The table draws only visible rows and pages data on demand, but frame pacing was not captured.
- HTTPS throughput. The numbers above are plain HTTP.
- The proxy overhead figure shares one machine and process with the origin and the load generator, so it is a lower bound on a real network.
