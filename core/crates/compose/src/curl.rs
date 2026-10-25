//! Requests from cURL commands and captured flows, and back to cURL.

use crate::{AuthKind, BodyKind, Built, KeyValue, Request};
use base64::Engine;
use percent_encoding::percent_decode_str;

/// Headers a client or the proxy sets itself; they make no sense to edit.
const SKIP_HEADERS: &[&str] = &[
    "host",
    "content-length",
    "connection",
    "proxy-connection",
    "keep-alive",
    "transfer-encoding",
    "te",
    "upgrade",
    "proxy-authorization",
];

fn decode(s: &str) -> String {
    percent_decode_str(&s.replace('+', " "))
        .decode_utf8_lossy()
        .into_owned()
}

fn pairs(q: &str) -> Vec<KeyValue> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => KeyValue::new(decode(k), decode(v)),
            None => KeyValue::new(decode(p), ""),
        })
        .collect()
}

/// The URL without its query, and the query as rows.
fn split_url(url: &str) -> (String, Vec<KeyValue>) {
    let url = url.split('#').next().unwrap_or(url);
    match url.split_once('?') {
        Some((base, q)) => (base.to_string(), pairs(q)),
        None => (url.to_string(), Vec::new()),
    }
}

fn looks_like_form(s: &str) -> bool {
    !s.is_empty() && s.contains('=') && !s.chars().any(char::is_whitespace)
}

/// Pick the body editor that fits the content type, or guess from the bytes.
fn body_from(content_type: Option<&str>, body: &[u8], r: &mut Request) {
    if body.is_empty() {
        return;
    }
    let text = String::from_utf8_lossy(body).into_owned();
    let ct = content_type.unwrap_or("").to_ascii_lowercase();
    let json = || serde_json::from_slice::<serde_json::Value>(body).ok();
    if ct.contains("json")
        || (ct.is_empty() && json().is_some_and(|v| v.is_object() || v.is_array()))
    {
        r.body_kind = BodyKind::Json;
        // Pretty is easier to edit; keep the text as sent when it is not JSON.
        r.body = json()
            .and_then(|v| serde_json::to_string_pretty(&v).ok())
            .unwrap_or(text);
    } else if ct.contains("x-www-form-urlencoded") || (ct.is_empty() && looks_like_form(&text)) {
        r.body_kind = BodyKind::Form;
        r.form = pairs(&text);
    } else if ct.contains("xml") {
        r.body_kind = BodyKind::Xml;
        r.body = text;
    } else {
        r.body_kind = BodyKind::Text;
        r.body = text;
    }
}

/// A request from what was seen on the wire: the query becomes rows, an
/// Authorization header moves to the auth settings, and the body gets the
/// editor that fits it. `body` must already be content-decoded.
pub fn from_parts(method: &str, url: &str, headers: &[(String, String)], body: &[u8]) -> Request {
    import_parts(method, url, headers, body, true)
}

fn import_parts(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
    decoded: bool,
) -> Request {
    let (base, params) = split_url(url);
    let mut r = Request {
        method: method.to_ascii_uppercase(),
        url: base,
        params,
        ..Default::default()
    };
    let mut content_type = None;
    for (k, v) in headers {
        let lk = k.to_ascii_lowercase();
        if SKIP_HEADERS.contains(&lk.as_str()) || lk.starts_with(':') {
            continue;
        }
        // The body is stored decoded, so the old encoding no longer applies.
        if decoded && lk == "content-encoding" {
            continue;
        }
        if lk == "content-type" {
            content_type = Some(v.clone());
        }
        if lk == "authorization" && r.auth == AuthKind::None {
            if let Some(t) = v
                .strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
            {
                r.auth = AuthKind::Bearer;
                r.auth_token = t.trim().to_string();
                continue;
            }
            if let Some(b) = v
                .strip_prefix("Basic ")
                .or_else(|| v.strip_prefix("basic "))
            {
                let pair = base64::engine::general_purpose::STANDARD
                    .decode(b.trim())
                    .ok()
                    .and_then(|d| String::from_utf8(d).ok());
                if let Some((u, p)) = pair.as_deref().and_then(|p| p.split_once(':')) {
                    r.auth = AuthKind::Basic;
                    r.auth_user = u.to_string();
                    r.auth_password = p.to_string();
                    continue;
                }
            }
        }
        r.headers.push(KeyValue::new(k.clone(), v.clone()));
    }
    if !decoded
        && !body.is_empty()
        && headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-encoding"))
    {
        r.body_kind = BodyKind::Text;
        r.body = String::from_utf8_lossy(body).into_owned();
    } else {
        body_from(content_type.as_deref(), body, &mut r);
    }
    r
}

