//! Search latency over 100k stored flows.
//! `cargo run --release -p proxy-store --example bench_search`

use bytes::Bytes;
use flow::{FlowRecord, FlowState, Timings};
use proxy_store::{Kind, Query, Store};
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let hosts = [
        "api.example.com",
        "cdn.example.net",
        "auth.example.com",
        "pay.example.org",
        "img.example.io",
    ];
    let paths = [
        "/v1/me",
        "/v1/items",
        "/oauth/token",
        "/img/banner.webp",
        "/v1/search",
        "/collect",
    ];
    let t = Instant::now();
    for i in 1..=100_000u64 {
        let body = format!(
            r#"{{"id":{i},"name":"user-{}","plan":"pro","tags":["a","b"]}}"#,
            i % 977
        );
        store.enqueue(Arc::new(FlowRecord {
            id: i,
            started_at_ms: 1_760_000_000_000 + i as i64,
            scheme: "https".into(),
            host: hosts[(i % 5) as usize].into(),
            port: 443,
            method: if i % 4 == 0 { "POST" } else { "GET" }.into(),
            path: format!("{}/{}", paths[(i % 6) as usize], i % 311),
            query: String::new(),
            status: [200, 200, 200, 204, 304, 404, 500][(i % 7) as usize],
            protocol: "h2".into(),
            state: FlowState::Complete,
            req_headers: vec![("accept".into(), "application/json".into())],
            res_headers: vec![("content-type".into(), "application/json".into())],
            res_size: body.len() as u64,
            res_body: Bytes::from(body),
            timings: Timings {
                total_us: 20_000 + (i % 2000) * 1000,
                ..Default::default()
            },
            ..Default::default()
        }));
    }
    store.flush();
    println!(
        "ingested 100,000 flows in {:.1} s ({} stored)",
        t.elapsed().as_secs_f64(),
        store.count().unwrap()
    );

    let q = |text: &str, kind| Query {
        text: text.into(),
        kind,
        ..Default::default()
    };
    println!("| query | matches | time |");
    println!("|---|---|---|");
    let cases = [
        ("all, first page", q("", Kind::All)),
        ("errors chip", q("", Kind::Errors)),
        ("slow chip", q("", Kind::Slow)),
        ("text: oauth", q("oauth", Kind::All)),
        ("text: user-42 (body)", q("user-42", Kind::All)),
        (
            "method:post status:5xx",
            q("method:post status:5xx", Kind::All),
        ),
        ("host substring: example.org", q("example.org", Kind::All)),
    ];
    for (name, query) in cases {
        let mut best = f64::MAX;
        let mut total = 0;
        for _ in 0..5 {
            let t = Instant::now();
            let page = store.search(&query, 0, 200).unwrap();
            best = best.min(t.elapsed().as_secs_f64() * 1000.0);
            total = page.total;
        }
        println!("| {name} | {total} | {best:.1} ms |");
    }
    store.close();
}
