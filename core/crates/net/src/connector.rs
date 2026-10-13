//! Upstream connections with per-phase timings.
//!
//! The connector resolves, connects and handshakes by hand so DNS, connect
//! and TLS durations are measured. The numbers ride along with the connection
//! as a response extension; the first response on a connection claims them,
//! later ones on the same connection report a reused connection.

use crate::body::BoxError;
use hyper::rt::{Read, ReadBufCursor, Write};
use hyper::Uri;
use hyper_util::client::legacy::connect::{Connected, Connection};
use hyper_util::rt::TokioIo;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::sync::OnceCell;
use tokio_rustls::TlsConnector;
use tower_service::Service;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Setup costs of one upstream connection.
#[derive(Debug)]
pub struct ConnInfo {
    pub dns_us: u64,
    pub connect_us: u64,
    pub tls_us: u64,
    claimed: AtomicBool,
}

impl ConnInfo {
    fn new(dns_us: u64, connect_us: u64, tls_us: u64) -> Arc<ConnInfo> {
        Arc::new(ConnInfo {
            dns_us,
            connect_us,
            tls_us,
            claimed: AtomicBool::new(false),
        })
    }

    /// True for the first caller only.
    pub fn claim(&self) -> bool {
        !self.claimed.swap(true, Ordering::AcqRel)
    }
}

pub enum Stream {
    Plain(TcpStream),
    Tls(Box<tokio_rustls::client::TlsStream<TcpStream>>),
}

impl Stream {
    pub fn alpn_h2(&self) -> bool {
        match self {
            Stream::Tls(t) => t.get_ref().1.alpn_protocol() == Some(b"h2"),
            Stream::Plain(_) => false,
        }
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Stream::Tls(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Stream::Tls(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_flush(cx),
            Stream::Tls(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Stream::Tls(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

/// A connected upstream stream, ready for hyper.
pub struct UpstreamIo {
    io: TokioIo<Stream>,
    info: Arc<ConnInfo>,
    h2: bool,
}

impl Read for UpstreamIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_read(cx, buf)
    }
}

impl Write for UpstreamIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.io).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_shutdown(cx)
    }

    fn is_write_vectored(&self) -> bool {
        self.io.is_write_vectored()
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.io).poll_write_vectored(cx, bufs)
    }
}

impl Connection for UpstreamIo {
    fn connected(&self) -> Connected {
        let c = Connected::new().extra(self.info.clone());
        if self.h2 {
            c.negotiated_h2()
        } else {
            c
        }
    }
}

/// Which ALPN protocols to offer upstream.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Alpn {
    H2AndHttp1,
    Http1Only,
}

/// Builds and holds the upstream TLS client configs, loading the system trust
/// store lazily off the async threads.
pub struct UpstreamTls {
    insecure: bool,
    configs: OnceCell<(Arc<ClientConfig>, Arc<ClientConfig>)>,
}

impl UpstreamTls {
    pub fn new(insecure: bool) -> Arc<UpstreamTls> {
        Arc::new(UpstreamTls {
            insecure,
            configs: OnceCell::new(),
        })
    }

    /// Start loading in the background so the first HTTPS request is not slow.
    pub fn warm(self: &Arc<Self>) {
        let me = self.clone();
        tokio::spawn(async move {
            let _ = me.config(Alpn::H2AndHttp1).await;
        });
    }

    pub async fn config(&self, alpn: Alpn) -> Arc<ClientConfig> {
        let insecure = self.insecure;
        let (h2, h1) = self
            .configs
            .get_or_init(|| async move {
                tokio::task::spawn_blocking(move || build_configs(insecure))
                    .await
                    .expect("tls config task")
            })
            .await;
        match alpn {
            Alpn::H2AndHttp1 => h2.clone(),
            Alpn::Http1Only => h1.clone(),
        }
    }
}

fn build_configs(insecure: bool) -> (Arc<ClientConfig>, Arc<ClientConfig>) {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let make = |alpn: Vec<Vec<u8>>| {
        let builder = ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .expect("protocol versions");
        let mut cfg = if insecure {
            builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(AcceptAny(provider.clone())))
                .with_no_client_auth()
        } else {
            builder
                .with_root_certificates(root_store())
                .with_no_client_auth()
        };
        cfg.alpn_protocols = alpn;
        Arc::new(cfg)
    };
    (
        make(vec![b"h2".to_vec(), b"http/1.1".to_vec()]),
        make(vec![b"http/1.1".to_vec()]),
    )
}

fn root_store() -> RootCertStore {
    let mut roots = RootCertStore::empty();
    let native = rustls_native_certs::load_native_certs();
    let (added, _ignored) = roots.add_parsable_certificates(native.certs);
    if added == 0 {
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    }
    roots
}

/// Verifier used with `--insecure-upstream`: accepts any certificate.
#[derive(Debug)]
struct AcceptAny(Arc<CryptoProvider>);

