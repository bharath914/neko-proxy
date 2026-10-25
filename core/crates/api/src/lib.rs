//! The client API: WebSocket + protobuf over a Unix domain socket.
//!
//! The desktop app, the CLI and (later) MCP all talk to the core through this
//! surface and nothing else.

mod approvals;
mod composer;
mod convert;
mod hub;
mod ops;
mod sdk_api;
mod server;

pub use compose;
pub use proto::pb;

pub use approvals::{Approval, ApprovalEvent, Approvals, Decision};
pub use convert::{rule_from_pb, rule_to_pb};
pub use hub::{Hub, HubEvent};
pub use ops::{replay_flow, ReplayEdits, Replayed};
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
    /// Send a request again as a new flow: `(flow id, status)`.
    fn replay(&self, call: ReplayCall) -> flow::BoxFuture<'_, Result<(u64, u16), String>>;
}

/// A request for the core to send.
#[derive(Clone, Debug)]
pub struct ReplayCall {
    pub method: String,
    /// Absolute URL.
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: bytes::Bytes,
}

/// Where a local AI client connects.
#[derive(Clone, Debug)]
pub struct McpEndpoint {
    pub url: String,
    pub token: String,
}

impl McpEndpoint {
    /// The command that registers this server with Claude Code.
    pub fn claude_command(&self) -> String {
        format!(
            "claude mcp add --transport http proxy {} --header \"Authorization: Bearer {}\"",
            self.url, self.token
        )
    }
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
    pub ids: Arc<flow::IdGen>,
    pub store: store::Store,
    pub hub: Hub,
    pub control: Arc<dyn Control>,
    pub rules: Arc<rules::RuleSet>,
    /// Requests written and saved in the app's composer.
    pub composer: Arc<compose::Composer>,
    pub approvals: Arc<Approvals>,
    /// None when the MCP server is not running.
    pub mcp: Option<McpEndpoint>,
    /// None when the core runs without the in-app SDK listener.
    pub sdk: Option<Arc<sdk::Sdk>>,
    pub info: CoreInfo,
}
