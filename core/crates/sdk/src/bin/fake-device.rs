//! A pretend phone. Pairs with a running core and streams varied synthetic
//! calls, so the desktop screens can be tried without a real app.
//!
//!   cargo run -p sdk --bin fake-device -- --port 8897 --app com.demo.shop --calls 200
//!
//! The token is kept in --state, so the second run reconnects without a prompt.

use clap::Parser;
use sdk::fake_client::{now_ms, Client};
use sdk::wire::{self, body_chunk::Part, envelope::Msg};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = sdk::DEFAULT_PORT)]
    port: u16,
    #[arg(long, default_value = "com.demo.shop")]
    app: String,
    #[arg(long, default_value = "Pixel 8 (fake)")]
    name: String,
    #[arg(long, default_value = "fake-device-1")]
    install_id: String,
    /// Calls to make (0 = until stopped).
    #[arg(long, default_value_t = 50)]
    calls: u64,
    /// Calls per second.
    #[arg(long, default_value_t = 4.0)]
    rate: f64,
    /// Where the token is kept between runs.
    #[arg(long)]
    state: Option<PathBuf>,
    /// Report this connection kind (emulator, usb, wifi).
    #[arg(long, default_value = "wifi")]
    connection: String,
}

struct Route {
    method: &'static str,
    url: &'static str,
    template: &'static str,
    name: &'static str,
    owner: &'static str,
    screen: &'static str,
    status: u32,
    req: &'static str,
    res: &'static str,
    ct: &'static str,
    lazy_bytes: usize,
    fail: Option<&'static str>,
}

