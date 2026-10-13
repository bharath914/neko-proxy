//! Per-install CA, key storage and leaf certificate minting.
//!
//! The CA key is kept in the desktop keyring (libsecret, through
//! `secret-tool`) when one is available, otherwise in `ca.key` with mode 0600.
//! The CA certificate is always written to `ca.pem` in the data directory so
//! trust helpers have something to install.

mod keystore;

use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose,
};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use time::{Duration, OffsetDateTime};

pub use keystore::{KeyStorage, KeyStorageKind};

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("certificate generation failed: {0}")]
    Generate(#[from] rcgen::Error),
    #[error("tls configuration failed: {0}")]
    Config(#[from] rustls::Error),
    #[error("io error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

const LEAF_CACHE_CAP: usize = 2048;

/// The proxy certificate authority and its leaf certificate cache.
pub struct CaStore {
    issuer: Issuer<'static, KeyPair>,
    cert_der: CertificateDer<'static>,
    cert_pem: String,
    cert_path: PathBuf,
    storage: KeyStorageKind,
    subject: String,
    not_after: String,
    sha256: String,
    spki_sha256: String,
    leaves: Mutex<HashMap<String, Arc<ServerConfig>>>,
}

impl std::fmt::Debug for CaStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaStore")
            .field("cert_path", &self.cert_path)
            .field("sha256", &self.sha256)
            .finish_non_exhaustive()
    }
}

impl CaStore {
    /// Load the CA from `dir`, creating it on first run.
    pub fn load_or_create(dir: &Path, storage: KeyStorage) -> Result<CaStore, TlsError> {
        std::fs::create_dir_all(dir).map_err(|source| TlsError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let cert_path = dir.join("ca.pem");

        if let Ok(pem) = std::fs::read_to_string(&cert_path) {
            if let Some((key_pem, kind)) = keystore::load(dir, storage) {
                match KeyPair::from_pem(&key_pem)
                    .map_err(TlsError::from)
                    .and_then(|key| Self::from_parts(&pem, key, cert_path.clone(), kind))
                {
                    Ok(ca) => return Ok(ca),
                    Err(e) => tracing::warn!("existing CA unusable, regenerating: {e}"),
                }
            } else {
                tracing::warn!("CA certificate present but its key is missing, regenerating");
            }
        }
        Self::create(dir, storage, cert_path)
    }

    fn create(dir: &Path, storage: KeyStorage, cert_path: PathBuf) -> Result<CaStore, TlsError> {
        let key = KeyPair::generate()?;
        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(
            DnType::CommonName,
            format!("Proxy Local CA ({})", host_label()),
        );
        dn.push(DnType::OrganizationName, "Proxy");
        params.distinguished_name = dn;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let now = OffsetDateTime::now_utc();
        params.not_before = now - Duration::days(1);
        params.not_after = now + Duration::days(3650);
        let cert = params.self_signed(&key)?;
        let pem = cert.pem();

        std::fs::write(&cert_path, &pem).map_err(|source| TlsError::Io {
            path: cert_path.clone(),
            source,
        })?;
        let kind =
            keystore::save(dir, storage, &key.serialize_pem()).map_err(|source| TlsError::Io {
                path: dir.join("ca.key"),
                source,
            })?;
        Self::from_parts(&pem, key, cert_path, kind)
    }

    fn from_parts(
        pem: &str,
        key: KeyPair,
        cert_path: PathBuf,
        storage: KeyStorageKind,
    ) -> Result<CaStore, TlsError> {
        let issuer = Issuer::from_ca_cert_pem(pem, key)?;
        let der = CertificateDer::from_pem_slice(pem.as_bytes())
            .map_err(|_| TlsError::Generate(rcgen::Error::CouldNotParseCertificate))?;
        let sha256 = hex(&Sha256::digest(der.as_ref()));
        let (subject, not_after) = describe(der.as_ref());
        let spki_sha256 = spki_hash(der.as_ref());
        Ok(CaStore {
            issuer,
            cert_der: der,
            cert_pem: pem.to_string(),
            cert_path,
            storage,
            subject,
            not_after,
            sha256,
            spki_sha256,
            leaves: Mutex::new(HashMap::new()),
        })
    }

    pub fn cert_path(&self) -> &Path {
        &self.cert_path
    }

    pub fn cert_pem(&self) -> &str {
        &self.cert_pem
    }

    pub fn cert_der(&self) -> &CertificateDer<'static> {
        &self.cert_der
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Base64 SHA-256 of the CA's SubjectPublicKeyInfo, the form Chromium's
    /// `--ignore-certificate-errors-spki-list` expects.
    pub fn spki_sha256(&self) -> &str {
        &self.spki_sha256
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn not_after(&self) -> &str {
        &self.not_after
    }

    pub fn key_storage(&self) -> KeyStorageKind {
        self.storage
    }

    /// A server config presenting a certificate for `host`, cached per host.
    /// ALPN offers h2 and http/1.1.
    pub fn server_config(&self, host: &str) -> Result<Arc<ServerConfig>, TlsError> {
        let host = host
            .trim_matches(|c| c == '[' || c == ']')
            .to_ascii_lowercase();
        if let Some(cfg) = self.leaves.lock().unwrap().get(&host) {
            return Ok(cfg.clone());
        }
        let cfg = Arc::new(self.mint(&host)?);
        let mut cache = self.leaves.lock().unwrap();
        if cache.len() >= LEAF_CACHE_CAP {
            cache.clear();
        }
        cache.insert(host, cfg.clone());
        Ok(cfg)
    }

    fn mint(&self, host: &str) -> Result<ServerConfig, TlsError> {
        let key = KeyPair::generate()?;
        let mut params = CertificateParams::new(vec![host.to_string()])?;
        let mut dn = DistinguishedName::new();
        if host.len() <= 64 {
            dn.push(DnType::CommonName, host);
        }
        params.distinguished_name = dn;
        params.is_ca = IsCa::NoCa;
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let now = OffsetDateTime::now_utc();
        params.not_before = now - Duration::days(1);
        params.not_after = now + Duration::days(300);
        let cert = params.signed_by(&key, &self.issuer)?;

        let chain = vec![cert.der().clone()];
        let key_der = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(key.serialize_der()));
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut cfg = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_no_client_auth()
            .with_single_cert(chain, key_der)?;
        cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        Ok(cfg)
    }
}

fn host_label() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "localhost".into())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn spki_hash(der: &[u8]) -> String {
    use base64::Engine;
    let Ok((_, cert)) = x509_parser::parse_x509_certificate(der) else {
        return String::new();
    };
    let digest = Sha256::digest(cert.tbs_certificate.subject_pki.raw);
    base64::engine::general_purpose::STANDARD.encode(digest)
}

