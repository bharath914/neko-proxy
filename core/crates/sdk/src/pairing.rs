//! Pairing: commit-reveal nonces, the code both screens show, and the
//! prompts waiting for a person.
//!
//! 1. SDK sends `commit = SHA-256(client_nonce)`; the core answers with its
//!    own nonce. The client nonce is committed before the server nonce is
//!    known, so a man in the middle cannot grind a certificate or a nonce
//!    that makes both sides show the same code.
//! 2. SDK reveals `client_nonce`; the core checks the commit.
//! 3. Both compute `code = SHA-256(fingerprint || client_nonce || server_nonce)`
//!    from the fingerprint *they* saw: first 4 bytes, big-endian, modulo
//!    1,000,000, zero padded to six digits.
//! 4. A person compares the codes and presses Allow.

use crate::limits::Limits;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;
use subtle::ConstantTimeEq;
use tokio::sync::oneshot;

pub const NONCE_LEN: usize = 32;

pub fn commit_of(client_nonce: &[u8]) -> [u8; 32] {
    Sha256::digest(client_nonce).into()
}

pub fn verify_commit(commit: &[u8], client_nonce: &[u8]) -> bool {
    client_nonce.len() == NONCE_LEN
        && commit.len() == 32
        && bool::from(commit.ct_eq(&commit_of(client_nonce)))
}

/// The six digits both sides display.
pub fn pairing_code(fingerprint: &[u8; 32], client_nonce: &[u8], server_nonce: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(fingerprint);
    h.update(client_nonce);
    h.update(server_nonce);
    let d = h.finalize();
    let n = u32::from_be_bytes([d[0], d[1], d[2], d[3]]) % 1_000_000;
    format!("{n:06}")
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("system randomness");
    b
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
}

/// What the desktop shows while a person decides.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PendingPrompt {
    pub id: u64,
    pub device_id: String,
    pub app_id: String,
    pub device_name: String,
    pub device_model: String,
    pub os_version: String,
    pub app_name: String,
    pub sdk_name: String,
    pub sdk_version: String,
    pub connection: String,
    pub remote_ip: String,
    pub code: String,
    pub created_ms: i64,
    pub expires_ms: i64,
    /// The install was paired before and is asking again.
    pub repair: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PromptError {
    /// A prompt for this install is already open.
    Pending,
    /// This install was denied or timed out a moment ago.
    Cooldown,
    /// Too many prompts are open.
    TooMany,
}

struct Open {
    prompt: PendingPrompt,
    tx: Option<oneshot::Sender<Decision>>,
}

#[derive(Default)]
struct Inner {
    next_id: u64,
    open: HashMap<u64, Open>,
    cooldown: HashMap<String, Instant>,
}

#[derive(Default)]
pub struct Prompts {
    inner: Mutex<Inner>,
}

impl Prompts {
    /// Open a prompt. The decision arrives on the returned receiver; if the
    /// prompt is cancelled or dropped, the receiver errors.
    pub fn open(
        &self,
        mut prompt: PendingPrompt,
        limits: &Limits,
    ) -> Result<(u64, oneshot::Receiver<Decision>), PromptError> {
        let mut g = self.inner.lock().unwrap();
        let now = Instant::now();
        g.cooldown.retain(|_, until| *until > now);
        if g.open
            .values()
            .any(|o| o.prompt.device_id == prompt.device_id && o.prompt.app_id == prompt.app_id)
        {
            return Err(PromptError::Pending);
        }
        if g.cooldown
            .contains_key(&install_key(&prompt.device_id, &prompt.app_id))
        {
            return Err(PromptError::Cooldown);
        }
        if g.open.len() >= limits.max_pending_prompts {
            return Err(PromptError::TooMany);
        }
        g.next_id += 1;
        let id = g.next_id;
        prompt.id = id;
        let (tx, rx) = oneshot::channel();
        g.open.insert(
            id,
            Open {
                prompt,
                tx: Some(tx),
            },
        );
        Ok((id, rx))
    }

