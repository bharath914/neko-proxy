use bytes::Bytes;
use std::sync::atomic::{AtomicU64, Ordering};

pub type FlowId = u64;
pub type Header = (String, String);

/// A flow is "slow" at or above this total duration.
pub const SLOW_MS: f64 = 1000.0;

pub mod flags {
    /// The client rejected the minted certificate; the host is tunneled from now on.
    pub const PINNED: u32 = 1;
    /// CONNECT tunnel that is not decrypted.
    pub const TUNNEL: u32 = 2;
    /// Transport or upstream failure.
    pub const ERROR: u32 = 4;
    pub const REQ_TRUNCATED: u32 = 8;
    pub const RES_TRUNCATED: u32 = 16;
    pub const WEBSOCKET: u32 = 32;
    /// The response came from a Map Local rule, not from upstream.
    pub const MAPPED: u32 = 64;
    /// The host is outside the SSL proxying list, so it was tunneled
    /// untouched even though it would otherwise have been decrypted.
    pub const SSL_BYPASSED: u32 = 128;
    /// A rule or a breakpoint edit changed the request or the response.
    pub const EDITED: u32 = 256;
    /// The flow was held at a breakpoint.
    pub const BREAKPOINT: u32 = 512;
    /// The request was sent again by the proxy (replay).
    pub const REPLAYED: u32 = 1024;
    /// SDK flow whose session dropped before the call finished.
    pub const INCOMPLETE: u32 = 2048;
    /// The SDK replaced values in this flow before sending it.
    pub const REDACTED: u32 = 4096;
    /// Events for this SDK flow were dropped under load.
    pub const DROPPED: u32 = 8192;
}

/// Where a flow was observed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Source {
    /// Seen on the wire by the proxy.
    #[default]
    Proxy,
    /// Reported by the in-app SDK.
    Sdk,
    /// An SDK record and a proxy record for the same call. The SDK record is
    /// the primary row; the proxy record stays stored as the wire view.
    Merged,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Proxy => "proxy",
            Source::Sdk => "sdk",
            Source::Merged => "merged",
        }
    }

    pub fn parse(s: &str) -> Source {
        match s {
            "sdk" => Source::Sdk,
            "merged" => Source::Merged,
            _ => Source::Proxy,
        }
    }
}

/// What the SDK did with a body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum BodyState {
    /// Sent with the flow, or there is none.
    #[default]
    Present = 0,
    /// Kept on the device; the core can ask for it.
    Lazy = 1,
    /// Not captured by policy (media type, strict redaction).
    Skipped = 2,
    /// Was on the device but is gone (evicted, app restarted).
    Gone = 3,
}

impl BodyState {
    pub fn from_u8(v: u8) -> BodyState {
        match v {
            1 => BodyState::Lazy,
            2 => BodyState::Skipped,
            3 => BodyState::Gone,
            _ => BodyState::Present,
        }
    }
}

/// Timings the client stack measured, in milliseconds, -1 when unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SdkTimings {
    pub dns: i32,
    pub connect: i32,
    pub tls: i32,
    pub request_sent: i32,
    pub ttfb: i32,
    pub download: i32,
}

impl Default for SdkTimings {
    fn default() -> Self {
        SdkTimings {
            dns: -1,
            connect: -1,
            tls: -1,
            request_sent: -1,
            ttfb: -1,
            download: -1,
        }
    }
}

/// Context only the SDK knows. `device_id`, `app_id` and the device and app
/// facts come from the authenticated session, never from call payloads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SdkInfo {
    pub device_id: String,
    pub app_id: String,
    pub call_id: String,
    pub name: String,
    pub tag: String,
    pub owner: String,
    pub screen: String,
    pub route_template: String,
    pub client_stack: String,
    pub device_name: String,
    pub device_model: String,
    pub os_version: String,
    pub app_name: String,
    pub app_version: String,
    pub build: String,
    pub sdk_name: String,
    pub sdk_version: String,
    pub connection: String,
    pub from_cache: bool,
    pub req_redacted: bool,
    pub res_redacted: bool,
    pub req_body: BodyState,
    pub res_body: BodyState,
    pub timings: Option<SdkTimings>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum FlowState {
    #[default]
    Pending = 0,
    Receiving = 1,
    Complete = 2,
    Failed = 3,
}

