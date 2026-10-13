//! Changes an AI client wants to make, waiting for a person to allow them.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, oneshot};

#[derive(Debug)]
pub struct Approval {
    pub id: String,
    pub tool: String,
    pub title: String,
    /// Pretty-printed arguments, so the person sees exactly what would happen.
    pub detail: String,
    pub requested_ms: i64,
}

#[derive(Clone, Debug)]
pub enum ApprovalEvent {
    Requested(Arc<Approval>),
    Decided { id: String, allowed: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Allowed,
    Denied,
    /// Nobody answered in time. Treated as a refusal.
    TimedOut,
}

struct Slot {
    approval: Arc<Approval>,
    tx: oneshot::Sender<bool>,
}

pub struct Approvals {
    pending: Mutex<HashMap<String, Slot>>,
    events: broadcast::Sender<ApprovalEvent>,
    next: AtomicU64,
}

impl Default for Approvals {
    fn default() -> Self {
        let (events, _) = broadcast::channel(64);
        Approvals {
            pending: Mutex::new(HashMap::new()),
            events,
            next: AtomicU64::new(1),
        }
    }
}

impl Approvals {
    pub fn subscribe(&self) -> broadcast::Receiver<ApprovalEvent> {
        self.events.subscribe()
    }

    pub fn list(&self) -> Vec<Arc<Approval>> {
        let mut v: Vec<_> = self
            .pending
            .lock()
            .unwrap()
            .values()
            .map(|s| s.approval.clone())
            .collect();
        v.sort_by_key(|a| (a.requested_ms, a.id.clone()));
        v
    }

    /// Answer a pending request. False if it is gone (answered or expired).
    pub fn decide(&self, id: &str, allow: bool) -> bool {
        let slot = self.pending.lock().unwrap().remove(id);
        let Some(slot) = slot else { return false };
        let _ = slot.tx.send(allow);
        let _ = self.events.send(ApprovalEvent::Decided {
            id: id.to_string(),
            allowed: allow,
        });
        true
    }

    /// Wait for a person. Returns when they answer or `timeout` passes.
    pub async fn ask(
        &self,
        tool: &str,
        title: String,
        detail: String,
        timeout: Duration,
    ) -> Decision {
        let id = format!("a{}", self.next.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = oneshot::channel();
        let approval = Arc::new(Approval {
            id: id.clone(),
            tool: tool.to_string(),
            title,
            detail,
            requested_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
        });
        self.pending.lock().unwrap().insert(
            id.clone(),
            Slot {
                approval: approval.clone(),
                tx,
            },
        );
        let _ = self.events.send(ApprovalEvent::Requested(approval));
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(true)) => Decision::Allowed,
            Ok(Ok(false)) | Ok(Err(_)) => Decision::Denied,
            Err(_) => {
                if self.pending.lock().unwrap().remove(&id).is_some() {
                    let _ = self.events.send(ApprovalEvent::Decided {
                        id,
                        allowed: false,
                    });
                }
                Decision::TimedOut
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn allow_deny_and_expire() {
        let a = Arc::new(Approvals::default());
        let a2 = a.clone();
        let t = tokio::spawn(async move {
            a2.ask("create_mock", "Mock /x".into(), "{}".into(), Duration::from_secs(5))
                .await
        });
        while a.list().is_empty() {
            tokio::task::yield_now().await;
        }
        let id = a.list()[0].id.clone();
        assert!(a.decide(&id, true));
        assert_eq!(t.await.unwrap(), Decision::Allowed);
        assert!(!a.decide(&id, true), "answered once");

        let a2 = a.clone();
        let t = tokio::spawn(async move {
            a2.ask("add_breakpoint", "t".into(), "{}".into(), Duration::from_secs(5))
                .await
        });
        while a.list().is_empty() {
            tokio::task::yield_now().await;
        }
        let id = a.list()[0].id.clone();
        a.decide(&id, false);
        assert_eq!(t.await.unwrap(), Decision::Denied);

        let d = a
            .ask("x", "t".into(), "{}".into(), Duration::from_millis(20))
            .await;
        assert_eq!(d, Decision::TimedOut);
        assert!(a.list().is_empty());
    }
}
