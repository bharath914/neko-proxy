//! Per-request path: resolve the target, record the flow, forward upstream,
//! tee the bodies, hand the response back.

use crate::body::{BodyEnd, BoxError, Tee};
use crate::connector::{self, Alpn, ConnInfo};
use crate::flowctx::{AppSlot, FlowCtx, NewFlow};
use crate::parse::{resolve_plain, resolve_tunneled, Target};
use crate::{Inner, ProxyBody};
use bytes::Bytes;
use flow::{flags, Delay, Edited, Edits, FlowSummary, Held, Intercept, Phase, Retarget, Verdict};
use http::header::{CONNECTION, CONTENT_TYPE, HOST, UPGRADE};
use http::{HeaderMap, HeaderName, Method, Request, Response, StatusCode, Uri, Version};
use http_body_util::{BodyExt, Empty, Full, Limited};
use hyper::body::Incoming;
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) enum ConnKind {
    /// A client talking to the proxy port directly.
    Plain,
    /// Decrypted traffic inside a CONNECT tunnel to this authority.
    Tunnel { host: String, port: u16 },
}

pub(crate) struct ConnCtx {
    pub kind: ConnKind,
    pub peer: SocketAddr,
    pub app: Arc<AppSlot>,
    /// Requests seen on this connection. Zero at the end of a freshly
    /// decrypted tunnel means the client walked away after the handshake.
    pub requests: std::sync::atomic::AtomicU32,
}

pub(crate) fn unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub(crate) fn empty() -> ProxyBody {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed_unsync()
}

pub(crate) fn full(b: impl Into<Bytes>) -> ProxyBody {
    Full::new(b.into())
        .map_err(|never| match never {})
        .boxed_unsync()
}

fn proto_label(v: Version) -> &'static str {
    match v {
        Version::HTTP_2 => "h2",
        Version::HTTP_3 => "h3",
        Version::HTTP_10 => "HTTP/1.0",
        Version::HTTP_09 => "HTTP/0.9",
        _ => "HTTP/1.1",
    }
}

fn headers_vec(h: &HeaderMap) -> Vec<(String, String)> {
    h.iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect()
}

const HOP_BY_HOP: [&str; 8] = [
    "connection",
    "proxy-connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
];

fn strip_hop_by_hop(h: &mut HeaderMap, keep_upgrade: bool) {
    let named: Vec<HeaderName> = h
        .get_all(CONNECTION)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(','))
        .filter_map(|t| HeaderName::from_bytes(t.trim().as_bytes()).ok())
        .collect();
    for n in named {
        if keep_upgrade && n == UPGRADE {
            continue;
        }
        h.remove(n);
    }
    for n in HOP_BY_HOP {
        // gRPC needs `te: trailers` to survive.
        if n == "te" && h.get("te").map(|v| v.as_bytes()) == Some(b"trailers") {
            continue;
        }
        h.remove(n);
    }
    if !keep_upgrade {
        h.remove(UPGRADE);
    }
}

fn is_upgrade(req: &Request<Incoming>) -> bool {
    req.headers().contains_key(UPGRADE)
        && req
            .headers()
            .get_all(CONNECTION)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .any(|v| {
                v.split(',')
                    .any(|t| t.trim().eq_ignore_ascii_case("upgrade"))
            })
}

fn error_chain(e: &dyn std::error::Error) -> String {
    let mut parts = vec![e.to_string()];
    let mut cur = e.source();
    while let Some(s) = cur {
        let m = s.to_string();
        if parts.last() != Some(&m) {
            parts.push(m);
        }
        cur = s.source();
    }
    if parts.len() > 1 && parts[0].starts_with("client error") {
        parts.remove(0);
    }
    parts.join(": ")
}

