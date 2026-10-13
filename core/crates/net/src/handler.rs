//! Per-request path: resolve the target, record the flow, forward upstream,
//! tee the bodies, hand the response back.

use crate::body::{BodyEnd, BoxError, Tee};
use crate::connector::{self, Alpn, ConnInfo};
use crate::flowctx::{AppSlot, FlowCtx, NewFlow};
use crate::parse::{resolve_plain, resolve_tunneled, Target};
use crate::{Inner, ProxyBody};
use bytes::Bytes;
use flow::{flags, Intercept};
use http::header::{CONNECTION, CONTENT_TYPE, HOST, UPGRADE};
use http::{HeaderMap, HeaderName, Method, Request, Response, StatusCode, Uri, Version};
use http_body_util::{BodyExt, Empty, Full};
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
        .insert("x-proxy-error", "1".parse().unwrap());
    res
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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
        if matches!(ctx.kind, ConnKind::Plain) && self.is_self(&target) {
            return Ok(self.local_page(&target));
        }
        if is_upgrade(&req) {
            return Ok(self.proxy_upgrade(req, target, &ctx).await);
        }
        Ok(self.proxy_http(req, target, &ctx).await)
    }

    fn is_self(&self, t: &Target) -> bool {
        if t.scheme != "http" {
            return false;
        }
        if t.host == "proxy.local" {
            return true;
        }
        let listen = self.cfg.listen;
        t.port == listen.port()
            && (t.host == "localhost"
                || t.host
                    .parse::<IpAddr>()
                    .map(|ip| ip.is_loopback() || ip == listen.ip())
                    .unwrap_or(false))
    }

    fn local_page(&self, t: &Target) -> Response<ProxyBody> {
        match t.path() {
            "/ca.pem" | "/ca.crt" => {
                let mut res = Response::new(full(self.ca.cert_pem().to_string()));
                let h = res.headers_mut();
                h.insert(CONTENT_TYPE, "application/x-pem-file".parse().unwrap());
                h.insert(
                    "content-disposition",
                    "attachment; filename=\"proxy-ca.pem\"".parse().unwrap(),
                );
                res
            }
            "/" => {
                let html = format!(
                    "<!doctype html><meta charset=utf-8><title>Proxy</title>\
                     <body style=\"font:15px system-ui;max-width:42rem;margin:4rem auto;padding:0 1rem\">\
                     <h1 style=\"font-size:1.4rem\">Proxy is running</h1>\
                     <p><a href=\"/ca.pem\">Download the CA certificate</a></p>\
                     <p style=\"color:#666\">SHA-256 <code>{}</code></p>",
                    self.ca.sha256()
                );
                let mut res = Response::new(full(html));
                res.headers_mut()
                    .insert(CONTENT_TYPE, "text/html; charset=utf-8".parse().unwrap());
                res
            }
            _ => text_response(
                StatusCode::NOT_FOUND,
                "Not found",
                "This is the proxy itself.",
            ),
        }
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
        target: Target,
        ctx: &ConnCtx,
    ) -> Response<ProxyBody> {
        let (mut parts, body) = req.into_parts();
        let flow = self.begin_flow(&target, &parts, &ctx.app);

        if !self.chain.is_empty() {
            let summary = flow.as_ref().map(|f| f.summary()).unwrap_or_default();
            if let Intercept::Respond(r) = self.chain.run_request(&summary, &mut parts) {
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

        let req_body: ProxyBody = match &flow {
            Some(f) => Tee::new(body, f.req_cap.clone(), |_| {}).boxed_unsync(),
            None => body.map_err(|e| Box::new(e) as BoxError).boxed_unsync(),
        };
        if let Some(f) = &flow {
            f.dispatched();
        }
        match self
            .client
            .request(Request::from_parts(parts, req_body))
            .await
        {
            Ok(res) => self.forward_response(flow, res),
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

    fn forward_response(
        self: &Arc<Self>,
        flow: Option<Arc<FlowCtx>>,
        res: Response<Incoming>,
    ) -> Response<ProxyBody> {
        let (mut parts, body) = res.into_parts();
        if let Some(f) = &flow {
            if !self.chain.is_empty() {
                self.chain.run_response(&f.summary(), &mut parts);
            }
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
                ws,
            );
        }
        strip_hop_by_hop(&mut parts.headers, false);
        let body: ProxyBody = match flow {
            Some(f) => {
                let f2 = f.clone();
                Tee::new(body, f.res_cap.clone(), move |end| f2.finish_body(end)).boxed_unsync()
            }
            None => body.map_err(|e| Box::new(e) as BoxError).boxed_unsync(),
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
            return self.forward_response(flow, Response::from_parts(rparts, body));
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
