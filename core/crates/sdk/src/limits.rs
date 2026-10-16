//! Every bound that applies to input from a device. All of it is hostile
//! until a session is authenticated, and mostly even after.

use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Limits {
    /// Largest frame accepted (length prefix excluded).
    pub max_frame: usize,
    /// Calls in flight per device and app.
    pub max_inflight_calls: usize,
    /// Body bytes kept per call and direction, whatever the SDK sends.
    pub max_body_per_call: usize,
    /// Largest inflated body chunk.
    pub max_chunk_inflated: usize,
    /// Authenticated sessions.
    pub max_sessions: usize,
    /// Connections that have not authenticated yet, in total and per address.
    pub max_handshaking: usize,
    pub max_handshaking_per_ip: usize,
    /// Sustained events per second per session; the burst allowance equals one second.
    pub max_events_per_sec: u32,
    pub max_headers: usize,
    pub max_header_bytes: usize,
    pub max_url_len: usize,
    /// Free-text metadata fields (names, tags, screen...).
    pub max_field_len: usize,
    /// Completed calls remembered per device for late chunks and lazy fetch.
    pub max_completed_calls: usize,
    pub handshake_timeout: Duration,
    /// Time a person has to answer a pairing prompt.
    pub pair_timeout: Duration,
    pub idle_timeout: Duration,
    pub ping_interval: Duration,
    /// A frame that has started must finish arriving within this.
    pub frame_timeout: Duration,
    /// Time to wait for a lazy body before giving up.
    pub fetch_timeout: Duration,
    /// How long calls of a lost session stay open, in case it reconnects.
    pub grace: Duration,
    pub max_pending_prompts: usize,
    /// Minimum gap between prompts from one install after a deny or timeout.
    pub prompt_cooldown: Duration,
    /// Outbound frames queued per session before it is dropped as too slow.
    pub outbound_queue: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_frame: 1 << 20,
            max_inflight_calls: 256,
            max_body_per_call: 8 << 20,
            max_chunk_inflated: 256 << 10,
            max_sessions: 8,
            max_handshaking: 16,
            max_handshaking_per_ip: 4,
            max_events_per_sec: 10_000,
            max_headers: 200,
            max_header_bytes: 64 << 10,
            max_url_len: 8 << 10,
            max_field_len: 512,
            max_completed_calls: 1024,
            handshake_timeout: Duration::from_secs(10),
            pair_timeout: Duration::from_secs(60),
            idle_timeout: Duration::from_secs(60),
            ping_interval: Duration::from_secs(15),
            frame_timeout: Duration::from_secs(15),
            fetch_timeout: Duration::from_secs(10),
            grace: Duration::from_secs(10),
            max_pending_prompts: 5,
            prompt_cooldown: Duration::from_secs(20),
            outbound_queue: 256,
        }
    }
}
