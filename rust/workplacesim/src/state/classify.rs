//! Routing classifier — placeholder for step 1.
//!
//! The full classifier (room selection from `agent_type`/`description` plus
//! `permission_mode`) lands in step 3 alongside the geometry port. For now
//! only the keyword list is stabilised so the shared constant doesn't move
//! later.

/// Keywords that route a sim to the lab. Lifted from `public/main.js`
/// `LAB_KEYWORDS`; keep in sync if that list changes.
pub const LAB_KEYWORDS: &[&str] = &[
    "test", "spec", "review", "verify", "verifier", "lint", "bench", "analyzer", "hunter", "qa",
];

/// Room a sim should walk to. Real body of this function lands in step 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Room {
    Lab,
    Meeting,
    Desk,
}

// TODO(step 3): implement full `classify(agent) -> Room` using LAB_KEYWORDS
// + permission_mode == "plan" → Meeting + fallthrough to Desk. Mirror
// `classify()` in public/main.js.
pub fn classify(_agent_type: &str, _description: &str, _permission_mode: &str) -> Room {
    Room::Desk
}
