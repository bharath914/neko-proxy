//! End-to-end tests of the data path against local origin servers.

use bytes::Bytes;
use flow::{flags, FlowEvent, FlowRecord, FlowSink, FlowState, IdGen, InterceptorChain};
use http::{Request, Response, StatusCode};
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use proxy_net::{Proxy, ProxyConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tls::{CaStore, KeyStorage};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};

#[derive(Default)]
struct Collect(Mutex<Vec<FlowEvent>>);

impl FlowSink for Collect {
    fn publish(&self, e: FlowEvent) {
        self.0.lock().unwrap().push(e);
    }
}

impl Collect {
    async fn finished(&self, n: usize) -> Vec<Arc<FlowRecord>> {
        for _ in 0..500 {
            let got: Vec<_> = self
                .0
                .lock()
                .unwrap()
                .iter()
                .filter_map(|e| match e {
                    FlowEvent::Finished(r) => Some(r.clone()),
                    _ => None,
                })
                .collect();
            if got.len() >= n {
                return got;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("timed out waiting for {n} finished flows");
    }

    fn count(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

struct Harness {
    addr: SocketAddr,
    proxy: Proxy,
    sink: Arc<Collect>,
    ca: Arc<CaStore>,
    _dir: tempfile::TempDir,
}

async fn start_proxy() -> Harness {
    start_proxy_with(None).await
}

/// A proxy with the person's rules wired in the way `proxyd` does it.
async fn start_proxy_with(rules: Option<Arc<rules::RuleSet>>) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let ca = Arc::new(CaStore::load_or_create(dir.path(), KeyStorage::File).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let sink = Arc::new(Collect::default());
    let mut cfg = ProxyConfig::new(addr);
    cfg.insecure_upstream = true;
    let mut chain = InterceptorChain::empty();
    if let Some(r) = &rules {
        chain.push_request(r.clone());
    }
    let proxy = Proxy::new(
        cfg,
        ca.clone(),
        sink.clone(),
        Arc::new(IdGen::starting_after(0)),
        chain,
    );
    if let Some(r) = rules {
        proxy.set_connect_policy(r);
    }
    let p = proxy.clone();
    tokio::spawn(async move { p.serve(listener, std::future::pending()).await });
    Harness {
        addr,
        proxy,
        sink,
        ca,
        _dir: dir,
    }
}

async fn origin_handler(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let path = req.uri().path().to_string();
    let method = req.method().clone();
    let body = req.into_body().collect().await.unwrap().to_bytes();
    let (status, ct, out) = match (method.as_str(), path.as_str()) {
        (_, "/json") => (
            200,
            "application/json",
            Bytes::from_static(br#"{"ok":true,"n":1}"#),
        ),
        ("POST", "/echo") => (201, "text/plain", body),
        (_, "/missing") => (404, "text/plain", Bytes::from_static(b"nope")),
        _ => (200, "text/plain", Bytes::from_static(b"hello")),
    };
    Ok(Response::builder()
        .status(status)
        .header("content-type", ct)
        .body(Full::new(out))
        .unwrap())
}

async fn start_origin_http() -> SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (s, _) = l.accept().await.unwrap();
            tokio::spawn(async move {
                let _ = auto::Builder::new(TokioExecutor::new())
                    .serve_connection(TokioIo::new(s), service_fn(origin_handler))
                    .await;
            });
        }
    });
    addr
}

struct TlsOrigin {
    addr: SocketAddr,
    cert: CertificateDer<'static>,
}

async fn start_origin_tls() -> TlsOrigin {
    let ck =
        rcgen::generate_simple_self_signed(vec!["localhost".into(), "127.0.0.1".into()]).unwrap();
    let cert = ck.cert.der().clone();
    let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(ck.signing_key.serialize_der()));
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut cfg = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert.clone()], key)
        .unwrap();
    cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    let acceptor = TlsAcceptor::from(Arc::new(cfg));
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (s, _) = l.accept().await.unwrap();
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(tls) = acceptor.accept(s).await else {
                    return;
                };
                let _ = auto::Builder::new(TokioExecutor::new())
                    .serve_connection(TokioIo::new(tls), service_fn(origin_handler))
                    .await;
            });
        }
    });
    TlsOrigin { addr, cert }
}