const ROUTES: &[Route] = &[
    Route {
        method: "GET",
        url: "https://api.demo-shop.test/v1/products?page=1",
        template: "/v1/products",
        name: "List products",
        owner: "catalog",
        screen: "ProductListActivity",
        status: 200,
        req: "",
        res: r#"{"items":[{"id":1,"title":"Mug"},{"id":2,"title":"Tee"}],"next":2}"#,
        ct: "application/json",
        lazy_bytes: 0,
        fail: None,
    },
    Route {
        method: "GET",
        url: "https://api.demo-shop.test/v1/products/42",
        template: "/v1/products/{id}",
        name: "Product detail",
        owner: "catalog",
        screen: "ProductActivity",
        status: 200,
        req: "",
        res: r#"{"id":42,"title":"Mug","price":{"amount":1299,"currency":"USD"}}"#,
        ct: "application/json",
        lazy_bytes: 0,
        fail: None,
    },
    Route {
        method: "POST",
        url: "https://api.demo-shop.test/v1/cart/7/coupon",
        template: "/v1/cart/{id}/coupon",
        name: "Apply coupon",
        owner: "payments",
        screen: "CartActivity",
        status: 200,
        req: r#"{"code":"SPRING25"}"#,
        res: r#"{"discount":2500}"#,
        ct: "application/json",
        lazy_bytes: 0,
        fail: None,
    },
    Route {
        method: "POST",
        url: "https://auth.demo-shop.test/oauth/token",
        template: "/oauth/token",
        name: "Sign in",
        owner: "identity",
        screen: "LoginActivity",
        status: 200,
        req: r#"{"username":"asha","password":"[redacted]"}"#,
        res: r#"{"access_token":"[redacted]","expires_in":3600}"#,
        ct: "application/json",
        lazy_bytes: 0,
        fail: None,
    },
    Route {
        method: "POST",
        url: "https://api.demo-shop.test/v1/checkout",
        template: "/v1/checkout",
        name: "Place order",
        owner: "payments",
        screen: "CheckoutActivity",
        status: 402,
        req: r#"{"cart":7}"#,
        res: r#"{"error":"card_declined"}"#,
        ct: "application/json",
        lazy_bytes: 0,
        fail: None,
    },
    Route {
        method: "GET",
        url: "https://cdn.demo-shop.test/catalog/export.json",
        template: "/catalog/export.json",
        name: "Catalog export",
        owner: "catalog",
        screen: "ProductListActivity",
        status: 200,
        req: "",
        res: "",
        ct: "application/json",
        lazy_bytes: 400_000,
        fail: None,
    },
    Route {
        method: "GET",
        url: "https://api.demo-shop.test/v1/recommendations",
        template: "/v1/recommendations",
        name: "Recommendations",
        owner: "growth",
        screen: "HomeActivity",
        status: 200,
        req: "",
        res: "",
        ct: "application/json",
        lazy_bytes: 0,
        fail: Some("timeout"),
    },
    Route {
        method: "GET",
        url: "https://api.demo-shop.test/v1/me",
        template: "/v1/me",
        name: "Profile",
        owner: "identity",
        screen: "HomeActivity",
        status: 500,
        req: "",
        res: r#"{"error":"internal"}"#,
        ct: "application/json",
        lazy_bytes: 0,
        fail: None,
    },
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a = Args::parse();
    let token: Vec<u8> = a
        .state
        .as_ref()
        .and_then(|p| std::fs::read(p).ok())
        .unwrap_or_default();
    let mut c = Client::connect_to(&a.host, a.port).await?;
    let mut hello = Client::hello_msg(&a.install_id, &a.app, &token);
    hello.device_name = a.name.clone();
    hello.connection = a.connection.clone();
    c.send(Msg::Hello(hello)).await;
    let ack = match c.next().await {
        Some(Msg::HelloAck(a)) => a,
        Some(Msg::Error(e)) => return Err(format!("refused: {}", e.message).into()),
        other => return Err(format!("unexpected {other:?}").into()),
    };
    println!("desktop: {}", ack.desktop_name);
    use wire::hello_ack::State;
    match State::try_from(ack.state).unwrap_or(State::Denied) {
        State::Authenticated => {}
        State::PairingRequired => {
            let (_, res) = c
                .pair_wait(|code| {
                    println!(
                        "Pairing code {} {}  (compare it with the desktop, then Allow)",
                        &code[..3],
                        &code[3..]
                    );
                })
                .await;
            if !res.allowed {
                return Err(format!("not allowed: {}", res.message).into());
            }
            if let Some(p) = &a.state {
                std::fs::write(p, &res.token)?;
            }
            println!("paired");
        }
        State::Denied => return Err("denied on the desktop".into()),
        State::IngestDisabled => return Err("SDK connections are off on the desktop".into()),
    }

    let gap = Duration::from_secs_f64(1.0 / a.rate.max(0.01));
    let mut n = 0u64;
    while a.calls == 0 || n < a.calls {
        // Answer pings and take config pushes without blocking the traffic.
        while let Some(m) = c.recv_within(Duration::from_millis(1)).await {
            match m {
                Msg::Ping(p) => {
                    c.send(Msg::Pong(wire::Pong {
                        echo_ms: p.time_ms,
                        time_ms: now_ms(),
                    }))
                    .await
                }
                Msg::BodyFetch(f) => serve_fetch(&mut c, f).await,
                _ => {}
            }
        }
        let r = &ROUTES[(n as usize) % ROUTES.len()];
        call(&mut c, r, n).await;
        n += 1;
        tokio::time::sleep(gap).await;
    }
    // Stay around briefly so lazy body fetches can be answered.
    let until = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < until {
        match c.recv_within(Duration::from_millis(200)).await {
            Some(Msg::BodyFetch(f)) => serve_fetch(&mut c, f).await,
            Some(Msg::Ping(p)) => {
                c.send(Msg::Pong(wire::Pong {
                    echo_ms: p.time_ms,
                    time_ms: now_ms(),
                }))
                .await
            }
            _ => {}
        }
    }
    println!("sent {n} calls");
    Ok(())
}

async fn serve_fetch(c: &mut Client, f: wire::BodyFetch) {
    let body = lazy_body(400_000);
    c.send(Msg::BodyFetchResult(wire::BodyFetchResult {
        call_id: f.call_id.clone(),
        part: f.part,
        available: true,
    }))
    .await;
    let chunks: Vec<&[u8]> = body.chunks(60_000).collect();
    for (i, d) in chunks.iter().enumerate() {
        c.send(Msg::BodyChunk(wire::BodyChunk {
            call_id: f.call_id.clone(),
            part: f.part,
            seq: i as u32,
            data: d.to_vec(),
            last: i + 1 == chunks.len(),
            ..Default::default()
        }))
        .await;
    }
}