impl FlowState {
    pub fn from_u8(v: u8) -> FlowState {
        match v {
            1 => FlowState::Receiving,
            2 => FlowState::Complete,
            3 => FlowState::Failed,
            _ => FlowState::Pending,
        }
    }
}

/// All durations in microseconds. Zero means "not applicable", for example no
/// DNS or TLS cost on a reused connection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Timings {
    pub dns_us: u64,
    pub connect_us: u64,
    pub tls_us: u64,
    pub send_us: u64,
    pub wait_us: u64,
    pub receive_us: u64,
    pub total_us: u64,
    pub reused: bool,
}

/// What the table needs. Cheap to clone and to ship in batches.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlowSummary {
    pub id: FlowId,
    pub started_at_ms: i64,
    pub duration_ms: f64,
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub method: String,
    /// Path and query.
    pub path: String,
    pub status: u16,
    pub protocol: String,
    pub app: String,
    pub req_size: u64,
    pub res_size: u64,
    pub state: FlowState,
    pub flags: u32,
    pub content_type: String,
    pub error: String,
    pub source: Source,
    /// The other half of a merged pair (the proxy record for an SDK row, the
    /// SDK record for a hidden proxy row). Zero when not merged.
    pub merged_with: FlowId,
    pub device_id: String,
    pub app_id: String,
    pub name: String,
    pub tag: String,
    pub owner: String,
    pub screen: String,
    pub route_template: String,
    pub client_stack: String,
}

/// A flow with everything captured. Bodies are the bytes on the wire, still
/// content-encoded; `decode` turns them into something readable.
#[derive(Clone, Debug, Default)]
pub struct FlowRecord {
    pub id: FlowId,
    pub started_at_ms: i64,
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub method: String,
    pub path: String,
    pub query: String,
    pub status: u16,
    pub protocol: String,
    pub app: String,
    pub state: FlowState,
    pub flags: u32,
    pub error: String,
    pub req_headers: Vec<Header>,
    pub res_headers: Vec<Header>,
    pub req_size: u64,
    pub res_size: u64,
    pub req_body: Bytes,
    pub res_body: Bytes,
    pub timings: Timings,
    pub source: Source,
    /// Remote address of the client for proxied flows; empty if unknown.
    pub client_ip: String,
    pub merged_with: FlowId,
    pub sdk: Option<std::sync::Arc<SdkInfo>>,
}

impl FlowRecord {
    pub fn summary(&self) -> FlowSummary {
        let mut path = self.path.clone();
        if !self.query.is_empty() {
            path.push('?');
            path.push_str(&self.query);
        }
        FlowSummary {
            id: self.id,
            started_at_ms: self.started_at_ms,
            duration_ms: self.timings.total_us as f64 / 1000.0,
            scheme: self.scheme.clone(),
            host: self.host.clone(),
            port: self.port,
            method: self.method.clone(),
            path,
            status: self.status,
            protocol: self.protocol.clone(),
            app: self.app.clone(),
            req_size: self.req_size,
            res_size: self.res_size,
            state: self.state,
            flags: self.flags,
            content_type: short_content_type(self.header(&self.res_headers, "content-type")),
            error: self.error.clone(),
            source: self.source,
            merged_with: self.merged_with,
            device_id: self.sdk.as_ref().map(|i| i.device_id.clone()).unwrap_or_default(),
            app_id: self.sdk.as_ref().map(|i| i.app_id.clone()).unwrap_or_default(),
            name: self.sdk.as_ref().map(|i| i.name.clone()).unwrap_or_default(),
            tag: self.sdk.as_ref().map(|i| i.tag.clone()).unwrap_or_default(),
            owner: self.sdk.as_ref().map(|i| i.owner.clone()).unwrap_or_default(),
            screen: self.sdk.as_ref().map(|i| i.screen.clone()).unwrap_or_default(),
            route_template: self
                .sdk
                .as_ref()
                .map(|i| i.route_template.clone())
                .unwrap_or_default(),
            client_stack: self
                .sdk
                .as_ref()
                .map(|i| i.client_stack.clone())
                .unwrap_or_default(),
        }
    }