pub(crate) fn text_response(status: StatusCode, title: &str, detail: &str) -> Response<ProxyBody> {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>{title}</title>\
         <body style=\"font:15px system-ui;max-width:42rem;margin:4rem auto;padding:0 1rem\">\
         <h1 style=\"font-size:1.4rem\">{title}</h1><p>{}</p>\
         <p style=\"color:#666\">Reported by the local debugging proxy.</p>",
        html_escape(detail)
    );
    let mut res = Response::new(full(body));
    *res.status_mut() = status;
    res.headers_mut()
        .insert(CONTENT_TYPE, "text/html; charset=utf-8".parse().unwrap());
    res.headers_mut()
        .insert("x-neko-error", "1".parse().unwrap());
    res
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// What rules and breakpoints see when the flow is not being recorded.
fn summary_for(target: &Target, method: &Method) -> FlowSummary {
    FlowSummary {
        scheme: target.scheme.to_string(),
        host: target.host.clone(),
        port: target.port,
        method: method.to_string(),
        path: target.path_and_query.clone(),
        ..Default::default()
    }
}

/// A message collected so it can be shown and edited at a breakpoint.
struct Collected {
    bytes: Bytes,
    /// The body was decompressed for editing, so `content-encoding` is gone.
    decoded: bool,
}

