//! A fake SDK client for tests and demos: connects over TLS without
//! verifying the certificate (it records the fingerprint, as a real SDK does
//! while pairing), speaks the protocol, and nothing more.

use crate::wire::{self, envelope::Msg, Envelope};
use crate::Sdk;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

#[derive(Debug)]
struct AcceptAll(Arc<Mutex<Option<[u8; 32]>>>);

impl ServerCertVerifier for AcceptAll {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        *self.0.lock().unwrap() = Some(Sha256::digest(end_entity.as_ref()).into());
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            m,
            c,
            d,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            m,
            c,
            d,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

pub struct Client {
    r: ReadHalf<TlsStream<TcpStream>>,
    w: WriteHalf<TlsStream<TcpStream>>,
    pub fingerprint: [u8; 32],
    pub seq: u32,
}

impl Client {
    pub async fn connect(port: u16) -> std::io::Result<Client> {
        Self::connect_to("127.0.0.1", port).await
    }

    pub async fn connect_to(host: &str, port: u16) -> std::io::Result<Client> {
        let seen = Arc::new(Mutex::new(None));
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let cfg = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAll(seen.clone())))
            .with_no_client_auth();
        let tcp = TcpStream::connect((host, port)).await?;
        let tls = tokio_rustls::TlsConnector::from(Arc::new(cfg))
            .connect(ServerName::try_from("proxy-ingest.local").unwrap(), tcp)
            .await?;
        let fingerprint = seen.lock().unwrap().unwrap();
        let (r, w) = tokio::io::split(tls);
        Ok(Client {
            r,
            w,
            fingerprint,
            seq: 0,
        })
    }

    pub async fn send(&mut self, msg: Msg) {
        let buf = crate::frame::encode(&Envelope { msg: Some(msg) });
        let _ = self.w.write_all(&buf).await;
        let _ = self.w.flush().await;
    }

    pub async fn send_raw(&mut self, bytes: &[u8]) {
        let _ = self.w.write_all(bytes).await;
        let _ = self.w.flush().await;
    }

    /// Next message, or None if the server closed the connection.
    pub async fn recv(&mut self) -> Option<Msg> {
        self.recv_within(Duration::from_secs(5)).await
    }

    pub async fn recv_within(&mut self, d: Duration) -> Option<Msg> {
        match crate::frame::read_envelope(&mut self.r, 4 << 20, Some(d), Duration::from_secs(5))
            .await
        {
            Ok(e) => e.msg,
            Err(_) => None,
        }
    }

    /// Next message that is not a Config push.
    pub async fn next_non_config(&mut self) -> Option<Msg> {
        loop {
            match self.next().await? {
                Msg::Config(_) => continue,
                other => return Some(other),
            }
        }
    }

    /// Skip pings and return the next substantive message.
    pub async fn next(&mut self) -> Option<Msg> {
        loop {
            match self.recv().await? {
                Msg::Ping(p) => {
                    self.send(Msg::Pong(wire::Pong {
                        echo_ms: p.time_ms,
                        time_ms: now_ms(),
                    }))
                    .await;
                }
                other => return Some(other),
            }
        }
    }

    pub fn hello_msg(install: &str, app: &str, token: &[u8]) -> wire::Hello {
        wire::Hello {
            protocol_version: wire::PROTOCOL_VERSION,
            sdk_name: "android".into(),
            sdk_version: "0.1.0".into(),
            app_id: app.into(),
            app_name: "Shop".into(),
            app_version: "1.2.3".into(),
            build: "42".into(),
            device_install_id: install.into(),
            device_name: "Pixel 8".into(),
            device_model: "Google Pixel 8".into(),
            os_version: "Android 15".into(),
            device_time_ms: now_ms(),
            token: token.to_vec(),
            connection: "usb".into(),
        }
    }

    pub async fn hello(&mut self, install: &str, app: &str, token: &[u8]) -> wire::HelloAck {
        self.send(Msg::Hello(Self::hello_msg(install, app, token)))
            .await;
        match self.next().await {
            Some(Msg::HelloAck(a)) => a,
            other => panic!("expected HelloAck, got {other:?}"),
        }
    }

    /// Run the pairing exchange; the "person" allows or denies once the
    /// prompt shows the same code. Returns the code and the result.
    pub async fn pair(&mut self, sdk: &Sdk, allow: bool) -> (String, wire::PairResult) {
        let nonce = [7u8 ^ self.seq as u8; 32];
        self.seq += 1;
        self.send(Msg::PairRequest(wire::PairRequest {
            commit: crate::pairing::commit_of(&nonce).to_vec(),
        }))
        .await;
        let ch = match self.next().await {
            Some(Msg::PairChallenge(c)) => c,
            other => panic!("expected PairChallenge, got {other:?}"),
        };
        self.send(Msg::PairReveal(wire::PairReveal {
            client_nonce: nonce.to_vec(),
        }))
        .await;
        let code = crate::pairing::pairing_code(&self.fingerprint, &nonce, &ch.server_nonce);
        let mut prompt = None;
        for _ in 0..200 {
            if let Some(p) = sdk.pending().into_iter().next() {
                prompt = Some(p);
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let prompt = prompt.expect("a pairing prompt");
        assert_eq!(
            prompt.code, code,
            "desktop and device must show the same code"
        );
        if allow {
            assert!(sdk.allow(prompt.id));
        } else {
            assert!(sdk.deny(prompt.id));
        }
        match self.next().await {
            Some(Msg::PairResult(r)) => (code, r),
            other => panic!("expected PairResult, got {other:?}"),
        }
    }

    /// Pair and let someone else decide: calls `on_code` with the six
    /// digits, then waits for the answer.
    pub async fn pair_wait(&mut self, on_code: impl FnOnce(&str)) -> (String, wire::PairResult) {
        let nonce: [u8; 32] = crate::pairing::random_bytes();
        self.send(Msg::PairRequest(wire::PairRequest {
            commit: crate::pairing::commit_of(&nonce).to_vec(),
        }))
        .await;
        let ch = match self.next().await {
            Some(Msg::PairChallenge(c)) => c,
            other => panic!("expected PairChallenge, got {other:?}"),
        };
        self.send(Msg::PairReveal(wire::PairReveal {
            client_nonce: nonce.to_vec(),
        }))
        .await;
        let code = crate::pairing::pairing_code(&self.fingerprint, &nonce, &ch.server_nonce);
        on_code(&code);
        loop {
            match self
                .recv_within(Duration::from_secs(ch.timeout_s as u64 + 5))
                .await
            {
                Some(Msg::PairResult(r)) => return (code, r),
                Some(Msg::Ping(p)) => {
                    self.send(Msg::Pong(wire::Pong {
                        echo_ms: p.time_ms,
                        time_ms: now_ms(),
                    }))
                    .await
                }
                other => panic!("expected PairResult, got {other:?}"),
            }
        }
    }

    pub async fn expect_closed(&mut self) {
        loop {
            match self.recv_within(Duration::from_secs(3)).await {
                None => return,
                Some(Msg::Ping(_)) | Some(Msg::Pong(_)) | Some(Msg::Error(_))
                | Some(Msg::Config(_)) => continue,
                Some(other) => panic!("expected the connection to close, got {other:?}"),
            }
        }
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

pub fn call_start(id: &str, url: &str) -> wire::CallStart {
    wire::CallStart {
        call_id: id.into(),
        started_at_ms: now_ms(),
        method: "GET".into(),
        url: url.into(),
        client_stack: "okhttp".into(),
        ..Default::default()
    }
}

pub fn complete(id: &str) -> wire::CallComplete {
    wire::CallComplete {
        call_id: id.into(),
        ended_at_ms: now_ms(),
        response_body_available: true,
        request_body_available: true,
        ..Default::default()
    }
}