fn client_config(trust: Option<&CertificateDer<'static>>, alpn: &[&[u8]]) -> Arc<ClientConfig> {
    let mut roots = RootCertStore::empty();
    if let Some(c) = trust {
        roots.add(c.clone()).unwrap();
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut cfg = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    cfg.alpn_protocols = alpn.iter().map(|a| a.to_vec()).collect();
    Arc::new(cfg)
}

async fn connect_tunnel(proxy: SocketAddr, authority: &str) -> TcpStream {
    let mut s = TcpStream::connect(proxy).await.unwrap();
    s.write_all(format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut buf = Vec::new();
    let mut b = [0u8; 1];
    while !buf.ends_with(b"\r\n\r\n") {
        s.read_exact(&mut b).await.unwrap();
        buf.push(b[0]);
    }
    assert!(
        buf.starts_with(b"HTTP/1.1 200"),
        "{}",
        String::from_utf8_lossy(&buf)
    );
    s
}

async fn get_over<T>(
    io: T,
    h2: bool,
    method: &str,
    path: &str,
    body: &'static [u8],
) -> (StatusCode, Bytes)
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "localhost")
        .body(Full::new(Bytes::from_static(body)))
        .unwrap();
    let res = if h2 {
        let (mut tx, conn) =
            hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(io))
                .await
                .unwrap();
        tokio::spawn(conn);
        let req = Request::builder()
            .method(method)
            .uri(format!("https://localhost{path}"))
            .body(Full::new(Bytes::from_static(body)))
            .unwrap();
        tx.send_request(req).await.unwrap()
    } else {
        let (mut tx, conn) = hyper::client::conn::http1::handshake(TokioIo::new(io))
            .await
            .unwrap();
        tokio::spawn(conn);
        tx.send_request(req).await.unwrap()
    };
    let status = res.status();
    (status, res.into_body().collect().await.unwrap().to_bytes())
}