/// Subject common name and expiry (RFC 3339) of a DER certificate.
fn describe(der: &[u8]) -> (String, String) {
    let Ok((_, cert)) = x509_parser::parse_x509_certificate(der) else {
        return (String::new(), String::new());
    };
    let subject = cert
        .subject()
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .unwrap_or_default()
        .to_string();
    let not_after = cert
        .validity()
        .not_after
        .to_datetime()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    (subject, not_after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::pki_types::ServerName;
    use rustls::{ClientConfig, ClientConnection, RootCertStore, ServerConnection};

    fn handshake(ca: &CaStore, name: &str, trusted: bool) -> Result<(), rustls::Error> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut roots = RootCertStore::empty();
        if trusted {
            roots.add(ca.cert_der().clone()).unwrap();
        }
        let client_cfg = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let server_cfg = ca.server_config(name).unwrap();
        let mut client = ClientConnection::new(
            Arc::new(client_cfg),
            ServerName::try_from(name.to_string()).unwrap(),
        )?;
        let mut server = ServerConnection::new(server_cfg)?;
        for _ in 0..20 {
            let mut buf = Vec::new();
            while client.wants_write() {
                client.write_tls(&mut buf).unwrap();
            }
            if !buf.is_empty() {
                server.read_tls(&mut &buf[..]).unwrap();
                server.process_new_packets()?;
            }
            let mut buf = Vec::new();
            while server.wants_write() {
                server.write_tls(&mut buf).unwrap();
            }
            if !buf.is_empty() {
                client.read_tls(&mut &buf[..]).unwrap();
                client.process_new_packets()?;
            }
            if !client.is_handshaking() && !server.is_handshaking() {
                return Ok(());
            }
        }
        panic!("handshake did not finish");
    }

    #[test]
    fn mints_trusted_leaf_for_dns_and_ip() {
        let dir = tempfile::tempdir().unwrap();
        let ca = CaStore::load_or_create(dir.path(), KeyStorage::File).unwrap();
        handshake(&ca, "example.com", true).unwrap();
        handshake(&ca, "127.0.0.1", true).unwrap();
        assert!(handshake(&ca, "example.com", false).is_err());
    }

    #[test]
    fn ca_persists_across_loads() {
        let dir = tempfile::tempdir().unwrap();
        let a = CaStore::load_or_create(dir.path(), KeyStorage::File).unwrap();
        let b = CaStore::load_or_create(dir.path(), KeyStorage::File).unwrap();
        assert_eq!(a.sha256(), b.sha256());
        assert_eq!(a.key_storage(), KeyStorageKind::File);
        assert!(a.subject().starts_with("Proxy Local CA"));
        assert!(a.not_after().starts_with("20"));
        assert_eq!(a.spki_sha256().len(), 44);
        // Leaves minted by the reloaded CA chain to the same root.
        handshake(&b, "api.example.com", true).unwrap();
    }

    #[test]
    fn key_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let _ = CaStore::load_or_create(dir.path(), KeyStorage::File).unwrap();
        let mode = std::fs::metadata(dir.path().join("ca.key"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