/// Read a whole body, up to `cap`, and undo content encoding when possible.
async fn collect_for_hold<B>(
    body: B,
    headers: &HeaderMap,
    cap: usize,
) -> Result<Collected, String>
where
    B: hyper::body::Body<Data = Bytes> + Unpin,
    B::Error: Into<BoxError>,
{
    let wire = Limited::new(body, cap)
        .collect()
        .await
        .map_err(|e| {
            let e: BoxError = e;
            format!(
                "The body is larger than the {} MiB a breakpoint can hold, or its stream failed: {e}",
                cap / (1024 * 1024)
            )
        })?
        .to_bytes();
    let enc = headers
        .get(http::header::CONTENT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if enc.is_empty() || enc.eq_ignore_ascii_case("identity") || wire.is_empty() {
        return Ok(Collected {
            bytes: wire,
            decoded: false,
        });
    }
    let d = decode::decode_content(enc, &wire, cap);
    if d.problem.is_none() {
        Ok(Collected {
            bytes: Bytes::from(d.bytes),
            decoded: true,
        })
    } else {
        Ok(Collected {
            bytes: wire,
            decoded: false,
        })
    }
}

/// Header map to the list a breakpoint editor shows. Drops what no longer
/// describes the body once it has been decoded.
fn held_headers(h: &HeaderMap, decoded: bool) -> Vec<(String, String)> {
    h.iter()
        .filter(|(k, _)| {
            !(decoded
                && (*k == http::header::CONTENT_ENCODING || *k == http::header::CONTENT_LENGTH))
        })
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect()
}

fn header_map(list: &[(String, String)]) -> HeaderMap {
    let mut m = HeaderMap::new();
    for (n, v) in list {
        if let (Ok(n), Ok(v)) = (
            HeaderName::from_bytes(n.as_bytes()),
            http::HeaderValue::from_str(v),
        ) {
            m.append(n, v);
        }
    }
    m
}

/// The body is now exactly these bytes: let the framing follow.
fn fix_framing(h: &mut HeaderMap, len: usize) {
    h.remove(http::header::TRANSFER_ENCODING);
    h.insert(http::header::CONTENT_LENGTH, http::HeaderValue::from(len));
}

impl Inner {
    pub(crate) async fn handle(
        self: Arc<Self>,
        req: Request<Incoming>,
        ctx: Arc<ConnCtx>,
    ) -> Result<Response<ProxyBody>, Infallible> {
        ctx.requests.fetch_add(1, Ordering::Relaxed);
        if req.method() == Method::CONNECT {
            return Ok(self.handle_connect(req, &ctx));
        }
        let host_header = req
            .headers()
            .get(HOST)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let target = match &ctx.kind {
            ConnKind::Plain => match resolve_plain(req.method(), req.uri(), host_header.as_deref())
            {
                Ok(t) => t,
                Err(e) => {
                    return Ok(text_response(
                        StatusCode::BAD_REQUEST,
                        "Bad proxy request",
                        &format!("Could not work out where this request is going ({e:?})."),
                    ))
                }
            },
            ConnKind::Tunnel { host, port } => {
                resolve_tunneled(req.method(), req.uri(), host, *port)
            }
        };
        if self.is_self(&target, &ctx.kind) {
            return Ok(crate::ca_page::serve(&self.ca, self.cfg.listen, &target));
        }
        if is_upgrade(&req) {
            return Ok(self.proxy_upgrade(req, target, &ctx).await);
        }
        Ok(self.proxy_http(req, target, &ctx).await)
    }

    /// Send a request again as a new, recorded flow.
    pub(crate) async fn replay(
        self: &Arc<Self>,
        r: crate::ReplayRequest,
    ) -> Result<crate::ReplayOutcome, String> {
        if self.cfg.offline {
            return Err("upstream connections are disabled".into());
        }
        let method = Method::from_bytes(r.method.trim().to_ascii_uppercase().as_bytes())
            .map_err(|_| format!("\"{}\" is not an HTTP method", r.method))?;
        let uri: Uri = r.url.parse().map_err(|_| format!("\"{}\" is not a URL", r.url))?;
        if uri.scheme().is_none() {
            return Err("the URL must start with http:// or https://".into());
        }
        let target = resolve_plain(&method, &uri, None)
            .map_err(|e| format!("could not work out where to send this ({e:?})"))?;

        let mut headers = header_map(&r.headers);
        strip_hop_by_hop(&mut headers, false);
        headers.remove(HOST);
        headers.remove(http::header::CONTENT_LENGTH);

        let flow = FlowCtx::start(
            self.sink.clone(),
            NewFlow {
                id: self.ids.next(),
                started_at_ms: unix_ms(),
                scheme: target.scheme,
                host: target.host.clone(),
                port: target.port,
                method: method.to_string(),
                path: target.path().to_string(),
                query: target.query().to_string(),
                req_headers: headers_vec(&headers),
                flags: flags::REPLAYED,
                protocol: "HTTP/1.1",
            },
            AppSlot::named("neko"),
            self.cfg.body_cap,
        );
        flow.req_cap.push(&r.body);

        let upstream = Uri::builder()
            .scheme(target.scheme)
            .authority(target.authority())
            .path_and_query(target.path_and_query.as_str())
            .build()
            .map_err(|e| {
                let m = format!("Invalid request target: {e}");
                flow.fail(m.clone());
                m
            })?;
        let mut builder = Request::builder().method(method).uri(upstream);
        if let Some(h) = builder.headers_mut() {
            *h = headers;
        }
        let req = builder.body(full(r.body)).map_err(|e| {
            let m = e.to_string();
            flow.fail(m.clone());
            m
        })?;
        flow.dispatched();
        let sent = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            self.client.request(req),
        )
        .await;
        let res = match sent {
            Ok(Ok(res)) => res,
            Ok(Err(e)) => {
                let m = error_chain(&e);
                flow.fail(m.clone());
                return Err(m);
            }
            Err(_) => {
                let m = "the server did not answer within 60 seconds".to_string();
                flow.fail(m.clone());
                return Err(m);
            }
        };
        let status = res.status().as_u16();
        let summary = flow.summary();
        let flow_id = summary.id;
        let forwarded = self.forward_response(Some(flow.clone()), summary, res).await;
        // Drain the body so the flow finishes with everything captured.
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            forwarded.into_body().collect(),
        )
        .await;
        Ok(crate::ReplayOutcome {
            flow_id,
            status,
        })
    }

    fn is_self(&self, t: &Target, kind: &ConnKind) -> bool {
        if t.host == crate::ca_page::LOCAL_HOST {
            return true;
        }
        if !matches!(kind, ConnKind::Plain) || t.scheme != "http" {
            return false;
        }
        let listen = self.cfg.listen;
        t.port == listen.port()
            && (t.host == "localhost"
                || t.host
                    .parse::<IpAddr>()
                    .map(|ip| ip.is_loopback() || ip == listen.ip())
                    .unwrap_or(false))
    }

    fn begin_flow(
        &self,
        target: &Target,
        parts: &http::request::Parts,
        app: &Arc<AppSlot>,
    ) -> Option<Arc<FlowCtx>> {
        if !self.recording.load(Ordering::Relaxed) {
            return None;
        }
        Some(FlowCtx::start(
            self.sink.clone(),
            NewFlow {
                id: self.ids.next(),
                started_at_ms: unix_ms(),
                scheme: target.scheme,
                host: target.host.clone(),
                port: target.port,
                method: parts.method.to_string(),
                path: target.path().to_string(),
                query: target.query().to_string(),
                req_headers: headers_vec(&parts.headers),
                flags: 0,
                protocol: proto_label(parts.version),
            },
            app.clone(),
            self.cfg.body_cap,
        ))
    }

    async fn proxy_http(
        self: &Arc<Self>,
        req: Request<Incoming>,
        mut target: Target,
        ctx: &ConnCtx,
    ) -> Response<ProxyBody> {
        let (mut parts, body) = req.into_parts();
        let flow = self.begin_flow(&target, &parts, &ctx.app);
        let gate = self.gate.lock().unwrap().clone();
        let current = |target: &Target, parts: &http::request::Parts| match &flow {
            Some(f) => f.summary(),
            None => summary_for(target, &parts.method),
        };

        // Breakpoint on the request: hold it, let the person look and edit.
        let mut held_body: Option<Bytes> = None;
        let mut body = Some(body);
        if let Some(g) = &gate {
            let summary = current(&target, &parts);
            if let Some(ticket) = g.request_ticket(&summary, &parts) {
                let collected = collect_for_hold(
                    body.take().expect("body is taken once"),
                    &parts.headers,
                    self.cfg.body_cap,
                )
                .await;
                let collected = match collected {
                    Ok(c) => c,
                    Err(msg) => {
                        if let Some(f) = &flow {
                            f.fail(msg.clone());
                        }
                        return text_response(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            "Cannot hold this request at a breakpoint",
                            &msg,
                        );
                    }
                };
                let held = Held {
                    method: parts.method.to_string(),
                    path_and_query: target.path_and_query.clone(),
                    status: 0,
                    headers: held_headers(&parts.headers, collected.decoded),
                    body: collected.bytes.clone(),
                };
                if let Some(f) = &flow {
                    f.add_flags(flags::BREAKPOINT);
                }
                match g.hold(&summary, Phase::Request, ticket, held).await {
                    Verdict::Abort(why) => {
                        let msg = format!("Aborted at a breakpoint: {why}");
                        if let Some(f) = &flow {
                            f.fail(msg.clone());
                        }
                        return text_response(
                            StatusCode::BAD_GATEWAY,
                            "The request was aborted at a breakpoint",
                            &why,
                        );
                    }
                    Verdict::Continue(edits) => {
                        let changed = !edits.is_empty() || collected.decoded;
                        let Edits {
                            method,
                            path_and_query,
                            headers,
                            body: new_body,
                            ..
                        } = edits;
                        if let Some(m) = method.and_then(|m| Method::from_bytes(m.as_bytes()).ok())
                        {
                            parts.method = m;
                        }
                        if let Some(pq) = path_and_query {
                            if pq.starts_with('/') && pq.parse::<http::uri::PathAndQuery>().is_ok()
                            {
                                target.path_and_query = pq;
                            }
                        }
                        match headers {
                            Some(list) => parts.headers = header_map(&list),
                            None if collected.decoded => {
                                parts.headers.remove(http::header::CONTENT_ENCODING);
                            }
                            None => {}
                        }
                        let final_body = new_body.unwrap_or(collected.bytes);
                        fix_framing(&mut parts.headers, final_body.len());
                        if let Some(f) = &flow {
                            if changed {
                                f.request_edited(
                                    parts.method.to_string(),
                                    target.path().to_string(),
                                    target.query().to_string(),
                                    headers_vec(&parts.headers),
                                );
                            }
                            f.req_cap.push(&final_body);
                        }
                        held_body = Some(final_body);
                    }
                }
            }
        }

        // Rules: rewrites, remaps, delays and mocks.
        let mut respond = None;
        if !self.chain.is_empty() {
            let summary = current(&target, &parts);
            if let Intercept::Respond(r) = self.chain.run_request(&summary, &mut parts) {
                respond = Some(r);
            }
        }
        let delay = parts.extensions.remove::<Delay>();
        let retarget = parts.extensions.remove::<Retarget>();
        if parts.extensions.remove::<Edited>().is_some() {
            if let Some(f) = &flow {
                f.add_flags(flags::EDITED);
            }
        }
        if let Some(Delay(d)) = delay {
            tokio::time::sleep(d).await;
        }
        if let Some(r) = respond {
            let (rp, rb) = r.into_parts();
            // What was served is what gets recorded, so the body shows.
            let rb_copy = rb.clone();
            let res = Response::from_parts(rp, full(rb));
            if let Some(f) = &flow {
                f.response_headers(
                    res.status().as_u16(),
                    proto_label(res.version()),
                    headers_vec(res.headers()),
                    None,
                    flags::MAPPED,
                );
                f.res_cap.push(&rb_copy);
                f.finish_body(BodyEnd::Complete);
            }
            return res;
        }
        if let Some(rt) = retarget {
            let scheme_changed = rt.scheme.map(|s| s != target.scheme).unwrap_or(false);
            if let Some(s) = rt.scheme {
                target.scheme = s;
            }
            if let Some(h) = rt.host {
                target.host = h;
            }
            match rt.port {
                Some(p) => target.port = p,
                None if scheme_changed => {
                    target.port = if target.scheme == "https" { 443 } else { 80 }
                }
                None => {}
            }
        }

        let uri = match Uri::builder()
            .scheme(target.scheme)
            .authority(target.authority())
            .path_and_query(target.path_and_query.as_str())
            .build()
        {
            Ok(u) => u,
            Err(e) => {
                let msg = format!("Invalid request target: {e}");
                if let Some(f) = &flow {
                    f.fail(msg.clone());
                }
                return text_response(StatusCode::BAD_REQUEST, "Bad proxy request", &msg);
            }
        };
        parts.uri = uri;
        parts.version = Version::HTTP_11;
        strip_hop_by_hop(&mut parts.headers, false);
        parts.headers.remove(HOST);

        let req_body: ProxyBody = match (held_body, body, &flow) {
            (Some(b), _, _) => full(b),
            (None, Some(body), Some(f)) => {
                Tee::new(body, f.req_cap.clone(), |_| {}).boxed_unsync()
            }
            (None, Some(body), None) => body.map_err(|e| Box::new(e) as BoxError).boxed_unsync(),
            (None, None, _) => empty(),
        };
        if let Some(f) = &flow {
            f.dispatched();
        }
        let summary = current(&target, &parts);
        match self
            .client
            .request(Request::from_parts(parts, req_body))
            .await
        {
            Ok(res) => self.forward_response(flow, summary, res).await,
            Err(e) => {
                let msg = error_chain(&e);
                if let Some(f) = &flow {
                    f.fail(msg.clone());
                }
                text_response(
                    StatusCode::BAD_GATEWAY,
                    "The proxy could not reach the server",
                    &msg,
                )
            }
        }
    }

    async fn forward_response(
        self: &Arc<Self>,
        flow: Option<Arc<FlowCtx>>,
        fallback: FlowSummary,
        res: Response<Incoming>,
    ) -> Response<ProxyBody> {
        let (mut parts, body) = res.into_parts();
        let summary = flow.as_ref().map(|f| f.summary()).unwrap_or(fallback);
        let mut extra = 0u32;

        if !self.chain.is_empty() {
            let before = parts.headers.clone();
            self.chain.run_response(&summary, &mut parts);
            if before != parts.headers {
                extra |= flags::EDITED;
            }
        }

        // Breakpoint on the response.
        let mut held_body: Option<Bytes> = None;
        let mut body = Some(body);
        let gate = self.gate.lock().unwrap().clone();
        if let (Some(g), true) = (&gate, parts.status != StatusCode::SWITCHING_PROTOCOLS) {
            if let Some(ticket) = g.response_ticket(&summary, &parts) {
                let collected = match collect_for_hold(
                    body.take().expect("body is taken once"),
                    &parts.headers,
                    self.cfg.body_cap,
                )
                .await
                {
                    Ok(c) => c,
                    Err(msg) => {
                        if let Some(f) = &flow {
                            f.fail(msg.clone());
                        }
                        return text_response(
                            StatusCode::BAD_GATEWAY,
                            "Cannot hold this response at a breakpoint",
                            &msg,
                        );
                    }
                };
                let held = Held {
                    method: summary.method.clone(),
                    path_and_query: String::new(),
                    status: parts.status.as_u16(),
                    headers: held_headers(&parts.headers, collected.decoded),
                    body: collected.bytes.clone(),
                };
                if let Some(f) = &flow {
                    f.add_flags(flags::BREAKPOINT);
                }
                match g.hold(&summary, Phase::Response, ticket, held).await {
                    Verdict::Abort(why) => {
                        let msg = format!("Aborted at a breakpoint: {why}");
                        if let Some(f) = &flow {
                            f.fail(msg);
                        }
                        return text_response(
                            StatusCode::BAD_GATEWAY,
                            "The response was aborted at a breakpoint",
                            &why,
                        );
                    }
                    Verdict::Continue(edits) => {
                        if !edits.is_empty() || collected.decoded {
                            extra |= flags::EDITED;
                        }
                        if let Some(st) = edits.status.and_then(|s| StatusCode::from_u16(s).ok()) {
                            parts.status = st;
                        }
                        match edits.headers {
                            Some(list) => parts.headers = header_map(&list),
                            None if collected.decoded => {
                                parts.headers.remove(http::header::CONTENT_ENCODING);
                            }
                            None => {}
                        }
                        let final_body = edits.body.unwrap_or(collected.bytes);
                        fix_framing(&mut parts.headers, final_body.len());
                        held_body = Some(final_body);
                    }
                }
            }
        }

        if let Some(f) = &flow {
            let info = parts.extensions.get::<Arc<ConnInfo>>().cloned();
            let claimed = info.as_ref().filter(|i| i.claim());
            let ws = if parts.status == StatusCode::SWITCHING_PROTOCOLS {
                flags::WEBSOCKET
            } else {
                0
            };
            f.response_headers(
                parts.status.as_u16(),
                proto_label(parts.version),
                headers_vec(&parts.headers),
                claimed.map(|i| &**i),
                ws | extra,
            );
        }
        strip_hop_by_hop(&mut parts.headers, false);
        let body: ProxyBody = match (held_body, body, flow) {
            (Some(b), _, flow) => {
                if let Some(f) = &flow {
                    f.res_cap.push(&b);
                    f.finish_body(BodyEnd::Complete);
                }
                full(b)
            }
            (None, Some(body), Some(f)) => {
                let f2 = f.clone();
                Tee::new(body, f.res_cap.clone(), move |end| f2.finish_body(end)).boxed_unsync()
            }
            (None, Some(body), None) => body.map_err(|e| Box::new(e) as BoxError).boxed_unsync(),
            (None, None, _) => empty(),
        };
        Response::from_parts(parts, body)
    }

    /// WebSocket and other HTTP/1.1 upgrades: forward the handshake on a
    /// dedicated connection, then splice both sides. Frames are not captured
    /// in Phase 1.
    async fn proxy_upgrade(
        self: &Arc<Self>,
        mut req: Request<Incoming>,
        target: Target,
        ctx: &ConnCtx,
    ) -> Response<ProxyBody> {
        let on_client = hyper::upgrade::on(&mut req);
        let (mut parts, _body) = req.into_parts();
        let flow = self.begin_flow(&target, &parts, &ctx.app);
        let fail = |flow: &Option<Arc<FlowCtx>>, msg: String| {
            if let Some(f) = flow {
                f.fail(msg.clone());
            }
            text_response(
                StatusCode::BAD_GATEWAY,
                "The proxy could not reach the server",
                &msg,
            )
        };

        let tls = if target.scheme == "https" {
            Some(self.upstream_tls.config(Alpn::Http1Only).await)
        } else {
            None
        };
        if self.cfg.offline {
            return fail(&flow, "upstream connections are disabled".into());
        }
        let (stream, info) = match connector::connect(&target.host, target.port, tls).await {
            Ok(v) => v,
            Err(e) => return fail(&flow, error_chain(&*e)),
        };
        let io = TokioIo::new(stream);
        let (mut sender, conn) = match hyper::client::conn::http1::Builder::new()
            .preserve_header_case(true)
            .handshake::<_, Empty<Bytes>>(io)
            .await
        {
            Ok(v) => v,
            Err(e) => return fail(&flow, error_chain(&e)),
        };
        tokio::spawn(async move {
            let _ = conn.with_upgrades().await;
        });

        parts.uri = target
            .path_and_query
            .parse()
            .unwrap_or_else(|_| Uri::from_static("/"));
        parts.version = Version::HTTP_11;
        strip_hop_by_hop(&mut parts.headers, true);
        parts.headers.remove(HOST);
        if let Ok(v) = target.authority().parse() {
            parts.headers.insert(HOST, v);
        }
        parts.headers.insert(CONNECTION, "upgrade".parse().unwrap());
        if let Some(f) = &flow {
            f.dispatched();
        }
        let up_req = Request::from_parts(parts, Empty::<Bytes>::new());
        let mut res = match sender.send_request(up_req).await {
            Ok(r) => r,
            Err(e) => return fail(&flow, error_chain(&e)),
        };

        if res.status() != StatusCode::SWITCHING_PROTOCOLS {
            // The server declined the upgrade; relay its answer as a normal response.
            let (mut rparts, body) = res.into_parts();
            rparts.extensions.insert(info);
            let summary = summary_for(&target, &Method::GET);
            return self.forward_response(flow, summary, Response::from_parts(rparts, body)).await;
        }

        let on_upstream = hyper::upgrade::on(&mut res);
        let (rparts, _) = res.into_parts();
        if let Some(f) = &flow {
            f.response_headers(
                101,
                "HTTP/1.1",
                headers_vec(&rparts.headers),
                info.claim().then_some(&*info),
                flags::WEBSOCKET,
            );
        }
        let f2 = flow.clone();
        tokio::spawn(async move {
            let (c, u) = match (on_client.await, on_upstream.await) {
                (Ok(c), Ok(u)) => (c, u),
                (c, u) => {
                    let why = c
                        .err()
                        .or(u.err())
                        .map(|e| e.to_string())
                        .unwrap_or_default();
                    if let Some(f) = f2 {
                        f.fail(format!("Upgrade failed: {why}"));
                    }
                    return;
                }
            };
            let mut c = TokioIo::new(c);
            let mut u = TokioIo::new(u);
            let result = tokio::io::copy_bidirectional(&mut c, &mut u).await;
            if let Some(f) = f2 {
                match result {
                    Ok((up, down)) => f.finish_stream(up, down, None),
                    Err(e) => f.finish_stream(0, 0, Some(format!("Connection ended: {e}"))),
                }
            }
        });

        let mut out = Response::new(empty());
        *out.status_mut() = StatusCode::SWITCHING_PROTOCOLS;
        *out.headers_mut() = rparts.headers;
        out
    }
}
