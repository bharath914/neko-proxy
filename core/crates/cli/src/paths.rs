use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// `$XDG_DATA_HOME/proxy`, defaulting to `~/.local/share/proxy`.
pub fn data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("PROXY_DATA_DIR") {
        return PathBuf::from(p);
    }
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".local/share"))
        .join("proxy")
}

/// `$XDG_RUNTIME_DIR/proxy/api.sock`, with a per-user fallback.
pub fn socket_path() -> PathBuf {
    if let Some(p) = std::env::var_os("PROXY_SOCKET") {
        return PathBuf::from(p);
    }
    match std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
        Some(d) if d.is_absolute() => d.join("proxy").join("api.sock"),
        _ => {
            // SAFETY: getuid has no preconditions.
            let uid = unsafe { libc::getuid() };
            PathBuf::from(format!("/tmp/proxy-{uid}")).join("api.sock")
        }
    }
}