    fn header<'a>(&self, headers: &'a [Header], name: &str) -> &'a str {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }

    pub fn request_header(&self, name: &str) -> &str {
        self.header(&self.req_headers, name)
    }

    pub fn response_header(&self, name: &str) -> &str {
        self.header(&self.res_headers, name)
    }

    pub fn url(&self) -> String {
        let default_port = match self.scheme.as_str() {
            "https" => 443,
            "http" => 80,
            _ => 0,
        };
        let mut s = format!("{}://{}", self.scheme, host_for_url(&self.host));
        if self.port != 0 && self.port != default_port {
            s.push(':');
            s.push_str(&self.port.to_string());
        }
        s.push_str(&self.path);
        if !self.query.is_empty() {
            s.push('?');
            s.push_str(&self.query);
        }
        s
    }
}

fn host_for_url(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

/// Short label for the Type column, from a Content-Type value.
pub fn short_content_type(ct: &str) -> String {
    let essence = ct
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if essence.is_empty() {
        return String::new();
    }
    let (kind, sub) = essence.split_once('/').unwrap_or((essence.as_str(), ""));
    let short = match (kind, sub) {
        ("image", "svg+xml") => "svg",
        (_, "json") => "json",
        (_, s) if s.ends_with("+json") => "json",
        (_, "html") | (_, "xhtml+xml") => "html",
        (_, "xml") | (_, "atom+xml") | (_, "rss+xml") => "xml",
        (_, s) if s.ends_with("+xml") => "xml",
        (_, "javascript") | (_, "x-javascript") | (_, "ecmascript") => "js",
        (_, "css") => "css",
        (_, "x-www-form-urlencoded") => "form",
        (_, "x-protobuf") | (_, "protobuf") | (_, "grpc") | (_, "x-protobuffer") => "proto",
        (_, "event-stream") => "sse",
        (_, "octet-stream") => "binary",
        (_, "wasm") => "wasm",
        (_, "pdf") => "pdf",
        (_, "zip") | (_, "gzip") | (_, "x-tar") => "archive",
        ("image", _) => "image",
        ("font", _) => "font",
        ("video", _) => "video",
        ("audio", _) => "audio",
        ("text", "plain") => "text",
        ("text", _) => "text",
        _ => "",
    };
    if short.is_empty() {
        sub.trim_start_matches("x-").to_string()
    } else {
        short.to_string()
    }
}

/// Monotonic flow ids. Seeded from the store so ids survive restarts.
#[derive(Debug)]
pub struct IdGen(AtomicU64);

impl IdGen {
    pub fn starting_after(last: FlowId) -> IdGen {
        IdGen(AtomicU64::new(last))
    }

    pub fn next(&self) -> FlowId {
        self.0.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn last(&self) -> FlowId {
        self.0.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_types() {
        assert_eq!(
            short_content_type("application/json; charset=utf-8"),
            "json"
        );
        assert_eq!(short_content_type("application/vnd.api+json"), "json");
        assert_eq!(short_content_type("image/webp"), "image");
        assert_eq!(short_content_type("image/svg+xml"), "svg");
        assert_eq!(short_content_type("text/html"), "html");
        assert_eq!(short_content_type(""), "");
    }

    #[test]
    fn url_hides_default_port() {
        let r = FlowRecord {
            scheme: "https".into(),
            host: "example.com".into(),
            port: 443,
            path: "/a".into(),
            query: "b=1".into(),
            ..Default::default()
        };
        assert_eq!(r.url(), "https://example.com/a?b=1");
        let r = FlowRecord { port: 8443, ..r };
        assert_eq!(r.url(), "https://example.com:8443/a?b=1");
    }

    #[test]
    fn ids_are_monotonic() {
        let g = IdGen::starting_after(41);
        assert_eq!(g.next(), 42);
        assert_eq!(g.next(), 43);
    }
}
