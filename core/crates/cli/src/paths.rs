use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// `~/Library/Application Support/proxy` on macOS, otherwise
/// `$XDG_DATA_HOME/proxy`, defaulting to `~/.local/share/proxy`.
pub fn data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("PROXY_DATA_DIR") {
        return PathBuf::from(p);
    }
    if cfg!(target_os = "macos") {
        return home().join("Library/Application Support/proxy");
    }
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".local/share"))
        .join("proxy")
}

/// `$TMPDIR/proxy/api.sock` on macOS (the per-user temp dir keeps the path
/// under the 104-byte sun_path limit), otherwise `$XDG_RUNTIME_DIR/proxy/api.sock`,
/// with a per-user fallback.
pub fn socket_path() -> PathBuf {
    if let Some(p) = std::env::var_os("PROXY_SOCKET") {
        return PathBuf::from(p);
    }
    let runtime = if cfg!(target_os = "macos") {
        std::env::var_os("TMPDIR")
    } else {
        std::env::var_os("XDG_RUNTIME_DIR")
    };
    match runtime.map(PathBuf::from) {
        Some(d) if d.is_absolute() => d.join("proxy").join("api.sock"),
        _ => {
            // SAFETY: getuid has no preconditions.
            let uid = unsafe { libc::getuid() };
            PathBuf::from(format!("/tmp/proxy-{uid}")).join("api.sock")
        }
    }
}
