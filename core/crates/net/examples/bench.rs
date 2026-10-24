//! Proxy overhead and throughput against a local origin.
//! `cargo run --release -p neko-net --example bench`

use flow::{IdGen, InterceptorChain, NullSink};
use http_body_util::Full;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use neko_net::{Proxy, ProxyConfig};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tls::{CaStore, KeyStorage};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn origin() -> SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    let body = bytes::Bytes::from(vec![b'x'; 1024]);
    tokio::spawn(async move {
        loop {
            let (s, _) = l.accept().await.unwrap();
            let body = body.clone();
            tokio::spawn(async move {
                let svc = service_fn(move |_req| {
                    let body = body.clone();
                    async move {
                        Ok::<_, std::convert::Infallible>(hyper::Response::new(Full::new(body)))
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(s), svc)
                    .await;
            });
        }
    });
    addr
}

/// One keep-alive request; returns once the full response is read.
async fn request(s: &mut TcpStream, req: &[u8], buf: &mut Vec<u8>) {
    s.write_all(req).await.unwrap();
    buf.clear();
    let mut chunk = [0u8; 4096];
    loop {
        let n = s.read(&mut chunk).await.unwrap();
        assert!(n > 0, "connection closed");
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..pos]).to_ascii_lowercase();
            let len: usize = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length: "))
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);
            if buf.len() >= pos + 4 + len {
                return;
            }
        }
    }
}

async fn latency(target: SocketAddr, req: String, n: usize) -> Vec<f64> {
    let mut s = TcpStream::connect(target).await.unwrap();
    s.set_nodelay(true).unwrap();
    let mut buf = Vec::new();
    for _ in 0..200 {
        request(&mut s, req.as_bytes(), &mut buf).await;
    }
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let t = Instant::now();
        request(&mut s, req.as_bytes(), &mut buf).await;
        out.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap());
    out
}

fn pct(v: &[f64], p: f64) -> f64 {
    v[((v.len() as f64 * p) as usize).min(v.len() - 1)]
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let dir = tempfile::tempdir().unwrap();
    let ca = Arc::new(CaStore::load_or_create(dir.path(), KeyStorage::File).unwrap());
    let origin = origin().await;

    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let paddr = l.local_addr().unwrap();
    let proxy = Proxy::new(
        ProxyConfig::new(paddr),
        ca,
        Arc::new(NullSink),
        Arc::new(IdGen::starting_after(0)),
        InterceptorChain::empty(),
    );
    let p = proxy.clone();
    tokio::spawn(async move { p.serve(l, std::future::pending()).await });
    tokio::time::sleep(Duration::from_millis(100)).await;

    let direct_req = format!("GET /x HTTP/1.1\r\nHost: {origin}\r\n\r\n");
    let proxied_req = format!("GET http://{origin}/x HTTP/1.1\r\nHost: {origin}\r\n\r\n");

    let n = 20_000;
    let direct = latency(origin, direct_req, n).await;
    let via = latency(paddr, proxied_req.clone(), n).await;
    println!("| metric | direct | via proxy | overhead |");
    println!("|---|---|---|---|");
    for (name, p) in [("p50", 0.5), ("p90", 0.9), ("p99", 0.99)] {
        let (d, v) = (pct(&direct, p), pct(&via, p));
        println!(
            "| latency {name} | {d:.3} ms | {v:.3} ms | {:.3} ms |",
            v - d
        );
    }

    // Throughput: many keep-alive connections for a fixed window.
    let count = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let mut tasks = Vec::new();
    let conns = 64;
    for _ in 0..conns {
        let (count, stop, req) = (count.clone(), stop.clone(), proxied_req.clone());
        tasks.push(tokio::spawn(async move {
            let mut s = TcpStream::connect(paddr).await.unwrap();
            s.set_nodelay(true).unwrap();
            let mut buf = Vec::new();
            while !stop.load(Ordering::Relaxed) {
                request(&mut s, req.as_bytes(), &mut buf).await;
                count.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    let window = Duration::from_secs(5);
    let t = Instant::now();
    tokio::time::sleep(window).await;
    stop.store(true, Ordering::Relaxed);
    let done = count.load(Ordering::Relaxed);
    let secs = t.elapsed().as_secs_f64();
    for t in tasks {
        let _ = t.await;
    }
    println!();
    println!(
        "sustained: {:.0} req/s over {conns} connections ({done} requests in {secs:.1} s)",
        done as f64 / secs
    );
}
