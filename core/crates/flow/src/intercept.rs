use crate::model::FlowSummary;
use bytes::Bytes;
use std::sync::Arc;

/// Decides, per CONNECT, whether TLS is decrypted or tunneled untouched.
/// Pinned hosts are tunneled regardless; this is the person's own choice.
pub trait ConnectPolicy: Send + Sync + 'static {
    fn should_decrypt(&self, host: &str, port: u16) -> bool;
}

/// What a request interceptor decides.
pub enum Intercept {
    /// Pass the (possibly edited) request upstream.
    Continue,
    /// Answer locally without contacting upstream. Reserved for mocks.
    Respond(http::Response<Bytes>),
}

/// Runs before the request is forwarded. Phase 1 ships an empty chain; rules,
/// breakpoints and mocks plug in here without touching the data path.
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
