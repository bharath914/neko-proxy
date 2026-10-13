use crate::model::FlowSummary;
use bytes::Bytes;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

/// Decides, per CONNECT, whether TLS is decrypted or tunneled untouched.
/// Pinned hosts are tunneled regardless; this is the person's own choice.
pub trait ConnectPolicy: Send + Sync + 'static {
    fn should_decrypt(&self, host: &str, port: u16) -> bool;
}

/// What a request interceptor decides.
pub enum Intercept {
    /// Pass the (possibly edited) request upstream.
    Continue,
    /// Answer locally without contacting upstream (mocks, Map Local).
    Respond(http::Response<Bytes>),
}

/// Runs before the request is forwarded. Rules plug in here; breakpoints use
/// the async [`Gate`].
pub trait RequestInterceptor: Send + Sync + 'static {
    fn on_request(&self, flow: &FlowSummary, parts: &mut http::request::Parts) -> Intercept;
}

/// Runs after response headers arrive, before they are returned to the client.
pub trait ResponseInterceptor: Send + Sync + 'static {
    fn on_response(&self, flow: &FlowSummary, parts: &mut http::response::Parts);
}

#[derive(Clone, Default)]
pub struct InterceptorChain {
    request: Vec<Arc<dyn RequestInterceptor>>,
    response: Vec<Arc<dyn ResponseInterceptor>>,
}

impl InterceptorChain {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.request.is_empty() && self.response.is_empty()
    }

    pub fn push_request(&mut self, i: Arc<dyn RequestInterceptor>) {
        self.request.push(i);
    }

    pub fn push_response(&mut self, i: Arc<dyn ResponseInterceptor>) {
        self.response.push(i);
    }

    pub fn run_request(&self, flow: &FlowSummary, parts: &mut http::request::Parts) -> Intercept {
        for i in &self.request {
            if let Intercept::Respond(r) = i.on_request(flow, parts) {
                return Intercept::Respond(r);
            }
        }
        Intercept::Continue
    }

    pub fn run_response(&self, flow: &FlowSummary, parts: &mut http::response::Parts) {
        for i in &self.response {
            i.on_response(flow, parts);
        }
    }
}

/// Left in `parts.extensions` by a request interceptor: wait this long before
/// the request goes upstream (or before a local answer is returned).
#[derive(Clone, Copy, Debug)]
pub struct Delay(pub Duration);

/// Left in `parts.extensions` by a request interceptor: send the request to a
/// different upstream. Unset fields keep the original value. The recorded flow
/// still shows what the client asked for.
#[derive(Clone, Debug, Default)]
pub struct Retarget {
    pub scheme: Option<&'static str>,
    pub host: Option<String>,
    pub port: Option<u16>,
}

/// Left in `parts.extensions` by an interceptor that changed the request, so
/// the flow can be marked as edited.
#[derive(Clone, Copy, Debug)]
pub struct Edited;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Request,
    Response,
}

/// A request or response held at a breakpoint, with its body.
#[derive(Clone, Debug, Default)]
pub struct Held {
    pub method: String,
    /// Path and query. Requests only.
    pub path_and_query: String,
    /// Responses only.
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
}

/// What may change while a message is held. Unset fields stay as they were.
#[derive(Clone, Debug, Default)]
pub struct Edits {
    pub method: Option<String>,
    pub path_and_query: Option<String>,
    pub status: Option<u16>,
    /// Replaces every header.
    pub headers: Option<Vec<(String, String)>>,
    pub body: Option<Bytes>,
}

impl Edits {
    pub fn is_empty(&self) -> bool {
        self.method.is_none()
            && self.path_and_query.is_none()
            && self.status.is_none()
            && self.headers.is_none()
            && self.body.is_none()
    }
}

pub enum Verdict {
    Continue(Edits),
    /// Drop the exchange; the client gets an error.
    Abort(String),
}

/// Why a message is being held and for how long at most.
#[derive(Clone, Debug)]
pub struct Ticket {
    pub rule_id: String,
    pub timeout: Duration,
}

/// Holds requests and responses until someone decides. Matching is
/// synchronous and cheap; only a match costs the data path an await.
pub trait Gate: Send + Sync + 'static {
    fn request_ticket(&self, flow: &FlowSummary, parts: &http::request::Parts) -> Option<Ticket>;
    fn response_ticket(&self, flow: &FlowSummary, parts: &http::response::Parts) -> Option<Ticket>;
    fn hold<'a>(
        &'a self,
        flow: &'a FlowSummary,
        phase: Phase,
        ticket: Ticket,
        held: Held,
    ) -> BoxFuture<'a, Verdict>;
}
