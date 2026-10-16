//! Wire types generated from `proto/sdk.proto`.
#![allow(clippy::all)]

include!(concat!(env!("OUT_DIR"), "/sdk.v1.rs"));

/// The protocol version this build speaks. Older SDKs are accepted down to
/// [`MIN_PROTOCOL_VERSION`]; bump the minimum only for incompatible changes.
pub const PROTOCOL_VERSION: u32 = 1;
pub const MIN_PROTOCOL_VERSION: u32 = 1;
