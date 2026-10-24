use flow::{FlowEvent, FlowId, FlowSink, FlowSummary};
use std::collections::{HashMap, BTreeSet};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

/// What subscribers see. Deliberately light: no bodies.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum HubEvent {
    Upsert(FlowSummary),
    /// The row is no longer listed on its own (merged into another).
    Remove(FlowId),
    Cleared,
}

/// Fan-out of flow events to API subscribers. Implements [`FlowSink`], so it
/// sits on the data path without ever blocking it.
#[derive(Clone)]
pub struct Hub {
    tx: broadcast::Sender<HubEvent>,
    inflight: Arc<Mutex<HashMap<FlowId, FlowSummary>>>,
    hidden: Arc<Mutex<BTreeSet<FlowId>>>,
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

impl Hub {
    pub fn new() -> Hub {
        let (tx, _) = broadcast::channel(8192);
        Hub {
            tx,
            inflight: Arc::new(Mutex::new(HashMap::new())),
            hidden: Arc::new(Mutex::new(BTreeSet::new())),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<HubEvent> {
        self.tx.subscribe()
    }

    /// Latest summary of a flow that has not finished yet.
    pub fn inflight(&self, id: FlowId) -> Option<FlowSummary> {
        self.inflight.lock().unwrap().get(&id).cloned()
    }

    pub fn inflight_count(&self) -> usize {
        self.inflight.lock().unwrap().len()
    }

    /// Announce that the store was wiped.
    pub fn cleared(&self) {
        self.hidden.lock().unwrap().clear();
        self.inflight.lock().unwrap().clear();
        let _ = self.tx.send(HubEvent::Cleared);
    }
}

impl FlowSink for Hub {
    fn publish(&self, event: FlowEvent) {
        if matches!(event, FlowEvent::Cleared) { self.cleared(); return; }
        // Serialize removals with deferred analysis notifications. A merged
        // wire row must not reappear when its storage worker finishes later.
        let mut hidden = self.hidden.lock().unwrap();
        let summary = match event {
            FlowEvent::Started(s) | FlowEvent::Updated(s) => {
                if hidden.contains(&s.id) { return; }
                self.inflight.lock().unwrap().insert(s.id, s.clone());
                s
            }
            FlowEvent::Snapshot(rec) => {
                if hidden.contains(&rec.id) { return; }
                let summary = rec.summary();
                self.inflight.lock().unwrap().insert(rec.id, summary.clone());
                summary
            }
            FlowEvent::Finished(rec) => {
                if hidden.contains(&rec.id) { return; }
                self.inflight.lock().unwrap().remove(&rec.id);
                rec.summary()
            }
            FlowEvent::Removed(id) => {
                hidden.insert(id);
                if hidden.len() > 8192 { hidden.pop_first(); }
                self.inflight.lock().unwrap().remove(&id);
                let _ = self.tx.send(HubEvent::Remove(id));
                return;
            }
            FlowEvent::Cleared => {
                unreachable!("cleared handled before acquiring hidden lock");
            }
        };
        let _ = self.tx.send(HubEvent::Upsert(summary));
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn analysis_does_not_resurrect_merged_wire_rows() {
        let hub=Hub::new();let mut rx=hub.subscribe();
        hub.publish(FlowEvent::Removed(1));
        assert!(matches!(rx.try_recv().unwrap(), HubEvent::Remove(1)));
        hub.publish(FlowEvent::Finished(Arc::new(flow::FlowRecord{id:1,..Default::default()})));
        assert!(rx.try_recv().is_err());
    }
}
