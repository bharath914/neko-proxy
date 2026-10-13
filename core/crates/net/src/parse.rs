//! Request-target parsing. Pure functions over strings so they can be fuzzed.

use http::{Method, Uri};

/// Where a request is going.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub scheme: &'static str,
    /// Host without brackets.
    pub host: String,
    pub port: u16,
    /// Path and query, always starting with `/` (or `*` for OPTIONS *).
    pub path_and_query: String,
}

impl Target {
    pub fn authority(&self) -> String {
        let default = if self.scheme == "https" { 443 } else { 80 };
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == default {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }

    pub fn path(&self) -> &str {
        self.path_and_query
            .split('?')
            .next()
            .unwrap_or(&self.path_and_query)
    }

    pub fn query(&self) -> &str {
        self.path_and_query
            .split_once('?')
            .map(|(_, q)| q)
            .unwrap_or("")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetError {
    /// No absolute URI and no usable Host header.
    NoHost,
    UnsupportedScheme,
    BadAuthority,
}

/// Parse `host:port`, `[v6]:port` or a bare host into `(host, port)`.
/// `default_port` applies when the port is missing.
pub fn parse_authority(s: &str, default_port: u16) -> Option<(String, u16)> {
    let s = s.trim();
    if s.is_empty() || s.len() > 255 || s.bytes().any(|b| b <= b' ' || b == b'/' || b == b'@') {
        return None;
    }
    let (host, port) = if let Some(rest) = s.strip_prefix('[') {
        let (h, tail) = rest.split_once(']')?;
        if h.is_empty() {
            return None;
        }
        match tail {
            "" => (h, default_port),
            t => (h, t.strip_prefix(':')?.parse().ok()?),
        }
    } else {
        match s.rsplit_once(':') {
            Some((h, p)) if !h.contains(':') => (h, p.parse().ok()?),
            Some(_) => return None, // bare IPv6 without brackets
            None => (s, default_port),
        }
    };
    if host.is_empty() || port == 0 {
        return None;
    }
    Some((host.to_ascii_lowercase(), port))
}

/// CONNECT authority. The port defaults to 443.
pub fn parse_connect_authority(s: &str) -> Option<(String, u16)> {
    parse_authority(s, 443)
}

/// Resolve the target of a request that arrived on a plain proxy connection.
/// Accepts absolute-form and falls back to the Host header.
pub fn resolve_plain(
    method: &Method,
    uri: &Uri,
    host_header: Option<&str>,
) -> Result<Target, TargetError> {
    let path_and_query = path_of(method, uri);
    if let Some(scheme) = uri.scheme_str() {
        let scheme = scheme_static(scheme)?;
        let auth = uri.authority().ok_or(TargetError::BadAuthority)?;
        let default = if scheme == "https" { 443 } else { 80 };
        let (host, port) =
            parse_authority(auth.as_str(), default).ok_or(TargetError::BadAuthority)?;
        return Ok(Target {
            scheme,
            host,
            port,
            path_and_query,
        });
    }
    let h = host_header.ok_or(TargetError::NoHost)?;
    let (host, port) = parse_authority(h, 80).ok_or(TargetError::BadAuthority)?;
    Ok(Target {
        scheme: "http",
        host,
        port,
        path_and_query,
    })
}

/// Resolve the target of a request inside a decrypted CONNECT tunnel.
pub fn resolve_tunneled(method: &Method, uri: &Uri, host: &str, port: u16) -> Target {
    Target {
        scheme: "https",
        host: host.to_string(),
        port,
        path_and_query: path_of(method, uri),
    }
}

fn path_of(_method: &Method, uri: &Uri) -> String {
    match uri.path_and_query() {
        Some(pq) if !pq.as_str().is_empty() => pq.as_str().to_string(),
        _ => "/".to_string(),
    }
}

fn scheme_static(s: &str) -> Result<&'static str, TargetError> {
    if s.eq_ignore_ascii_case("http") {
        Ok("http")
    } else if s.eq_ignore_ascii_case("https") {
        Ok("https")
    } else {
        Err(TargetError::UnsupportedScheme)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_forms() {
        assert_eq!(
            parse_connect_authority("Example.com:8443"),
            Some(("example.com".into(), 8443))
        );
        assert_eq!(
            parse_connect_authority("example.com"),
            Some(("example.com".into(), 443))
        );
        assert_eq!(
            parse_connect_authority("[::1]:8080"),
            Some(("::1".into(), 8080))
        );
        assert_eq!(parse_connect_authority("::1"), None);
        assert_eq!(parse_connect_authority(""), None);
        assert_eq!(parse_connect_authority("a b:1"), None);
        assert_eq!(parse_connect_authority("host:0"), None);
        assert_eq!(parse_connect_authority("host:99999"), None);
        assert_eq!(parse_connect_authority("user@host:1"), None);
    }

    #[test]
    fn plain_targets() {
        let t = resolve_plain(
            &Method::GET,
            &"http://example.com:8080/a?b=1".parse().unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(t.host, "example.com");
        assert_eq!(t.port, 8080);
        assert_eq!(t.path(), "/a");
        assert_eq!(t.query(), "b=1");
        assert_eq!(t.authority(), "example.com:8080");

        let t = resolve_plain(&Method::GET, &"/x".parse().unwrap(), Some("h.test")).unwrap();
        assert_eq!((t.host.as_str(), t.port), ("h.test", 80));

        assert_eq!(
            resolve_plain(&Method::GET, &"/x".parse().unwrap(), None),
            Err(TargetError::NoHost)
        );
        assert_eq!(
            resolve_plain(&Method::GET, &"ftp://h/x".parse().unwrap(), None),
            Err(TargetError::UnsupportedScheme)
        );
    }

    #[test]
    fn tunneled_targets() {
        let t = resolve_tunneled(&Method::GET, &"/p?q=1".parse().unwrap(), "api.test", 443);
        assert_eq!(t.authority(), "api.test");
        assert_eq!(t.path_and_query, "/p?q=1");
        let t = resolve_tunneled(
            &Method::GET,
            &"https://api.test/p".parse().unwrap(),
            "api.test",
            8443,
        );
        assert_eq!(t.authority(), "api.test:8443");
        assert_eq!(t.path_and_query, "/p");
    }
}
