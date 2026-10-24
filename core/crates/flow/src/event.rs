use crate::model::{FlowRecord, FlowSummary};
use std::sync::Arc;

/// Lifecycle events published by the data path.
#[derive(Clone, Debug)]
pub enum FlowEvent {
    /// Request headers seen.
    Started(FlowSummary),
    /// Response headers seen, body still streaming.
    Updated(FlowSummary),
    /// Flow is complete or failed. Carries everything captured.
    Finished(Arc<FlowRecord>),
    /// Bounded preview of a response that is still streaming.
    Snapshot(Arc<FlowRecord>),
    /// A row stopped being a row of its own: the wire half of a merged
    /// SDK and proxy pair. The record stays stored.
    Removed(crate::FlowId),
    /// The store was wiped.
    Cleared,
}

/// Receives events from the data path. Implementations must not block.
pub trait FlowSink: Send + Sync + 'static {
    fn publish(&self, event: FlowEvent);
}

/// Discards everything. Handy for tests and benchmarks.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullSink;

impl FlowSink for NullSink {
    fn publish(&self, _event: FlowEvent) {}
}
