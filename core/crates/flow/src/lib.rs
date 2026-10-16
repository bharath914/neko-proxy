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
    BoxFuture, ConnectPolicy, Delay, Edited, Edits, Gate, Held, Intercept, InterceptorChain, Phase,
    RequestInterceptor, ResponseInterceptor, Retarget, Ticket, Verdict,
};
pub use model::{
    flags, short_content_type, BodyState, FlowId, FlowRecord, FlowState, FlowSummary, Header, IdGen,
    SdkInfo, SdkTimings, Source, Timings, SLOW_MS,
};