impl ServerCertVerifier for AcceptAny {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// Resolve, connect and optionally handshake. Returns the stream and its
/// measured setup costs.
pub async fn connect(
    host: &str,
    port: u16,
    tls: Option<Arc<ClientConfig>>,
) -> Result<(Stream, Arc<ConnInfo>), BoxError> {
    let t0 = Instant::now();
    let addrs: Vec<SocketAddr> = if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        vec![SocketAddr::new(ip, port)]
    } else {
        let resolved = tokio::time::timeout(CONNECT_TIMEOUT, tokio::net::lookup_host((host, port)))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "DNS lookup timed out"))?
            .map_err(|e| io::Error::new(e.kind(), format!("DNS lookup failed for {host}: {e}")))?;
        resolved.collect()
    };
    if addrs.is_empty() {
        return Err(format!("no addresses for {host}").into());
    }
    let dns_us = t0.elapsed().as_micros() as u64;

    let t1 = Instant::now();
    let tcp = connect_any(&addrs).await?;
    let _ = tcp.set_nodelay(true);
    let connect_us = t1.elapsed().as_micros() as u64;

    match tls {
        None => Ok((Stream::Plain(tcp), ConnInfo::new(dns_us, connect_us, 0))),
        Some(cfg) => {
            let name = ServerName::try_from(host.to_string())
                .map_err(|e| format!("invalid server name {host}: {e}"))?;
            let t2 = Instant::now();
            let stream =
                tokio::time::timeout(CONNECT_TIMEOUT, TlsConnector::from(cfg).connect(name, tcp))
                    .await
                    .map_err(|_| {
                        io::Error::new(io::ErrorKind::TimedOut, "TLS handshake timed out")
                    })?
                    .map_err(|e| describe_tls_error(host, e))?;
            let tls_us = t2.elapsed().as_micros() as u64;
            Ok((
                Stream::Tls(Box::new(stream)),
                ConnInfo::new(dns_us, connect_us, tls_us),
            ))
        }
    }
}

fn describe_tls_error(host: &str, e: io::Error) -> BoxError {
    if let Some(re) = e.get_ref().and_then(|i| i.downcast_ref::<rustls::Error>()) {
        return format!("TLS handshake with {host} failed: {re}").into();
    }
    format!("TLS handshake with {host} failed: {e}").into()
}

/// Alternate address families, starting with the first family returned, so
/// a dead IPv6 or IPv4 path does not hold up the working one.
fn interleave(addrs: &[SocketAddr]) -> Vec<SocketAddr> {
    let first_is_v6 = addrs.first().map(|a| a.is_ipv6()).unwrap_or(false);
    let (mut a, mut b): (Vec<_>, Vec<_>) = addrs
        .iter()
        .copied()
        .partition(|x| x.is_ipv6() == first_is_v6);
    let mut out = Vec::with_capacity(addrs.len());
    a.reverse();
    b.reverse();
    while !a.is_empty() || !b.is_empty() {
        if let Some(x) = a.pop() {
            out.push(x);
        }
        if let Some(x) = b.pop() {
            out.push(x);
        }
    }
    out
}

/// Staggered connect: start the next address every 250 ms (or as soon as the
/// previous attempt fails) and take the first that succeeds.
async fn connect_any(addrs: &[SocketAddr]) -> io::Result<TcpStream> {
    const STAGGER: Duration = Duration::from_millis(250);
    let ordered = interleave(addrs);
    let mut attempts = tokio::task::JoinSet::new();
    let mut next = 0;
    let mut last_err: Option<io::Error> = None;
    loop {
        if next < ordered.len() {
            let addr = ordered[next];
            next += 1;
            attempts.spawn(async move {
                match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(addr)).await {
                    Ok(r) => r,
                    Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "connect timed out")),
                }
            });
        }
        let more = next < ordered.len();
        let stagger = async {
            if more {
                tokio::time::sleep(STAGGER).await
            } else {
                std::future::pending::<()>().await
            }
        };
        tokio::select! {
            joined = attempts.join_next() => match joined {
                Some(Ok(Ok(stream))) => return Ok(stream),
                Some(Ok(Err(e))) => last_err = Some(e),
                Some(Err(e)) => last_err = Some(io::Error::other(e.to_string())),
                None => {
                    return Err(last_err.unwrap_or_else(|| io::Error::other("no addresses")));
                }
            },
            _ = stagger => {}
        }
    }
}

/// `tower::Service<Uri>` for hyper-util's pooled client.
#[derive(Clone)]
pub struct Connector {
    tls: Arc<UpstreamTls>,
    offline: bool,
}

impl Connector {
    pub fn new(tls: Arc<UpstreamTls>, offline: bool) -> Connector {
        Connector { tls, offline }
    }
}

impl Service<Uri> for Connector {
    type Response = UpstreamIo;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<UpstreamIo, BoxError>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let tls = self.tls.clone();
        let offline = self.offline;
        Box::pin(async move {
            if offline {
                return Err("upstream connections are disabled".into());
            }
            let https = uri.scheme_str() == Some("https");
            let host = uri
                .host()
                .ok_or("request has no host")?
                .trim_matches(|c| c == '[' || c == ']')
                .to_string();
            let port = uri.port_u16().unwrap_or(if https { 443 } else { 80 });
            let cfg = if https {
                Some(tls.config(Alpn::H2AndHttp1).await)
            } else {
                None
            };
            let (stream, info) = connect(&host, port, cfg).await?;
            let h2 = stream.alpn_h2();
            Ok(UpstreamIo {
                io: TokioIo::new(stream),
                info,
                h2,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_alternate_starting_with_the_first() {
        let a = |s: &str| s.parse::<SocketAddr>().unwrap();
        let out = interleave(&[
            a("[2001:db8::1]:80"),
            a("[2001:db8::2]:80"),
            a("192.0.2.1:80"),
            a("192.0.2.2:80"),
        ]);
        assert_eq!(
            out,
            [
                a("[2001:db8::1]:80"),
                a("192.0.2.1:80"),
                a("[2001:db8::2]:80"),
                a("192.0.2.2:80")
            ]
        );
        assert_eq!(interleave(&[a("192.0.2.1:80")]), [a("192.0.2.1:80")]);
        assert!(interleave(&[]).is_empty());
    }

    #[tokio::test]
    async fn skips_a_dead_address() {
        let live = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dead = {
            let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            l.local_addr().unwrap()
        };
        let stream = connect_any(&[dead, live.local_addr().unwrap()])
            .await
            .unwrap();
        assert_eq!(stream.peer_addr().unwrap(), live.local_addr().unwrap());
    }
}
