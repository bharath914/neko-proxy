use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// `~/Library/Application Support/neko` on macOS, otherwise
/// `$XDG_DATA_HOME/neko`, defaulting to `~/.local/share/neko`.
pub fn data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("NEKO_DATA_DIR") {
        return PathBuf::from(p);
    }
    if cfg!(target_os = "macos") {
        return home().join("Library/Application Support/neko");
    }
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".local/share"))
        .join("neko")
}

/// `$TMPDIR/neko/api.sock` on macOS (the per-user temp dir keeps the path
/// under the 104-byte sun_path limit), otherwise `$XDG_RUNTIME_DIR/neko/api.sock`,
/// with a per-user fallback.
pub fn socket_path() -> PathBuf {
    if let Some(p) = std::env::var_os("NEKO_SOCKET") {
        return PathBuf::from(p);
    }
    let runtime = if cfg!(target_os = "macos") {
        std::env::var_os("TMPDIR")
    } else {
        std::env::var_os("XDG_RUNTIME_DIR")
    };
    match runtime.map(PathBuf::from) {
        Some(d) if d.is_absolute() => d.join("neko").join("api.sock"),
        _ => {
            // SAFETY: getuid has no preconditions.
            let uid = unsafe { libc::getuid() };
            PathBuf::from(format!("/tmp/neko-{uid}")).join("api.sock")
        }
    }
}
