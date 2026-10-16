//! The pages the proxy serves about itself: certificate downloads in every
//! format a device wants, a status endpoint and a TLS trust probe.
//!
//! Reached at `http://proxy.local` (or the listen address) through the proxy,
//! and at `https://proxy.local` inside a decrypted tunnel. The HTTPS side is
//! how the page learns whether the device already trusts the CA: the request
//! only succeeds when the handshake does.

use crate::handler::{full, text_response};
use crate::parse::Target;
use crate::ProxyBody;
use base64::Engine;
use http::header::CONTENT_TYPE;
use http::{Response, StatusCode};
use std::net::SocketAddr;
use tls::CaStore;

/// The made-up host that always means "the proxy itself".
pub(crate) const LOCAL_HOST: &str = "proxy.local";

const PAGE: &str = include_str!("pages/ca.html");

pub(crate) fn serve(ca: &CaStore, listen: SocketAddr, t: &Target) -> Response<ProxyBody> {
    match t.path() {
        "/" | "/index.html" => page(ca, listen),
        "/ca.pem" | "/ca.crt" => download(
            ca.cert_pem().as_bytes().to_vec(),
            "application/x-pem-file",
            "proxy-ca.pem",
        ),
        "/ca.cer" | "/ca.der" => download(
            ca.cert_der().as_ref().to_vec(),
            "application/x-x509-ca-cert",
            "proxy-ca.cer",
        ),
        "/ca.mobileconfig" => download(
            mobileconfig(ca).into_bytes(),
            "application/x-apple-aspen-config",
            "proxy-ca.mobileconfig",
        ),
        "/status.json" | "/trust-check" => probe(ca),
        _ => text_response(
            StatusCode::NOT_FOUND,
            "Not found",
            "This is the proxy itself.",
        ),
    }
}

fn download(body: Vec<u8>, mime: &str, filename: &str) -> Response<ProxyBody> {
    let mut res = Response::new(full(body));
    let h = res.headers_mut();
    h.insert(CONTENT_TYPE, mime.parse().unwrap());
    h.insert(
        "content-disposition",
        format!("attachment; filename=\"{filename}\"").parse().unwrap(),
    );
    h.insert("cache-control", "no-store".parse().unwrap());
    res
}

/// JSON the page polls. Open to any origin so a page loaded by IP address can
/// ask `proxy.local` (which only resolves when the device uses the proxy), and
/// so the HTTP page can read the HTTPS answer.
fn probe(ca: &CaStore) -> Response<ProxyBody> {
    let mut res = Response::new(full(format!(
        "{{\"ok\":true,\"sha256\":{}}}",
        json_str(ca.sha256())
    )));
    let h = res.headers_mut();
    h.insert(CONTENT_TYPE, "application/json".parse().unwrap());
    h.insert("access-control-allow-origin", "*".parse().unwrap());
    h.insert("cache-control", "no-store".parse().unwrap());
    res
}

fn page(ca: &CaStore, listen: SocketAddr) -> Response<ProxyBody> {
    let info = format!(
        "{{\"sha256\":{},\"subject\":{},\"notAfter\":{},\"listen\":{},\"listenAny\":{}}}",
        json_str(ca.sha256()),
        json_str(ca.subject()),
        json_str(ca.not_after()),
        json_str(&listen.to_string()),
        listen.ip().is_unspecified(),
    );
    let mut res = Response::new(full(PAGE.replace("/*INFO*/null", &info)));
    let h = res.headers_mut();
    h.insert(CONTENT_TYPE, "text/html; charset=utf-8".parse().unwrap());
    h.insert("cache-control", "no-store".parse().unwrap());
    res
}

/// A JSON string literal that is also safe inside a `<script>` element.
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// 8-4-4-4-12 identifier taken from 32 hex digits.
fn uuid_from(hex: &str) -> String {
    let h = hex.to_ascii_uppercase();
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// An iOS and macOS configuration profile that installs the CA as a root.
/// Identifiers derive from the certificate hash, so the same CA always gives
/// the same profile and a reinstall replaces it instead of stacking up.
fn mobileconfig(ca: &CaStore) -> String {
    let hash = ca.sha256();
    let cert = base64::engine::general_purpose::STANDARD.encode(ca.cert_der().as_ref());
    let subject = xml_escape(ca.subject());
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n<dict>\n\
\t<key>PayloadContent</key>\n\t<array>\n\t\t<dict>\n\
\t\t\t<key>PayloadCertificateFileName</key>\n\t\t\t<string>proxy-ca.cer</string>\n\
\t\t\t<key>PayloadContent</key>\n\t\t\t<data>{cert}</data>\n\
\t\t\t<key>PayloadDescription</key>\n\t\t\t<string>Lets the Proxy debugging tool read HTTPS traffic from this device.</string>\n\
\t\t\t<key>PayloadDisplayName</key>\n\t\t\t<string>{subject}</string>\n\
\t\t\t<key>PayloadIdentifier</key>\n\t\t\t<string>local.proxy.ca.cert.{id}</string>\n\
\t\t\t<key>PayloadType</key>\n\t\t\t<string>com.apple.security.root</string>\n\
\t\t\t<key>PayloadUUID</key>\n\t\t\t<string>{u1}</string>\n\
\t\t\t<key>PayloadVersion</key>\n\t\t\t<integer>1</integer>\n\
\t\t</dict>\n\t</array>\n\
\t<key>PayloadDescription</key>\n\t<string>Trusts the certificate authority of a Proxy debugging session. Remove it when you are done.</string>\n\
\t<key>PayloadDisplayName</key>\n\t<string>Proxy debugging CA</string>\n\
\t<key>PayloadIdentifier</key>\n\t<string>local.proxy.ca.{id}</string>\n\
\t<key>PayloadOrganization</key>\n\t<string>Proxy</string>\n\
\t<key>PayloadRemovalDisallowed</key>\n\t<false/>\n\
\t<key>PayloadType</key>\n\t<string>Configuration</string>\n\
\t<key>PayloadUUID</key>\n\t<string>{u2}</string>\n\
\t<key>PayloadVersion</key>\n\t<integer>1</integer>\n\
</dict>\n</plist>\n",
        id = &hash[..16],
        u1 = uuid_from(&hash[..32]),
        u2 = uuid_from(&hash[32..64]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_strings_cannot_close_a_script() {
        assert_eq!(json_str("a\"b</script>"), "\"a\\\"b\\u003c/script\\u003e\"");
    }

    #[test]
    fn uuid_has_the_standard_shape() {
        assert_eq!(
            uuid_from("0123456789abcdef0123456789abcdef"),
            "01234567-89AB-CDEF-0123-456789ABCDEF"
        );
    }
}
