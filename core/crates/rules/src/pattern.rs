//! Host patterns: literal names with `*` standing for any run of characters.

/// Case-insensitive glob over a host name. `*` matches any run, including
/// dots, so `*.example.com` matches `a.b.example.com` but not `example.com`.
pub fn host_matches(pattern: &str, host: &str) -> bool {
    glob(
        pattern.to_ascii_lowercase().as_bytes(),
        host.to_ascii_lowercase().as_bytes(),
    )
}

/// An SSL proxying entry: a host pattern with an optional `:port`.
pub fn ssl_entry_matches(entry: &str, host: &str, port: u16) -> bool {
    let (pat, want_port) = match entry.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => {
            (h, p.parse::<u16>().ok())
        }
        _ => (entry, None),
    };
    if let Some(p) = want_port {
        if p != port {
            return false;
        }
    }
    host_matches(pat, host)
}

fn glob(p: &[u8], s: &[u8]) -> bool {
    // Iterative match with single-star backtracking; linear for sane inputs.
    let (mut pi, mut si) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while si < s.len() {
        if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            mark = si;
            pi += 1;
        } else if pi < p.len() && p[pi] == s[si] {
            pi += 1;
            si += 1;
        } else if let Some(st) = star {
            pi = st + 1;
            mark += 1;
            si = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(host_matches("example.com", "EXAMPLE.com"));
        assert!(host_matches("*.example.com", "a.b.example.com"));
        assert!(!host_matches("*.example.com", "example.com"));
        assert!(host_matches("api.*", "api.example.org"));
        assert!(host_matches("*", "anything"));
        assert!(host_matches("a*b*c", "aXXbYYc"));
        assert!(!host_matches("a*b*c", "aXXbYY"));
        assert!(!host_matches("", "x"));
        assert!(host_matches("", ""));
    }

    #[test]
    fn entries_with_ports() {
        assert!(ssl_entry_matches("example.com", "example.com", 443));
        assert!(ssl_entry_matches("example.com", "example.com", 8443));
        assert!(ssl_entry_matches("example.com:8443", "example.com", 8443));
        assert!(!ssl_entry_matches("example.com:8443", "example.com", 443));
        assert!(ssl_entry_matches("*.test:443", "a.test", 443));
    }
}
