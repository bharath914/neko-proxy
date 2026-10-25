//! `Set-Cookie` headers of a response, for the Cookies tab.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires: String,
    pub max_age: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: String,
}

/// One cookie per `Set-Cookie` header.
///
/// Each header is parsed on its own: `Expires` holds a comma
/// ("Wed, 21 Oct 2026 ..."), so nothing is ever split on commas.
pub fn parse_set_cookies(headers: &[(String, String)]) -> Vec<Cookie> {
    headers
        .iter()
        .filter(|(k, _)| k.trim().eq_ignore_ascii_case("set-cookie"))
        .filter_map(|(_, v)| parse_one(v))
        .collect()
}

fn parse_one(header: &str) -> Option<Cookie> {
    let header = header.trim();
    if header.is_empty() {
        return None;
    }
    let mut parts = header.split(';');
    let first = parts.next().unwrap_or("").trim();
    let (name, value) = match first.split_once('=') {
        Some((n, v)) => (n.trim(), v.trim()),
        None => (first, ""),
    };
    if name.is_empty() && value.is_empty() {
        return None;
    }
    let mut c = Cookie {
        name: name.to_string(),
        value: value.to_string(),
        ..Cookie::default()
    };
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (key, val) = match part.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => (part, ""),
        };
        match key.to_ascii_lowercase().as_str() {
            "domain" => c.domain = val.to_string(),
            "path" => c.path = val.to_string(),
            "expires" => c.expires = val.to_string(),
            "max-age" => c.max_age = val.to_string(),
            "secure" => c.secure = true,
            "httponly" => c.http_only = true,
            "samesite" => c.same_site = val.to_string(),
            _ => {}
        }
    }
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn full_cookie() {
        let hs = h(&[(
            "Set-Cookie",
            "sid=abc123; Domain=example.com; Path=/app; Expires=Wed, 21 Oct 2026 07:28:00 GMT; Max-Age=3600; Secure; HttpOnly; SameSite=Lax",
        )]);
        let c = parse_set_cookies(&hs);
        assert_eq!(c.len(), 1);
        assert_eq!(
            c[0],
            Cookie {
                name: "sid".into(),
                value: "abc123".into(),
                domain: "example.com".into(),
                path: "/app".into(),
                expires: "Wed, 21 Oct 2026 07:28:00 GMT".into(),
                max_age: "3600".into(),
                secure: true,
                http_only: true,
                same_site: "Lax".into(),
            }
        );
    }

    #[test]
    fn name_and_value_only() {
        let c = parse_set_cookies(&h(&[("set-cookie", "a=b")]));
        assert_eq!(c[0].name, "a");
        assert_eq!(c[0].value, "b");
        assert!(!c[0].secure && !c[0].http_only);
        assert_eq!(c[0].path, "");
    }

    #[test]
    fn header_name_case_and_other_headers_ignored() {
        let hs = h(&[
            ("Content-Type", "text/plain"),
            ("SET-COOKIE", "a=1"),
            ("Set-Cookie2", "x=1"),
            ("set-cookie", "b=2"),
        ]);
        let c = parse_set_cookies(&hs);
        assert_eq!(c.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    }

    #[test]
    fn attribute_names_case_insensitive() {
        let c = parse_set_cookies(&h(&[(
            "Set-Cookie",
            "a=1; PATH=/x; domain=.e.com; MAX-AGE=5; SECURE; httponly; samesite=Strict",
        )]));
        let c = &c[0];
        assert_eq!(c.path, "/x");
        assert_eq!(c.domain, ".e.com");
        assert_eq!(c.max_age, "5");
        assert!(c.secure && c.http_only);
        assert_eq!(c.same_site, "Strict");
    }

    #[test]
    fn no_equals_uses_whole_part_as_name() {
        let c = parse_set_cookies(&h(&[("Set-Cookie", "flag; Secure")]));
        assert_eq!(c[0].name, "flag");
        assert_eq!(c[0].value, "");
        assert!(c[0].secure);
    }

    #[test]
    fn quoted_value_kept_and_value_with_equals() {
        let c = parse_set_cookies(&h(&[
            ("Set-Cookie", "q=\"a b\"; Path=/"),
            ("Set-Cookie", "tok=abc==; HttpOnly"),
        ]));
        assert_eq!(c[0].value, "\"a b\"");
        assert_eq!(c[1].value, "abc==");
    }

    #[test]
    fn empty_headers_skipped() {
        let c = parse_set_cookies(&h(&[
            ("Set-Cookie", ""),
            ("Set-Cookie", "   "),
            ("Set-Cookie", "; Path=/"),
            ("Set-Cookie", "ok=1"),
        ]));
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].name, "ok");
    }

    #[test]
    fn empty_value_and_spacing() {
        let c = parse_set_cookies(&h(&[("Set-Cookie", "  gone = ;Path=/;;  Max-Age = 0 ")]));
        assert_eq!(c[0].name, "gone");
        assert_eq!(c[0].value, "");
        assert_eq!(c[0].path, "/");
        assert_eq!(c[0].max_age, "0");
    }

    #[test]
    fn unknown_attributes_ignored_and_none() {
        let c = parse_set_cookies(&h(&[("Set-Cookie", "a=1; Priority=High; Partitioned")]));
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].value, "1");
        assert!(parse_set_cookies(&[]).is_empty());
    }
}
