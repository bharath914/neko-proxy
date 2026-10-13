//! The client API: WebSocket + protobuf over a Unix domain socket.
//!
//! The desktop app, the CLI and (later) MCP all talk to the core through this
//! surface and nothing else.

mod convert;
mod hub;
mod server;

pub use proto::pb;

pub use hub::{Hub, HubEvent};
pub use server::{serve, ApiError};

use std::sync::Arc;
use std::time::Instant;

/// What the API needs to drive the data path, without depending on it.
pub trait Control: Send + Sync + 'static {
    fn recording(&self) -> bool;
    fn set_recording(&self, on: bool);
    fn pinned_hosts(&self) -> Vec<String>;
    fn clear_pinned(&self);
    /// Forget one pinned host; true if it was pinned.
    fn unpin_host(&self, host: &str) -> bool;
}

/// Static facts about the running core.
#[derive(Clone, Debug)]
pub struct CoreInfo {
    pub version: String,
    pub listen: String,
    pub data_dir: String,
    pub insecure_upstream: bool,
    pub ca_path: String,
    pub ca_sha256: String,
    pub ca_subject: String,
    pub ca_not_after: String,
    pub ca_pem: String,
    pub ca_spki_sha256: String,
    pub ca_key_storage: String,
    pub started: Instant,
}

pub struct ApiContext {
    pub store: store::Store,
    pub hub: Hub,
    pub control: Arc<dyn Control>,
    pub rules: Arc<rules::RuleSet>,
    pub info: CoreInfo,
}