fn lazy_body(n: usize) -> Vec<u8> {
    let mut s = String::from("{\"items\":[");
    let mut i = 0;
    while s.len() < n {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("{{\"id\":{i},\"title\":\"Item {i}\"}}"));
        i += 1;
    }
    s.push_str("]}");
    s.into_bytes()
}

async fn call(c: &mut Client, r: &Route, n: u64) {
    let id = format!("call-{n}-{}", now_ms());
    let started = now_ms();
    c.send(Msg::CallStart(wire::CallStart {
        call_id: id.clone(),
        started_at_ms: started,
        method: r.method.into(),
        url: r.url.into(),
        headers: vec![
            wire::Header {
                name: "Accept".into(),
                value: "application/json".into(),
            },
            wire::Header {
                name: "Authorization".into(),
                value: "[redacted]".into(),
            },
            wire::Header {
                name: "User-Agent".into(),
                value: "okhttp/4.12.0".into(),
            },
        ],
        http_version: "h2".into(),
        client_stack: "okhttp".into(),
        route_template: r.template.into(),
        name: r.name.into(),
        owner: r.owner.into(),
        screen: r.screen.into(),
        request_body_size: r.req.len() as u64,
        ..Default::default()
    }))
    .await;
    if !r.req.is_empty() {
        c.send(Msg::BodyChunk(wire::BodyChunk {
            call_id: id.clone(),
            part: Part::Request as i32,
            seq: 0,
            data: r.req.as_bytes().to_vec(),
            last: true,
            ..Default::default()
        }))
        .await;
    }
    let ms = 20 + (n * 37 % 400) as i64;
    tokio::time::sleep(Duration::from_millis(ms as u64 / 4)).await;
    if let Some(kind) = r.fail {
        c.send(Msg::CallFailed(wire::CallFailed {
            call_id: id,
            kind: kind.into(),
            message: "Read timed out".into(),
            timings: Some(wire::Timings {
                dns: 3,
                connect: 20,
                tls: 30,
                request_sent: 1,
                ttfb: -1,
                download: -1,
            }),
            ended_at_ms: now_ms(),
        }))
        .await;
        return;
    }
    c.send(Msg::ResponseStart(wire::ResponseStart {
        call_id: id.clone(),
        status: r.status,
        headers: vec![wire::Header {
            name: "Content-Type".into(),
            value: r.ct.into(),
        }],
        http_version: "h2".into(),
        response_body_size: (r.res.len().max(r.lazy_bytes)) as u64,
    }))
    .await;
    let lazy = r.lazy_bytes > 0;
    if !lazy && !r.res.is_empty() {
        c.send(Msg::BodyChunk(wire::BodyChunk {
            call_id: id.clone(),
            part: Part::Response as i32,
            seq: 0,
            data: r.res.as_bytes().to_vec(),
            last: true,
            ..Default::default()
        }))
        .await;
    }
    c.send(Msg::CallComplete(wire::CallComplete {
        call_id: id,
        ended_at_ms: started + ms,
        timings: Some(wire::Timings {
            dns: if n.is_multiple_of(3) { 4 } else { 0 },
            connect: if n.is_multiple_of(3) { 18 } else { 0 },
            tls: if n.is_multiple_of(3) { 31 } else { -1 },
            request_sent: 1,
            ttfb: ms as i32 / 2,
            download: 3,
        }),
        response_body_available: !lazy,
        request_body_available: true,
        request_redacted: r.template == "/oauth/token",
        response_redacted: r.template == "/oauth/token",
        response_body_size: (r.res.len().max(r.lazy_bytes)) as u64,
        request_body_size: r.req.len() as u64,
        ..Default::default()
    }))
    .await;
}
