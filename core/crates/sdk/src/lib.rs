//! In-app SDK ingest. Runs inside `proxyd`, next to the proxy listener.
//!
//! Devices connect over TLS to the ingest port, pair once with a person
//! comparing a six-digit code, then push the calls their app makes. See
//! `docs/sdk/docs/03-protocol.md`.

pub mod adb;
pub mod assembler;
pub mod cert;
pub mod config;
pub mod devices;
pub mod discovery;
pub mod fake_client;
pub mod frame;
pub mod limits;
pub mod merge;
pub mod pairing;
pub mod record;
mod service;
mod session;
pub mod wire;

pub use assembler::{FetchOutcome, Identity, Stats, StatsSnapshot};
pub use config::{DeviceConfig, Redaction};
pub use limits::Limits;
pub use pairing::PendingPrompt;
pub use service::{
    ConnStatus, DeviceInfo, FetchError, PromptPolicy, Sdk, SdkError, SdkOptions, Snapshot, Status,
    Tap, DEFAULT_PORT,
};
