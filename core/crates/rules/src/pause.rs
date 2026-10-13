//! Messages held at breakpoints, waiting for a person (or an AI client the
//! person approved) to resume or abort them.

use flow::{Edits, FlowId, FlowSummary, Held, Phase, Verdict};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, oneshot};

/// One held message. Cheap to clone; shared with API subscribers.
#[derive(Debug)]
pub struct PausedFlow {
    pub flow_id: FlowId,
    pub rule_id: String,
    pub phase: Phase,
    pub summary: FlowSummary,
    pub held: Held,
    pub since_ms: i64,
    pub timeout_s: u64,
}

#[derive(Clone, Debug)]
pub enum PauseEvent {
    Paused(Arc<PausedFlow>),
    /// A held message went on its way. `how` is `resumed`, `edited`,
    /// `aborted`, `timeout` or `gone` (the client hung up).
    Released {
        flow_id: FlowId,
        phase: Phase,
        how: &'static str,
    },
}

struct Slot {
    paused: Arc<PausedFlow>,
    tx: oneshot::Sender<Verdict>,
}

pub struct Pauses {
    slots: Mutex<HashMap<FlowId, Slot>>,
    events: broadcast::Sender<PauseEvent>,
}

impl Default for Pauses {
    fn default() -> Self {
        let (events, _) = broadcast::channel(256);
        Pauses {
            slots: Mutex::new(HashMap::new()),
            events,
        }
    }
}

/// Removes the slot if the data path stops waiting (client hung up).
struct Guard<'a> {
    pauses: &'a Pauses,
    flow_id: FlowId,
    phase: Phase,
    armed: bool,
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.pauses.release(self.flow_id, self.phase, "gone");
        }
    }
}

impl Pauses {
    pub fn subscribe(&self) -> broadcast::Receiver<PauseEvent> {
        self.events.subscribe()
    }

    /// Everything held right now, oldest first.
    pub fn list(&self) -> Vec<Arc<PausedFlow>> {
        let mut v: Vec<_> = self
            .slots
            .lock()
            .unwrap()
            .values()
            .map(|s| s.paused.clone())
            .collect();
        v.sort_by_key(|p| (p.since_ms, p.flow_id));
        v
    }

    pub fn get(&self, flow_id: FlowId) -> Option<Arc<PausedFlow>> {
        self.slots
            .lock()
            .unwrap()
            .get(&flow_id)
            .map(|s| s.paused.clone())
    }

    /// Let a held message go. False if nothing was held for that flow.
    pub fn resume(&self, flow_id: FlowId, verdict: Verdict) -> bool {
        let slot = self.slots.lock().unwrap().remove(&flow_id);
        let Some(slot) = slot else { return false };
        let how = match &verdict {
            Verdict::Abort(_) => "aborted",
            Verdict::Continue(e) if e.is_empty() => "resumed",
            Verdict::Continue(_) => "edited",
        };
        let phase = slot.paused.phase;
        let delivered = slot.tx.send(verdict).is_ok();
        let _ = self.events.send(PauseEvent::Released {
            flow_id,
            phase,
            how: if delivered { how } else { "gone" },
        });
        true
    }

    fn release(&self, flow_id: FlowId, phase: Phase, how: &'static str) {
        if self.slots.lock().unwrap().remove(&flow_id).is_some() {
            let _ = self.events.send(PauseEvent::Released {
                flow_id,
                phase,
                how,
            });
        }
    }

    /// Hold until resumed, aborted or timed out. A timeout lets the message
    /// through unchanged so a forgotten breakpoint never wedges a client.
    pub async fn hold(
        &self,
        summary: &FlowSummary,
        rule_id: String,
        phase: Phase,
        timeout: std::time::Duration,
        held: Held,
    ) -> Verdict {
        let (tx, rx) = oneshot::channel();
        let paused = Arc::new(PausedFlow {
            flow_id: summary.id,
            rule_id,
            phase,
            summary: summary.clone(),
            held,
            since_ms: now_ms(),
            timeout_s: timeout.as_secs(),
        });
        self.slots.lock().unwrap().insert(
            summary.id,
            Slot {
                paused: paused.clone(),
                tx,
            },
        );
        let _ = self.events.send(PauseEvent::Paused(paused));
        let mut guard = Guard {
            pauses: self,
            flow_id: summary.id,
            phase,
            armed: true,
        };
        let verdict = match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(v)) => {
                guard.armed = false;
                v
            }
            Ok(Err(_)) => Verdict::Continue(Edits::default()),
            Err(_) => {
                self.release(summary.id, phase, "timeout");
                guard.armed = false;
                Verdict::Continue(Edits::default())
            }
        };
        verdict
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