/// Split a shell command line into words: quotes, `$'…'`, backslashes and
/// line continuations as bash reads them.
fn words(s: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('\n') => {}
                Some('\r') => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                }
                Some(n) => {
                    cur.push(n);
                    in_word = true;
                }
                None => {}
            },
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(n) => cur.push(n),
                        None => return Err("a single quote is never closed".into()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(n @ ('"' | '\\' | '$' | '`')) => cur.push(n),
                            Some('\n') => {}
                            Some(n) => {
                                cur.push('\\');
                                cur.push(n);
                            }
                            None => return Err("a double quote is never closed".into()),
                        },
                        Some(n) => cur.push(n),
                        None => return Err("a double quote is never closed".into()),
                    }
                }
            }
            '$' if chars.peek() == Some(&'\'') => {
                chars.next();
                in_word = true;
                let mut raw = Vec::new();
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some('\\') => match chars.next() {
                            Some('n') => raw.push(b'\n'),
                            Some('t') => raw.push(b'\t'),
                            Some('r') => raw.push(b'\r'),
                            Some('x') => {
                                let hex: String = (0..2)
                                    .filter_map(|_| chars.next_if(|c| c.is_ascii_hexdigit()))
                                    .collect();
                                if let Ok(b) = u8::from_str_radix(&hex, 16) {
                                    raw.push(b);
                                }
                            }
                            Some('u') => {
                                let hex: String = (0..4)
                                    .filter_map(|_| chars.next_if(|c| c.is_ascii_hexdigit()))
                                    .collect();
                                if let Some(ch) =
                                    u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                                {
                                    raw.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
                                }
                            }
                            Some(n) => raw.extend_from_slice(n.encode_utf8(&mut [0; 4]).as_bytes()),
                            None => return Err("a $'…' quote is never closed".into()),
                        },
                        Some(n) => raw.extend_from_slice(n.encode_utf8(&mut [0; 4]).as_bytes()),
                        None => return Err("a $'…' quote is never closed".into()),
                    }
                }
                cur.push_str(&String::from_utf8_lossy(&raw));
            }
            c if c.is_whitespace() => {
                if in_word {
                    out.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                cur.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        out.push(cur);
    }
    Ok(out)
}

/// Long and short options that take a value but do not change the request.
const IGNORED_WITH_VALUE: &[&str] = &[
    "-o",
    "--output",
    "-m",
    "--max-time",
    "--connect-timeout",
    "--retry",
    "-w",
    "--write-out",
    "--cacert",
    "--capath",
    "--cert",
    "-E",
    "--key",
    "-c",
    "--cookie-jar",
    "--limit-rate",
    "--max-redirs",
    "-y",
    "-Y",
];

const ROUTING_OPTIONS: &[&str] = &[
    "--connect-to",
    "--resolve",
    "-x",
    "--proxy",
    "--proxy-user",
    "-U",
    "--unix-socket",
    "--abstract-unix-socket",
    "--interface",
];

/// Short options that take a value; the value may be glued on (`-XPOST`).
const SHORT_WITH_VALUE: &str = "XHdubAeFTomxwcEyYU";

/// Long options read below that take a value.
const LONG_WITH_VALUE: &[&str] = &[
    "--request",
    "--header",
    "--data",
    "--data-raw",
    "--data-ascii",
    "--data-binary",
    "--data-urlencode",
    "--json",
    "--user",
    "--user-agent",
    "--referer",
    "--cookie",
    "--url",
    "--form",
    "--form-string",
    "--upload-file",
];

/// Whether the next word is this option's value, to be taken as it is.
fn takes_value(flag: &str) -> bool {
    match flag.strip_prefix("--") {
        Some(_) => {
            LONG_WITH_VALUE.contains(&flag)
                || IGNORED_WITH_VALUE.contains(&flag)
                || ROUTING_OPTIONS.contains(&flag)
        }
        None => flag.len() == 2 && flag[1..].chars().all(|c| SHORT_WITH_VALUE.contains(c)),
    }
}

