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