#[tokio::test]
async fn plain_http_is_forwarded_and_recorded() {
    let h = start_proxy().await;
    let origin = start_origin_http().await;
    let mut s = TcpStream::connect(h.addr).await.unwrap();
    let req = format!(
        "GET http://{origin}/json?x=1 HTTP/1.1\r\nHost: {origin}\r\nConnection: close\r\n\r\n"
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).await.unwrap();
    assert!(out.starts_with("HTTP/1.1 200"), "{out}");
    assert!(out.ends_with(r#"{"ok":true,"n":1}"#), "{out}");

    let flows = h.sink.finished(1).await;
    let f = &flows[0];
    assert_eq!(f.method, "GET");
    assert_eq!(f.scheme, "http");
    assert_eq!(f.path, "/json");
    assert_eq!(f.query, "x=1");
    assert_eq!(f.status, 200);
    assert_eq!(f.state, FlowState::Complete);
    assert_eq!(&f.res_body[..], br#"{"ok":true,"n":1}"#);
    assert_eq!(f.res_size, 17);
    assert!(f.timings.total_us > 0);
    assert!(
        f.timings.connect_us > 0,
        "fresh connection records connect time"
    );
    assert!(!f.timings.reused);
    assert_eq!(f.summary().content_type, "json");
}

#[tokio::test]
async fn post_body_is_captured() {
    let h = start_proxy().await;
    let origin = start_origin_http().await;
    let mut s = TcpStream::connect(h.addr).await.unwrap();
    let req = format!(
        "POST http://{origin}/echo HTTP/1.1\r\nHost: {origin}\r\nContent-Length: 7\r\nConnection: close\r\n\r\nabc=123"
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).await.unwrap();
    assert!(out.starts_with("HTTP/1.1 201"), "{out}");
    let f = &h.sink.finished(1).await[0];
    assert_eq!(&f.req_body[..], b"abc=123");
    assert_eq!(f.req_size, 7);
    assert_eq!(&f.res_body[..], b"abc=123");
}

#[tokio::test]
async fn https_is_intercepted_over_http1_and_http2() {
    let h = start_proxy().await;
    let origin = start_origin_tls().await;
    let authority = format!("127.0.0.1:{}", origin.addr.port());

    // HTTP/1.1 to the proxy, h2 negotiated upstream.
    let tcp = connect_tunnel(h.addr, &authority).await;
    let name = ServerName::try_from("127.0.0.1").unwrap();
    let tls = TlsConnector::from(client_config(Some(h.ca.cert_der()), &[b"http/1.1"]))
        .connect(name.clone(), tcp)
        .await
        .expect("client trusts the proxy CA");
    let (status, body) = get_over(tls, false, "GET", "/json", b"").await;
    assert_eq!(status, 200);
    assert_eq!(&body[..], br#"{"ok":true,"n":1}"#);

    // h2 to the proxy.
    let tcp = connect_tunnel(h.addr, &authority).await;
    let tls = TlsConnector::from(client_config(Some(h.ca.cert_der()), &[b"h2"]))
        .connect(name, tcp)
        .await
        .unwrap();
    assert_eq!(tls.get_ref().1.alpn_protocol(), Some(&b"h2"[..]));
    let (status, body) = get_over(tls, true, "POST", "/echo", b"ping").await;
    assert_eq!(status, 201);
    assert_eq!(&body[..], b"ping");

    let flows = h.sink.finished(2).await;
    for f in &flows {
        assert_eq!(f.scheme, "https");
        assert_eq!(f.host, "127.0.0.1");
        assert_eq!(f.protocol, "h2", "upstream negotiated h2");
        assert_eq!(f.state, FlowState::Complete);
    }
    // Both requests share one pooled h2 connection: one pays for the
    // handshake, the other reports a reused connection.
    let fresh: Vec<_> = flows.iter().filter(|f| !f.timings.reused).collect();
    assert_eq!(fresh.len(), 1);
    assert!(
        fresh[0].timings.tls_us > 0,
        "tls handshake timed on the fresh connection"
    );
    assert!(fresh[0].timings.connect_us > 0);
    let post = flows.iter().find(|f| f.method == "POST").unwrap();
    assert_eq!(&post.req_body[..], b"ping");
    assert_eq!(&post.res_body[..], b"ping");
    drop(origin.cert);
}

#[tokio::test]
async fn untrusting_client_marks_host_pinned_then_tunnels() {
    let h = start_proxy().await;
    let origin = start_origin_tls().await;
    let authority = format!("127.0.0.1:{}", origin.addr.port());
    let name = ServerName::try_from("127.0.0.1").unwrap();

    // First attempt: client does not trust the minted certificate.
    let tcp = connect_tunnel(h.addr, &authority).await;
    let res = TlsConnector::from(client_config(None, &[b"http/1.1"]))
        .connect(name.clone(), tcp)
        .await;
    assert!(res.is_err());
    let flows = h.sink.finished(1).await;
    assert_ne!(flows[0].flags & flags::PINNED, 0);
    assert_eq!(flows[0].state, FlowState::Failed);
    assert_eq!(h.proxy.pinned_hosts(), vec!["127.0.0.1".to_string()]);

    // Second attempt: tunneled, so the client sees the origin's own cert.
    let tcp = connect_tunnel(h.addr, &authority).await;
    let tls = TlsConnector::from(client_config(Some(&origin.cert), &[b"http/1.1"]))
        .connect(name, tcp)
        .await
        .expect("origin certificate reaches the client untouched");
    let (status, body) = get_over(tls, false, "GET", "/hello", b"").await;
    assert_eq!(status, 200);
    assert_eq!(&body[..], b"hello");

    let flows = h.sink.finished(2).await;
    let t = &flows[1];
    assert_eq!(t.protocol, "tunnel");
    assert_ne!(t.flags & flags::TUNNEL, 0);
    assert_ne!(t.flags & flags::PINNED, 0);
    assert_eq!(t.state, FlowState::Complete);
    assert!(t.res_size > 0, "tunnel counted bytes");

    h.proxy.clear_pinned();
    assert!(h.proxy.pinned_hosts().is_empty());
}

#[tokio::test]
async fn map_local_answers_from_the_file_and_is_recorded() {
    let rules = Arc::new(rules::RuleSet::in_memory());
    let h = start_proxy_with(Some(rules.clone())).await;
    let origin = start_origin_http().await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("mapped.json");
    std::fs::write(&file, br#"{"mapped":1}"#).unwrap();
    rules
        .put_map_local(rules::MapLocalRule {
            id: String::new(),
            enabled: true,
            host: "127.0.0.1".into(),
            port: origin.port(),
            path: "/json".into(),
            query: String::new(),
            file: file.clone(),
            status: 201,
            content_type: String::new(),
        })
        .unwrap();

    let get = |path: &str| {
        let req = format!(
            "GET http://{origin}{path} HTTP/1.1\r\nHost: {origin}\r\nConnection: close\r\n\r\n"
        );
        let addr = h.addr;
        async move {
            let mut s = TcpStream::connect(addr).await.unwrap();
            s.write_all(req.as_bytes()).await.unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).await.unwrap();
            out
        }
    };

    let out = get("/json?cache=bust").await;
    assert!(out.starts_with("HTTP/1.1 201"), "{out}");
    assert!(out.to_ascii_lowercase().contains("content-type: application/json"));
    assert!(out.ends_with(r#"{"mapped":1}"#), "{out}");

    // The file is read per request, so an edit shows at once.
    std::fs::write(&file, br#"{"mapped":2}"#).unwrap();
    assert!(get("/json").await.ends_with(r#"{"mapped":2}"#));

    // A path the rule does not cover still reaches the origin.
    assert!(get("/missing").await.starts_with("HTTP/1.1 404"));

    let flows = h.sink.finished(3).await;
    let mapped = &flows[0];
    assert_ne!(mapped.flags & flags::MAPPED, 0);
    assert_eq!(mapped.status, 201);
    assert_eq!(&mapped.res_body[..], br#"{"mapped":1}"#, "body is recorded");
    assert_eq!(mapped.state, FlowState::Complete);
    assert_eq!(flows[2].flags & flags::MAPPED, 0);
    assert_eq!(flows[2].status, 404);
}

#[tokio::test]
async fn ssl_list_tunnels_hosts_outside_it_and_decrypts_inside() {
    let rules = Arc::new(rules::RuleSet::in_memory());
    rules
        .set_ssl(rules::SslSettings {
            mode: rules::SslMode::Listed,
            hosts: vec!["only.example.org".into()],
            exclude: vec![],
        })
        .unwrap();
    let h = start_proxy_with(Some(rules.clone())).await;
    let origin = start_origin_tls().await;
    let authority = format!("127.0.0.1:{}", origin.addr.port());
    let name = ServerName::try_from("127.0.0.1").unwrap();

    // Outside the list: untouched. The client sees the origin's own
    // certificate, and the host is not marked pinned.
    let tcp = connect_tunnel(h.addr, &authority).await;
    let tls = TlsConnector::from(client_config(Some(&origin.cert), &[b"http/1.1"]))
        .connect(name.clone(), tcp)
        .await
        .expect("origin certificate reaches the client untouched");
    let (status, body) = get_over(tls, false, "GET", "/hello", b"").await;
    assert_eq!(status, 200);
    assert_eq!(&body[..], b"hello");
    let flows = h.sink.finished(1).await;
    assert_ne!(flows[0].flags & flags::TUNNEL, 0);
    assert_ne!(flows[0].flags & flags::SSL_BYPASSED, 0);
    assert_eq!(flows[0].flags & flags::PINNED, 0);
    assert!(h.proxy.pinned_hosts().is_empty());

    // Add the host to the list: the next connection is decrypted.
    rules.enable_host("127.0.0.1").unwrap();
    let tcp = connect_tunnel(h.addr, &authority).await;
    let tls = TlsConnector::from(client_config(Some(h.ca.cert_der()), &[b"http/1.1"]))
        .connect(name.clone(), tcp)
        .await
        .expect("the proxy CA is presented once the host is listed");
    let (status, _) = get_over(tls, false, "GET", "/json", b"").await;
    assert_eq!(status, 200);
    let flows = h.sink.finished(2).await;
    let decrypted = flows.iter().find(|f| f.method == "GET").unwrap();
    assert_eq!(decrypted.path, "/json");
    assert_eq!(decrypted.flags & flags::SSL_BYPASSED, 0);

    // Excluding it again wins over the list.
    rules.disable_host("127.0.0.1").unwrap();
    let tcp = connect_tunnel(h.addr, &authority).await;
    TlsConnector::from(client_config(Some(&origin.cert), &[b"http/1.1"]))
        .connect(name, tcp)
        .await
        .expect("excluded host is tunneled");
}

#[tokio::test]
async fn websocket_upgrade_passes_through() {
    let h = start_proxy().await;
    // Raw origin: accept the upgrade, then echo bytes.
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = l.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut s, _) = l.accept().await.unwrap();
        let mut buf = Vec::new();
        let mut b = [0u8; 1];
        while !buf.ends_with(b"\r\n\r\n") {
            s.read_exact(&mut b).await.unwrap();
            buf.push(b[0]);
        }
        let head = String::from_utf8_lossy(&buf).to_ascii_lowercase();
        assert!(head.contains("upgrade: websocket"), "{head}");
        s.write_all(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n")
            .await
            .unwrap();
        let mut echo = [0u8; 4];
        s.read_exact(&mut echo).await.unwrap();
        s.write_all(&echo).await.unwrap();
        let _ = s.shutdown().await;
    });

    let mut c = TcpStream::connect(h.addr).await.unwrap();
    let req = format!(
        "GET http://{origin}/socket HTTP/1.1\r\nHost: {origin}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    c.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    let mut b = [0u8; 1];
    while !buf.ends_with(b"\r\n\r\n") {
        c.read_exact(&mut b).await.unwrap();
        buf.push(b[0]);
    }
    assert!(
        buf.starts_with(b"HTTP/1.1 101"),
        "{}",
        String::from_utf8_lossy(&buf)
    );
    c.write_all(b"ping").await.unwrap();
    let mut echo = [0u8; 4];
    c.read_exact(&mut echo).await.unwrap();
    assert_eq!(&echo, b"ping");
    drop(c);

    let f = &h.sink.finished(1).await[0];
    assert_eq!(f.status, 101);
    assert_ne!(f.flags & flags::WEBSOCKET, 0);
    assert_eq!(f.req_size, 4);
    assert_eq!(f.res_size, 4);
}

#[tokio::test]
async fn paused_capture_forwards_without_recording() {
    let h = start_proxy().await;
    let origin = start_origin_http().await;
    h.proxy.set_recording(false);
    assert!(!h.proxy.recording());
    let mut s = TcpStream::connect(h.addr).await.unwrap();
    s.write_all(
        format!(
            "GET http://{origin}/hello HTTP/1.1\r\nHost: {origin}\r\nConnection: close\r\n\r\n"
        )
        .as_bytes(),
    )
    .await
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).await.unwrap();
    assert!(out.starts_with("HTTP/1.1 200"));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(h.sink.count(), 0);
}

#[tokio::test]
async fn unreachable_upstream_gives_502_and_failed_flow() {
    let h = start_proxy().await;
    // A port nobody listens on.
    let dead = {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap()
    };
    let mut s = TcpStream::connect(h.addr).await.unwrap();
    s.write_all(
        format!("GET http://{dead}/x HTTP/1.1\r\nHost: {dead}\r\nConnection: close\r\n\r\n")
            .as_bytes(),
    )
    .await
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).await.unwrap();
    assert!(out.starts_with("HTTP/1.1 502"), "{out}");
    let f = &h.sink.finished(1).await[0];
    assert_eq!(f.state, FlowState::Failed);
    assert_ne!(f.flags & flags::ERROR, 0);
    assert!(!f.error.is_empty());
}

#[tokio::test]
async fn proxy_serves_its_own_ca() {
    let h = start_proxy().await;
    let mut s = TcpStream::connect(h.addr).await.unwrap();
    s.write_all(b"GET /ca.pem HTTP/1.1\r\nHost: proxy.local\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).await.unwrap();
    assert!(out.contains("BEGIN CERTIFICATE"), "{out}");
    assert_eq!(h.sink.count(), 0, "local pages are not flows");
}

#[tokio::test]
async fn keep_alive_reuse_reports_reused_connection() {
    let h = start_proxy().await;
    let origin = start_origin_http().await;
    let mut s = TcpStream::connect(h.addr).await.unwrap();
    for _ in 0..2 {
        s.write_all(
            format!("GET http://{origin}/hello HTTP/1.1\r\nHost: {origin}\r\n\r\n").as_bytes(),
        )
        .await
        .unwrap();
        let mut buf = vec![0u8; 512];
        let mut got = 0;
        loop {
            let n = s.read(&mut buf[got..]).await.unwrap();
            got += n;
            if buf[..got].ends_with(b"hello") {
                break;
            }
        }
    }
    let flows = h.sink.finished(2).await;
    let reused = flows.iter().filter(|f| f.timings.reused).count();
    let fresh = flows.iter().filter(|f| !f.timings.reused).count();
    assert_eq!((fresh, reused), (1, 1));
    let _ = Empty::<Bytes>::new();
}

/// Feed mutated and random bytes to the real connection handler. Nothing
/// may panic or hang, and the proxy must keep serving afterwards.
#[tokio::test]
async fn parser_survives_hostile_input() {
    use tokio::io::{duplex, AsyncWriteExt};
    let dir = tempfile::tempdir().unwrap();
    let ca = Arc::new(CaStore::load_or_create(dir.path(), KeyStorage::File).unwrap());
    let mut cfg = ProxyConfig::new("127.0.0.1:8899".parse().unwrap());
    cfg.offline = true;
    let sink = Arc::new(Collect::default());
    let proxy = Proxy::new(cfg, ca, sink, Arc::new(IdGen::starting_after(0)), InterceptorChain::empty());

    let seeds: [&[u8]; 8] = [
        b"GET http://example.com/a?b=c HTTP/1.1\r\nHost: example.com\r\n\r\n",
        b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n",
        b"POST http://h/p HTTP/1.1\r\nHost: h\r\nContent-Length: 5\r\n\r\nhello",
        b"POST http://h/p HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n",
        b"GET /ca.pem HTTP/1.1\r\nHost: proxy.local\r\n\r\n",
        b"GET ws://h/s HTTP/1.1\r\nHost: h\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n",
        b"OPTIONS * HTTP/1.1\r\nHost: h\r\n\r\n",
        b"GET http://[::1]:99999/ HTTP/1.1\r\nHost: x\r\n\r\n",
    ];
    let mut state: u64 = 0x9E3779B97F4A7C15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for round in 0..400 {
        let mut input = seeds[(next() % seeds.len() as u64) as usize].to_vec();
        match round % 4 {
            0 => {
                for _ in 0..(next() % 6) {
                    let i = (next() % input.len() as u64) as usize;
                    input[i] = next() as u8;
                }
            }
            1 => {
                let at = (next() % input.len() as u64) as usize;
                input.truncate(at.max(1));
            }
            2 => {
                let n = (next() % 600) as usize;
                input = (0..n).map(|_| next() as u8).collect();
            }
            _ => {
                let at = (next() % input.len() as u64) as usize;
                let junk: Vec<u8> = (0..(next() % 40)).map(|_| next() as u8).collect();
                input.splice(at..at, junk);
            }
        }
        let (mut client, server) = duplex(64 * 1024);
        let p = proxy.clone();
        let handle = tokio::spawn(async move {
            p.serve_io(server, "127.0.0.1:50000".parse().unwrap()).await;
        });
        let _ = client.write_all(&input).await;
        let _ = client.shutdown().await;
        // Drain whatever the proxy answers so it never blocks on a full pipe.
        let mut sink = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(2), tokio::io::AsyncReadExt::read_to_end(&mut client, &mut sink)).await;
        tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .expect("handler finished")
            .expect("handler did not panic");
    }
}
