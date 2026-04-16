//! Wall clock helper.
//!
//! JS callers used `Date.now()`; every state method here takes `now_ms: u64`
//! explicitly so tests are deterministic. Production callers thread through
//! `now_ms()` from this module.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current UNIX time in milliseconds. Panics if the system clock is before the
/// epoch — same failure mode as Node would give, and not something we need to
/// handle gracefully for a visualizer.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before UNIX epoch")
        .as_millis() as u64
}
