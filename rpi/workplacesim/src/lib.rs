//! workplacesim — Rust port.
//!
//! Step 2 wires `state` up behind an axum HTTP server that mirrors
//! `server/index.js`'s endpoint surface 1:1. SSE and static serving are
//! deferred to step 7.

pub mod config;
pub mod render;
pub mod server;
pub mod state;

#[cfg(any(feature = "desktop", feature = "fb"))]
pub use server::Shared;

pub use state::{
    clock, new_state, Agent, BufferDescription, CurrentError, Event, Lifecycle, Pretool,
    PretoolToolInput, StartAgent, State, StopAgent, ToolEvent, Visit, VisitRoom,
};
