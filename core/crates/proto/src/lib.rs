//! Wire types for the client API, generated from `proto/api.proto`.
//! Shared by the core (server) and the desktop app (client).
#![allow(clippy::all)]

pub mod pb {
    include!(concat!(env!("OUT_DIR"), "/neko.api.v1.rs"));
}

pub use prost;