/// A request from a `curl …` command, as "Copy as cURL" in browsers writes it.
pub fn parse_curl(text: &str) -> Result<Request, String> {
    let mut args = words(text.trim())?;
    if args.first().map(String::as_str) == Some("curl") {
        args.remove(0);
    }
    if args.is_empty() {
        return Err("Paste a curl command, such as curl https://example.com".into());
    }
    // Normalize: `--opt=value` and `-XPOST` into separate words, bundles like
    // `-sSL` into single flags. A value is taken as it is, even when it starts
    // with `-` (a multipart body from "Copy as cURL" starts with `------`).
    let mut norm: Vec<String> = Vec::new();
    let mut want_value = false;
    for a in args {
        if std::mem::take(&mut want_value) {
            norm.push(a);
        } else if a.starts_with("--") {
            match a.split_once('=') {
                Some((k, v)) if k.len() > 2 => {
                    norm.push(k.to_string());
                    norm.push(v.to_string());
                }
                _ => {
                    want_value = takes_value(&a);
                    norm.push(a);
                }
            }
        } else if a.len() > 2 && a.starts_with('-') {
            for (i, c) in a[1..].char_indices() {
                norm.push(format!("-{c}"));
                if SHORT_WITH_VALUE.contains(c) {
                    let rest = &a[1 + i + c.len_utf8()..];
                    if rest.is_empty() {
                        want_value = true;
                    } else {
                        norm.push(rest.to_string());
                    }
                    break;
                }
            }
        } else {
            want_value = takes_value(&a);
            norm.push(a);
        }
    }

    let mut method: Option<String> = None;
    let mut url: Option<String> = None;
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut data: Vec<String> = Vec::new();
    let mut get = false;
    let mut head = false;
    let mut user: Option<String> = None;
    let mut it = norm.into_iter();
    let value = |it: &mut std::vec::IntoIter<String>, flag: &str| {
        it.next().ok_or_else(|| format!("{flag} needs a value"))
    };
    while let Some(a) = it.next() {
        match a.as_str() {
            "-X" | "--request" => method = Some(value(&mut it, &a)?),
            "-H" | "--header" => {
                let h = value(&mut it, &a)?;
                if let Some((k, v)) = h.split_once(':') {
                    headers.push((k.trim().to_string(), v.trim().to_string()));
                }
            }
            "-d" | "--data" | "--data-raw" | "--data-ascii" | "--data-binary" => {
                let d = value(&mut it, &a)?;
                if d.starts_with('@') && a != "--data-raw" {
                    return Err(format!("{a} {d} reads a file; paste the body instead"));
                }
                data.push(d);
            }
            "--data-urlencode" => {
                let d = value(&mut it, &a)?;
                let enc = |s: &str| {
                    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC)
                        .to_string()
                };
                data.push(match d.split_once('=') {
                    Some((k, v)) if !k.is_empty() => format!("{k}={}", enc(v)),
                    Some((_, v)) => enc(v),
                    None => enc(&d),
                });
            }
            "--json" => {
                data.push(value(&mut it, &a)?);
                headers.push(("Content-Type".into(), "application/json".into()));
                headers.push(("Accept".into(), "application/json".into()));
            }
            "-u" | "--user" => user = Some(value(&mut it, &a)?),
            "-A" | "--user-agent" => headers.push(("User-Agent".into(), value(&mut it, &a)?)),
            "-e" | "--referer" => headers.push(("Referer".into(), value(&mut it, &a)?)),
            "-b" | "--cookie" => {
                let c = value(&mut it, &a)?;
                if c.contains('=') {
                    headers.push(("Cookie".into(), c));
                }
            }
            "--url" => {
                if url.is_some() {
                    return Err("the command has more than one URL".into());
                }
                url = Some(value(&mut it, &a)?);
            }
            "-G" | "--get" => get = true,
            "-I" | "--head" => head = true,
            "-F" | "--form" | "--form-string" => {
                return Err("multipart forms (-F) are not supported yet".into())
            }
            "-T" | "--upload-file" => return Err("uploading a file (-T) is not supported".into()),
            f if ROUTING_OPTIONS.contains(&f) => {
                return Err(format!(
                    "{f} changes where the request goes; remove it to import"
                ));
            }
            f if IGNORED_WITH_VALUE.contains(&f) => {
                value(&mut it, f)?;
            }
            f if f.starts_with('-') && f.len() > 1 => {}
            _ => {
                if url.is_none() {
                    url = Some(a);
                } else {
                    return Err("the command has more than one URL".into());
                }
            }
        }
    }
    let mut url = url.ok_or("The command has no URL")?;
    // A leading {{variable}} usually holds the scheme; leave it to the build.
    if !url.contains("://") && !url.starts_with("{{") {
        url = format!("http://{url}");
    }
    let body = data.join("&");
    let method = match method {
        Some(m) => m,
        None if head => "HEAD".into(),
        None if !data.is_empty() && !get => "POST".into(),
        None => "GET".into(),
    };
    let body = if get && !body.is_empty() {
        if let Some(i) = url.find('#') {
            url.truncate(i);
        }
        url.push(if url.contains('?') { '&' } else { '?' });
        url.push_str(&body);
        String::new()
    } else {
        body
    };
    if let Some(u) = user {
        let b64 = base64::engine::general_purpose::STANDARD.encode(u.as_bytes());
        headers.push(("Authorization".into(), format!("Basic {b64}")));
    }
    Ok(import_parts(
        &method,
        &url,
        &headers,
        body.as_bytes(),
        false,
    ))
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// A `curl` command that sends exactly what was built.
pub fn to_curl(b: &Built) -> String {
    let mut parts = vec!["curl".to_string()];
    // curl sends GET, or POST once there is a body; -X HEAD would wait for a body.
    let implied = if b.body.is_empty() { "GET" } else { "POST" };
    if b.method == "HEAD" && b.body.is_empty() {
        parts.push("-I".into());
    } else if b.method != implied {
        parts.push(format!("-X {}", quote(&b.method)));
    }
    parts.push(quote(&b.url));
    for (k, v) in &b.headers {
        parts.push(format!("-H {}", quote(&format!("{k}: {v}"))));
    }
    if !b.body.is_empty() {
        parts.push(format!(
            "--data-raw {}",
            quote(&String::from_utf8_lossy(&b.body))
        ));
    }
    parts.join(" \\\n  ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_the_method_for_the_shell() {
        let method = "CUSTOM'$(touch nope)";
        let built = Built {
            method: method.into(),
            url: "https://x.dev/".into(),
            headers: Vec::new(),
            body: Vec::new(),
            ..Default::default()
        };
        let command = to_curl(&built);
        assert!(command.contains(&format!("-X {}", quote(method))));
        assert_eq!(words(&command).unwrap()[2], method);
    }

    #[test]
    fn rejects_multiple_urls() {
        for command in [
            "curl https://a https://b",
            "curl --url https://a https://b",
            "curl https://a --url https://b",
            "curl --unknown value https://b",
        ] {
            assert_eq!(
                parse_curl(command).unwrap_err(),
                "the command has more than one URL"
            );
        }
    }

    #[test]
    fn rejects_routing_options() {
        for flag in ROUTING_OPTIONS {
            for command in [
                format!("curl https://x.dev {flag} destination"),
                if flag.starts_with("--") {
                    format!("curl {flag}=destination https://x.dev")
                } else {
                    format!("curl {flag}destination https://x.dev")
                },
            ] {
                assert_eq!(
                    parse_curl(&command).unwrap_err(),
                    format!("{flag} changes where the request goes; remove it to import")
                );
            }
        }
    }

    #[test]
    fn get_data_is_added_after_removing_the_fragment() {
        for (url, expected) in [
            ("https://x.dev/#fragment?ignored", "https://x.dev/?b=2"),
            ("https://x.dev/?a=1#fragment", "https://x.dev/?a=1&b=2"),
        ] {
            let r = parse_curl(&format!("curl -G '{url}' -d b=2")).unwrap();
            let built = crate::build(&r, &[]).unwrap();
            assert_eq!(built.url, expected);
            assert!(built.body.is_empty());
        }
    }

    #[test]
    fn curl_keeps_content_encoding_and_body() {
        let r = parse_curl(r#"curl https://x.dev -H 'Content-Encoding: gzip' -H 'Content-Type: application/json' --data-raw '{ "a":1 }'"#).unwrap();
        let built = crate::build(&r, &[]).unwrap();
        assert!(built
            .headers
            .contains(&("Content-Encoding".into(), "gzip".into())));
        assert_eq!(built.body, br#"{ "a":1 }"#);
    }

    #[test]
    fn ansi_quotes_decode_utf8_bytes_together() {
        assert_eq!(
            words(r"curl $'\xC3\xA9' $'a\xFFé\u263A'").unwrap(),
            vec!["curl", "é", "a�é☺"]
        );
    }

    #[test]
    fn reads_a_browser_copy_as_curl() {
        let cmd = r#"curl 'https://api.example.com/v1/items?page=2&q=red+shoes' \
  -H 'accept: application/json' \
  -H 'authorization: Bearer abc.def' \
  -H 'content-type: application/json' \
  --data-raw $'{"name":"it\'s","n":1}' \
  --compressed"#;
        let r = parse_curl(cmd).unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "https://api.example.com/v1/items");
        assert_eq!(
            r.params,
            vec![KeyValue::new("page", "2"), KeyValue::new("q", "red shoes")]
        );
        assert_eq!(r.auth, AuthKind::Bearer);
        assert_eq!(r.auth_token, "abc.def");
        assert_eq!(r.body_kind, BodyKind::Json);
        assert!(r.body.contains("\"it's\""), "{}", r.body);
        assert!(r
            .headers
            .iter()
            .all(|h| !h.name.eq_ignore_ascii_case("authorization")));
    }

    #[test]
    fn short_flags_glued_values_and_get() {
        let r = parse_curl("curl -sSL -XPUT example.com/a -d a=1 -d b=2 -u ann:pw").unwrap();
        assert_eq!(r.method, "PUT");
        assert_eq!(r.url, "http://example.com/a");
        assert_eq!(r.body_kind, BodyKind::Form);
        assert_eq!(
            r.form,
            vec![KeyValue::new("a", "1"), KeyValue::new("b", "2")]
        );
        assert_eq!(
            (r.auth, r.auth_user.as_str(), r.auth_password.as_str()),
            (AuthKind::Basic, "ann", "pw")
        );

        let g =
            parse_curl("curl -G https://x.dev/s --data-urlencode 'q=a b' -H \"X-A: \\\"q\\\"\"")
                .unwrap();
        assert_eq!(g.method, "GET");
        assert_eq!(g.params, vec![KeyValue::new("q", "a b")]);
        assert_eq!(g.headers, vec![KeyValue::new("X-A", "\"q\"")]);
    }

    #[test]
    fn rejects_what_it_cannot_send() {
        assert!(parse_curl("curl -F a=@x https://x").is_err());
        assert!(parse_curl("curl -d @body.json https://x").is_err());
        assert!(parse_curl("curl -s").is_err());
        assert!(parse_curl("curl 'https://x").is_err());
    }

    #[test]
    fn from_parts_moves_basic_auth_and_drops_wire_headers() {
        let r = from_parts(
            "post",
            "https://x.dev/login",
            &[
                ("Host".into(), "x.dev".into()),
                ("Content-Length".into(), "7".into()),
                ("Content-Encoding".into(), "gzip".into()),
                ("Authorization".into(), "Basic dTpw".into()),
                ("Content-Type".into(), "text/plain".into()),
            ],
            b"hello",
        );
        assert_eq!(r.method, "POST");
        assert_eq!((r.auth, r.auth_user.as_str()), (AuthKind::Basic, "u"));
        assert_eq!(r.headers, vec![KeyValue::new("Content-Type", "text/plain")]);
        assert_eq!((r.body_kind, r.body.as_str()), (BodyKind::Text, "hello"));
    }

    #[test]
    fn values_that_start_with_a_dash_are_kept_whole() {
        let cmd = "curl https://x.dev/up -H 'content-type: multipart/form-data; boundary=----B' \
                   --data-raw $'------B\\r\\nContent-Disposition: form-data; name=\"a\"\\r\\n\\r\\n1\\r\\n------B--\\r\\n' \
                   -d -1 -H '-X: y'";
        let r = parse_curl(cmd).unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.body_kind, BodyKind::Text);
        assert!(
            r.body
                .starts_with("------B\r\nContent-Disposition: form-data; name=\"a\""),
            "{}",
            r.body
        );
        assert!(r.body.ends_with("------B--\r\n&-1"), "{}", r.body);
        assert!(r.headers.contains(&KeyValue::new("-X", "y")));
    }

    #[test]
    fn to_curl_keeps_the_method() {
        let b = |method: &str, body: &[u8]| Built {
            method: method.into(),
            url: "https://x.dev/".into(),
            headers: Vec::new(),
            body: body.to_vec(),
            ..Default::default()
        };
        for (method, body) in [
            ("GET", &b"q=1"[..]),
            ("HEAD", b""),
            ("POST", b"a"),
            ("POST", b""),
            ("GET", b""),
        ] {
            let back = parse_curl(&to_curl(&b(method, body))).unwrap();
            assert_eq!(back.method, method, "{body:?}");
        }
        assert!(to_curl(&b("HEAD", b"")).contains("-I"));
    }

    #[test]
    fn round_trips_through_build_and_curl() {
        let r = parse_curl("curl -X DELETE 'https://x.dev/a?id=1%262' -H 'X-K: v'").unwrap();
        let b = crate::build(&r, &[]).unwrap();
        assert_eq!(b.url, "https://x.dev/a?id=1%262");
        let back = parse_curl(&to_curl(&b)).unwrap();
        assert_eq!(back.method, "DELETE");
        assert_eq!(back.params, r.params);
    }
}
