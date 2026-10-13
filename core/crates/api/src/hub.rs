use flow::{FlowEvent, FlowId, FlowSink, FlowSummary};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

/// What subscribers see. Deliberately light: no bodies.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum HubEvent {
    Upsert(FlowSummary),
    Cleared,
}

/// Fan-out of flow events to API subscribers. Implements [`FlowSink`], so it
/// sits on the data path without ever blocking it.
#[derive(Clone)]
pub struct Hub {
    tx: broadcast::Sender<HubEvent>,
    inflight: Arc<Mutex<HashMap<FlowId, FlowSummary>>>,
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
        self.inflight.lock().unwrap().clear();
        let _ = self.tx.send(HubEvent::Cleared);
    }
}

impl FlowSink for Hub {
    fn publish(&self, event: FlowEvent) {
        let summary = match event {
            FlowEvent::Started(s) | FlowEvent::Updated(s) => {
                self.inflight.lock().unwrap().insert(s.id, s.clone());
                s
            }
            FlowEvent::Finished(rec) => {
                self.inflight.lock().unwrap().remove(&rec.id);
                rec.summary()
            }
            FlowEvent::Cleared => {
                self.cleared();
                return;
            }
        };
        let _ = self.tx.send(HubEvent::Upsert(summary));
    }
}