    pub fn decide(&self, id: u64, decision: Decision, limits: &Limits) -> bool {
        let mut g = self.inner.lock().unwrap();
        let Some(mut o) = g.open.remove(&id) else {
            return false;
        };
        if decision == Decision::Deny {
            let key = install_key(&o.prompt.device_id, &o.prompt.app_id);
            g.cooldown
                .insert(key, Instant::now() + limits.prompt_cooldown);
        }
        if let Some(tx) = o.tx.take() {
            let _ = tx.send(decision);
        }
        true
    }

    /// Remove a prompt that ended some other way (timeout, session closed).
    pub fn close(&self, id: u64, timed_out: bool, limits: &Limits) {
        let mut g = self.inner.lock().unwrap();
        if let Some(o) = g.open.remove(&id) {
            if timed_out {
                let key = install_key(&o.prompt.device_id, &o.prompt.app_id);
                g.cooldown
                    .insert(key, Instant::now() + limits.prompt_cooldown);
            }
        }
    }

    pub fn list(&self) -> Vec<PendingPrompt> {
        let g = self.inner.lock().unwrap();
        let mut v: Vec<_> = g.open.values().map(|o| o.prompt.clone()).collect();
        v.sort_by_key(|p| p.id);
        v
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().open.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn install_key(device: &str, app: &str) -> String {
    format!("{device}\u{0}{app}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt(dev: &str) -> PendingPrompt {
        PendingPrompt {
            device_id: dev.into(),
            app_id: "com.x".into(),
            ..Default::default()
        }
    }

    #[test]
    fn code_is_six_digits_and_depends_on_every_input() {
        let fp = [7u8; 32];
        let cn = [1u8; 32];
        let sn = [2u8; 32];
        let c = pairing_code(&fp, &cn, &sn);
        assert_eq!(c.len(), 6);
        assert!(c.bytes().all(|b| b.is_ascii_digit()));
        assert_eq!(c, pairing_code(&fp, &cn, &sn));
        assert_ne!(c, pairing_code(&[8u8; 32], &cn, &sn));
        assert_ne!(c, pairing_code(&fp, &[3u8; 32], &sn));
        assert_ne!(c, pairing_code(&fp, &cn, &[4u8; 32]));
    }

    /// Known answer, shared with the Kotlin and Swift test suites so all
    /// three implementations agree on the derivation.
    #[test]
    fn known_answer() {
        let fp: Vec<u8> = (0u8..32).collect();
        let cn: Vec<u8> = (32u8..64).collect();
        let sn: Vec<u8> = (64u8..96).collect();
        let code = pairing_code(fp.as_slice().try_into().unwrap(), &cn, &sn);
        assert_eq!(code, KNOWN_CODE);
    }
    pub const KNOWN_CODE: &str = "730832";

    #[test]
    fn commit_checks() {
        let n = [9u8; NONCE_LEN];
        let c = commit_of(&n);
        assert!(verify_commit(&c, &n));
        assert!(!verify_commit(&c, &[8u8; NONCE_LEN]));
        assert!(!verify_commit(&c[..16], &n));
        assert!(!verify_commit(&c, &n[..16]));
    }

    #[test]
    fn prompts_limit_duplicates_cap_and_cooldown() {
        let limits = Limits::default();
        let p = Prompts::default();
        let (id, rx) = p.open(prompt("a"), &limits).unwrap();
        assert_eq!(
            p.open(prompt("a"), &limits).unwrap_err(),
            PromptError::Pending
        );
        assert!(p.decide(id, Decision::Allow, &limits));
        assert_eq!(rx.blocking_recv().unwrap(), Decision::Allow);
        assert!(!p.decide(id, Decision::Allow, &limits));

        let (id, _rx) = p.open(prompt("a"), &limits).unwrap();
        p.decide(id, Decision::Deny, &limits);
        assert_eq!(
            p.open(prompt("a"), &limits).unwrap_err(),
            PromptError::Cooldown
        );

        let mut held = Vec::new();
        for i in 0..limits.max_pending_prompts {
            held.push(p.open(prompt(&format!("d{i}")), &limits).unwrap());
        }
        assert_eq!(
            p.open(prompt("one-more"), &limits).unwrap_err(),
            PromptError::TooMany
        );
        assert_eq!(p.list().len(), limits.max_pending_prompts);
    }
}
