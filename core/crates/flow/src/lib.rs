//! Flow model, lifecycle events and the interceptor pipeline.
//!
//! This crate has no I/O. The data path (`net`) builds [`FlowRecord`]s, runs
//! the [`InterceptorChain`] and hands events to a [`FlowSink`]. Everything
//! else (store, API) hangs off the sink so the data path never waits on them.

mod event;
mod intercept;
mod model;

pub use event::{FlowEvent, FlowSink, NullSink};
pub use intercept::{
    ConnectPolicy, Intercept, InterceptorChain, RequestInterceptor, ResponseInterceptor,
};
pub use model::{
    flags, short_content_type, FlowId, FlowRecord, FlowState, FlowSummary, Header, IdGen, Timings,
    SLOW_MS,
};
