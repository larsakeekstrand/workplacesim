//! workplacesim — Rust port.
//!
//! Step 1 exposes only the `state` module; HTTP and rendering come in
//! later steps. The module mirrors `server/state.js` semantics event-for-event.

pub mod state;

pub use state::{
    clock, new_state, Agent, BufferDescription, CurrentError, Event, Lifecycle, StartAgent, State,
    StopAgent, ToolEvent, VisitRoom, Visit,
};
