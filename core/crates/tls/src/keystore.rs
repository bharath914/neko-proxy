//! CA key storage: the login Keychain on macOS, libsecret through `secret-tool`
//! elsewhere when available, else a file with mode 0600. `secret-tool` is used
//! instead of linking libsecret so the core stays a single static-ish binary
//! with no desktop dependencies.

#[cfg(not(target_os = "macos"))]
use std::io::Read;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
#[cfg(not(target_os = "macos"))]
use std::process::{Command, Stdio};
#[cfg(not(target_os = "macos"))]
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyStorage {
    /// Keyring when available, otherwise a file.
    Auto,
    /// Always a file in the data directory.
    File,
    /// Keyring only. Fails if it is unavailable.
    Keyring,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyStorageKind {
    Keyring,
    File,
}

impl KeyStorageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            KeyStorageKind::Keyring => "keyring",
            KeyStorageKind::File => "file",
        }
    }
}

#[cfg(not(target_os = "macos"))]
const SECRET_TOOL_TIMEOUT: Duration = Duration::from_secs(4);

pub(crate) fn load(dir: &Path, storage: KeyStorage) -> Option<(String, KeyStorageKind)> {
    if let Ok(pem) = std::fs::read_to_string(dir.join("ca.key")) {
        if pem.contains("PRIVATE KEY") {
            return Some((pem, KeyStorageKind::File));
        }
    }
    if storage != KeyStorage::File {
        if let Some(pem) = keyring_lookup(dir) {
            return Some((pem, KeyStorageKind::Keyring));
        }
    }
    None
}

pub(crate) fn save(dir: &Path, storage: KeyStorage, pem: &str) -> std::io::Result<KeyStorageKind> {
    if storage != KeyStorage::File {
        if keyring_store(dir, pem) {
            // A stale file from a previous install must not shadow the keyring.
            let _ = std::fs::remove_file(dir.join("ca.key"));
            return Ok(KeyStorageKind::Keyring);
        }
        if storage == KeyStorage::Keyring {
            return Err(std::io::Error::other("keyring unavailable"));
        }
    }
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(dir.join("ca.key"))?;
    f.write_all(pem.as_bytes())?;
    Ok(KeyStorageKind::File)
}

#[cfg(not(target_os = "macos"))]
fn attrs(dir: &Path) -> [String; 4] {
    [
        "service".into(),
        "proxy-ca".into(),
        "dir".into(),
        dir.to_string_lossy().into_owned(),
    ]
}

#[cfg(not(target_os = "macos"))]
fn keyring_available() -> bool {
    std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some() && which("secret-tool")
}

#[cfg(not(target_os = "macos"))]
fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
fn keyring_lookup(dir: &Path) -> Option<String> {
    if !keyring_available() {
        return None;
    }
    let mut child = Command::new("secret-tool")
        .arg("lookup")
        .args(attrs(dir))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut out = String::new();
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let _ = stdout.read_to_string(&mut out);
        out
    });
    if !wait_with_timeout(&mut child) {
        return None;
    }
    let out = reader.join().ok()?;
    out.contains("PRIVATE KEY").then_some(out)
}

#[cfg(not(target_os = "macos"))]
fn keyring_store(dir: &Path, pem: &str) -> bool {
    if !keyring_available() {
        return false;
    }
    let child = Command::new("secret-tool")
        .arg("store")
        .arg("--label=Proxy local CA key")
        .args(attrs(dir))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else { return false };
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(pem.as_bytes()).is_err() {
            return false;
        }
    }
    wait_with_timeout(&mut child)
}

#[cfg(not(target_os = "macos"))]
fn wait_with_timeout(child: &mut std::process::Child) -> bool {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if start.elapsed() < SECRET_TOOL_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "proxy-ca";

#[cfg(target_os = "macos")]
fn keyring_lookup(dir: &Path) -> Option<String> {
    let bytes =
        security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, &dir.to_string_lossy())
            .ok()?;
    let pem = String::from_utf8(bytes).ok()?;
    pem.contains("PRIVATE KEY").then_some(pem)
}

#[cfg(target_os = "macos")]
fn keyring_store(dir: &Path, pem: &str) -> bool {
    security_framework::passwords::set_generic_password(
        KEYCHAIN_SERVICE,
        &dir.to_string_lossy(),
        pem.as_bytes(),
    )
    .is_ok()
}
