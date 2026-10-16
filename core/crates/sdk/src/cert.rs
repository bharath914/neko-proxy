//! The self-signed certificate of the ingest listener. It is separate from
//! the proxy CA and is never trusted through a CA: clients pin its SHA-256
//! fingerprint during pairing.

use rcgen::{CertificateParams, DnType, KeyPair, PKCS_ECDSA_P256_SHA256};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const CERT_FILE: &str = "sdk-ingest.pem";
pub const KEY_FILE: &str = "sdk-ingest.key";

#[derive(Clone)]
pub struct IngestCert {
    der: Vec<u8>,
    key_der: Vec<u8>,
    fingerprint: [u8; 32],
    dir: PathBuf,
}

impl IngestCert {
    /// Load the certificate from `dir`, creating it on first run. The
    /// fingerprint is stable across restarts.
    pub fn load_or_create(dir: &Path) -> io::Result<IngestCert> {
        std::fs::create_dir_all(dir)?;
        let cert_path = dir.join(CERT_FILE);
        let key_path = dir.join(KEY_FILE);
        if cert_path.exists() && key_path.exists() {
            if let Some(c) = Self::read(dir, &cert_path, &key_path) {
                return Ok(c);
            }
        }
        Self::generate(dir)
    }

    /// Replace the certificate. Every paired SDK pins the old fingerprint,
    /// so every pairing has to be redone. Only call on explicit user action.
    pub fn regenerate(dir: &Path) -> io::Result<IngestCert> {
        Self::generate(dir)
    }

    fn read(dir: &Path, cert_path: &Path, key_path: &Path) -> Option<IngestCert> {
        let pem = std::fs::read_to_string(cert_path).ok()?;
        let key_pem = std::fs::read_to_string(key_path).ok()?;
        let der = pem_to_der(&pem, "CERTIFICATE")?;
        let key = KeyPair::from_pem(&key_pem).ok()?;
        Some(IngestCert {
            fingerprint: Sha256::digest(&der).into(),
            der,
            key_der: key.serialize_der(),
            dir: dir.to_path_buf(),
        })
    }

    fn generate(dir: &Path) -> io::Result<IngestCert> {
        let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).map_err(io::Error::other)?;
        let mut params =
            CertificateParams::new(vec!["proxy-ingest.local".to_string()]).map_err(io::Error::other)?;
        params
            .distinguished_name
            .push(DnType::CommonName, "Proxy SDK ingest");
        // Valid for a long time: the pin, not expiry, is what is checked.
        params.not_before = rcgen::date_time_ymd(2024, 1, 1);
        params.not_after = rcgen::date_time_ymd(2124, 1, 1);
        let cert = params.self_signed(&key).map_err(io::Error::other)?;
        write_private(&dir.join(KEY_FILE), key.serialize_pem().as_bytes())?;
        std::fs::write(dir.join(CERT_FILE), cert.pem())?;
        let der = cert.der().to_vec();
        Ok(IngestCert {
            fingerprint: Sha256::digest(&der).into(),
            der,
            key_der: key.serialize_der(),
            dir: dir.to_path_buf(),
        })
    }

    pub fn fingerprint(&self) -> [u8; 32] {
        self.fingerprint
    }

    pub fn fingerprint_hex(&self) -> String {
        hex(&self.fingerprint)
    }

    /// First 16 hex characters, as advertised over mDNS.
    pub fn fingerprint_short(&self) -> String {
        self.fingerprint_hex()[..16].to_string()
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn server_config(&self) -> Result<Arc<rustls::ServerConfig>, rustls::Error> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let cfg = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(self.der.clone())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key_der.clone())),
            )?;
        Ok(Arc::new(cfg))
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn pem_to_der(pem: &str, label: &str) -> Option<Vec<u8>> {
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let start = pem.find(&begin)? + begin.len();
    let stop = pem.find(&end)?;
    let b64: String = pem[start..stop].chars().filter(|c| !c.is_whitespace()).collect();
    base64_decode(&b64)
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return None,
        } as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(unix)]
fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(data)
}

#[cfg(not(unix))]
fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
    std::fs::write(path, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_stable_across_loads_and_changes_on_regenerate() {
        let dir = tempfile::tempdir().unwrap();
        let a = IngestCert::load_or_create(dir.path()).unwrap();
        let b = IngestCert::load_or_create(dir.path()).unwrap();
        assert_eq!(a.fingerprint(), b.fingerprint());
        assert_eq!(a.fingerprint_hex().len(), 64);
        assert_eq!(a.fingerprint_short().len(), 16);
        a.server_config().unwrap();
        let c = IngestCert::regenerate(dir.path()).unwrap();
        assert_ne!(a.fingerprint(), c.fingerprint());
        let d = IngestCert::load_or_create(dir.path()).unwrap();
        assert_eq!(c.fingerprint(), d.fingerprint());
    }

    #[cfg(unix)]
    #[test]
    fn key_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        IngestCert::load_or_create(dir.path()).unwrap();
        let mode = std::fs::metadata(dir.path().join(KEY_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0);
    }
}
