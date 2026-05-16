//! Runtime-tunable configuration for the Rust port.
//!
//! Foundation for a live-tunable config website (see the full plan at
//! `~/.claude/plans/i-want-to-add-ethereal-thimble.md`). This module is
//! deliberately self-contained: later tasks will rewire renderer reads and
//! server routes to pull from this config, at which point the current
//! hardcoded consts in `sim_store.rs`/`fx_store.rs`/`palette.rs`/
//! `desktop.rs`/`state/mod.rs` become `DEFAULT_*` consts referenced from
//! here. For now the defaults are duplicated so that this module lands
//! without forcing churn on the other modules.
//!
//! JSON shape is a single flat object — every field is `#[serde(default = …)]`
//! so partial JSON (missing fields) fills in with defaults, and nonsensical
//! out-of-range values are repaired by `Config::clamp`.

use serde::{Deserialize, Serialize};

pub mod persist;

// --- Motion ---
pub const DEFAULT_WALK_SPEED_PX_PER_SEC: f32 = 90.0;
pub const DEFAULT_MIN_SEGMENT_MS: u64 = 180;
pub const DEFAULT_BOB_CYCLE_MS: u64 = 1800;

// --- Effects ---
pub const DEFAULT_FOOTSTEP_LIFETIME_MS: u64 = 900;
pub const DEFAULT_FOOTSTEP_INTERVAL_MS: u64 = 120;
pub const DEFAULT_MOTE_LIFETIME_MS: u64 = 1200;
pub const DEFAULT_MOTE_CAP: usize = 40;
pub const DEFAULT_TETHER_LIFETIME_MS: u64 = 2000;
pub const DEFAULT_HALO_LIFETIME_MS: u64 = 2000;
pub const DEFAULT_ERROR_GLYPH_MS: u64 = 2000;

// --- Ticker ---
pub const DEFAULT_FILE_TICK_MS: u64 = 12_000;
pub const DEFAULT_FILE_TICK_CAP: usize = 3;
pub const DEFAULT_BENCH_FLASH_MS: u64 = 800;
pub const DEFAULT_WINDOW_SPILL_ALPHA: f32 = 0.18;

// --- TTLs ---
pub const DEFAULT_PENDING_TTL_MS: u64 = 60_000;
pub const DEFAULT_STOP_GRACE_MS: u64 = 10_000;
pub const DEFAULT_VISIT_MIN_MS: u64 = 1_000;
pub const DEFAULT_VISIT_MAX_MS: u64 = 120_000;
pub const DEFAULT_VISIT_DEFAULT_MS: u64 = 20_000;

// --- Display ---
pub const DEFAULT_WINDOW_W: u32 = 1280;
pub const DEFAULT_WINDOW_H: u32 = 720;
pub const DEFAULT_FULLSCREEN: bool = false;
pub const DEFAULT_TARGET_FPS: u32 = 30;

// Per-field serde defaults. `#[serde(default = "path")]` needs a named fn per
// field, so we have one tiny closure-body fn per const. Keeps the struct
// definition readable and partial-JSON parsing forward-compatible.
fn default_walk_speed_px_per_sec() -> f32 {
    DEFAULT_WALK_SPEED_PX_PER_SEC
}
fn default_min_segment_ms() -> u64 {
    DEFAULT_MIN_SEGMENT_MS
}
fn default_bob_cycle_ms() -> u64 {
    DEFAULT_BOB_CYCLE_MS
}
fn default_footstep_lifetime_ms() -> u64 {
    DEFAULT_FOOTSTEP_LIFETIME_MS
}
fn default_footstep_interval_ms() -> u64 {
    DEFAULT_FOOTSTEP_INTERVAL_MS
}
fn default_mote_lifetime_ms() -> u64 {
    DEFAULT_MOTE_LIFETIME_MS
}
fn default_mote_cap() -> usize {
    DEFAULT_MOTE_CAP
}
fn default_tether_lifetime_ms() -> u64 {
    DEFAULT_TETHER_LIFETIME_MS
}
fn default_halo_lifetime_ms() -> u64 {
    DEFAULT_HALO_LIFETIME_MS
}
fn default_error_glyph_ms() -> u64 {
    DEFAULT_ERROR_GLYPH_MS
}
fn default_file_tick_ms() -> u64 {
    DEFAULT_FILE_TICK_MS
}
fn default_file_tick_cap() -> usize {
    DEFAULT_FILE_TICK_CAP
}
fn default_bench_flash_ms() -> u64 {
    DEFAULT_BENCH_FLASH_MS
}
fn default_window_spill_alpha() -> f32 {
    DEFAULT_WINDOW_SPILL_ALPHA
}
fn default_pending_ttl_ms() -> u64 {
    DEFAULT_PENDING_TTL_MS
}
fn default_stop_grace_ms() -> u64 {
    DEFAULT_STOP_GRACE_MS
}
fn default_visit_min_ms() -> u64 {
    DEFAULT_VISIT_MIN_MS
}
fn default_visit_max_ms() -> u64 {
    DEFAULT_VISIT_MAX_MS
}
fn default_visit_default_ms() -> u64 {
    DEFAULT_VISIT_DEFAULT_MS
}
fn default_window_w() -> u32 {
    DEFAULT_WINDOW_W
}
fn default_window_h() -> u32 {
    DEFAULT_WINDOW_H
}
fn default_fullscreen() -> bool {
    DEFAULT_FULLSCREEN
}
fn default_target_fps() -> u32 {
    DEFAULT_TARGET_FPS
}

/// Flat, serde-friendly config record. Grouped below by rustdoc comments —
/// the JSON intentionally stays flat so partial POSTs (changing a single
/// field) are trivial to express.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    // --- Motion ---
    #[serde(default = "default_walk_speed_px_per_sec")]
    pub walk_speed_px_per_sec: f32,
    #[serde(default = "default_min_segment_ms")]
    pub min_segment_ms: u64,
    #[serde(default = "default_bob_cycle_ms")]
    pub bob_cycle_ms: u64,

    // --- Effects ---
    #[serde(default = "default_footstep_lifetime_ms")]
    pub footstep_lifetime_ms: u64,
    #[serde(default = "default_footstep_interval_ms")]
    pub footstep_interval_ms: u64,
    #[serde(default = "default_mote_lifetime_ms")]
    pub mote_lifetime_ms: u64,
    #[serde(default = "default_mote_cap")]
    pub mote_cap: usize,
    #[serde(default = "default_tether_lifetime_ms")]
    pub tether_lifetime_ms: u64,
    #[serde(default = "default_halo_lifetime_ms")]
    pub halo_lifetime_ms: u64,
    #[serde(default = "default_error_glyph_ms")]
    pub error_glyph_ms: u64,

    // --- Ticker ---
    #[serde(default = "default_file_tick_ms")]
    pub file_tick_ms: u64,
    #[serde(default = "default_file_tick_cap")]
    pub file_tick_cap: usize,
    #[serde(default = "default_bench_flash_ms")]
    pub bench_flash_ms: u64,
    #[serde(default = "default_window_spill_alpha")]
    pub window_spill_alpha: f32,

    // --- TTLs ---
    #[serde(default = "default_pending_ttl_ms")]
    pub pending_ttl_ms: u64,
    #[serde(default = "default_stop_grace_ms")]
    pub stop_grace_ms: u64,
    #[serde(default = "default_visit_min_ms")]
    pub visit_min_ms: u64,
    #[serde(default = "default_visit_max_ms")]
    pub visit_max_ms: u64,
    #[serde(default = "default_visit_default_ms")]
    pub visit_default_ms: u64,

    // --- Display ---
    #[serde(default = "default_window_w")]
    pub window_w: u32,
    #[serde(default = "default_window_h")]
    pub window_h: u32,
    #[serde(default = "default_fullscreen")]
    pub fullscreen: bool,
    #[serde(default = "default_target_fps")]
    pub target_fps: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            walk_speed_px_per_sec: DEFAULT_WALK_SPEED_PX_PER_SEC,
            min_segment_ms: DEFAULT_MIN_SEGMENT_MS,
            bob_cycle_ms: DEFAULT_BOB_CYCLE_MS,

            footstep_lifetime_ms: DEFAULT_FOOTSTEP_LIFETIME_MS,
            footstep_interval_ms: DEFAULT_FOOTSTEP_INTERVAL_MS,
            mote_lifetime_ms: DEFAULT_MOTE_LIFETIME_MS,
            mote_cap: DEFAULT_MOTE_CAP,
            tether_lifetime_ms: DEFAULT_TETHER_LIFETIME_MS,
            halo_lifetime_ms: DEFAULT_HALO_LIFETIME_MS,
            error_glyph_ms: DEFAULT_ERROR_GLYPH_MS,

            file_tick_ms: DEFAULT_FILE_TICK_MS,
            file_tick_cap: DEFAULT_FILE_TICK_CAP,
            bench_flash_ms: DEFAULT_BENCH_FLASH_MS,
            window_spill_alpha: DEFAULT_WINDOW_SPILL_ALPHA,

            pending_ttl_ms: DEFAULT_PENDING_TTL_MS,
            stop_grace_ms: DEFAULT_STOP_GRACE_MS,
            visit_min_ms: DEFAULT_VISIT_MIN_MS,
            visit_max_ms: DEFAULT_VISIT_MAX_MS,
            visit_default_ms: DEFAULT_VISIT_DEFAULT_MS,

            window_w: DEFAULT_WINDOW_W,
            window_h: DEFAULT_WINDOW_H,
            fullscreen: DEFAULT_FULLSCREEN,
            target_fps: DEFAULT_TARGET_FPS,
        }
    }
}

impl Config {
    /// Per-field [min, max, default] triples, mirroring `clamp()`. Exposed
    /// verbatim by `GET /api/config/bounds` so the config UI can populate
    /// slider ranges without hardcoding them. Keep this table in sync with
    /// `clamp()` — a mismatch means the UI lets users pick values the server
    /// will snap back.
    ///
    /// Each entry also carries a `restart_required` flag. The flag is `true`
    /// for display-pipeline fields (`window_w`, `window_h`, `fullscreen`) that
    /// the live renderer can't pick up without a rebind — on the fb build the
    /// kernel sets geometry so those fields are effectively ignored anyway,
    /// and on the desktop build an in-flight window may need a restart for
    /// the new size to "stick" cleanly. Every other field applies live.
    pub fn bounds() -> ConfigBounds {
        ConfigBounds {
            walk_speed_px_per_sec: Bounds::f32(10.0, 500.0, DEFAULT_WALK_SPEED_PX_PER_SEC, false),
            min_segment_ms: Bounds::u64(0, 2_000, DEFAULT_MIN_SEGMENT_MS, false),
            bob_cycle_ms: Bounds::u64(300, 10_000, DEFAULT_BOB_CYCLE_MS, false),

            footstep_lifetime_ms: Bounds::u64(100, 10_000, DEFAULT_FOOTSTEP_LIFETIME_MS, false),
            footstep_interval_ms: Bounds::u64(30, 2_000, DEFAULT_FOOTSTEP_INTERVAL_MS, false),
            mote_lifetime_ms: Bounds::u64(100, 10_000, DEFAULT_MOTE_LIFETIME_MS, false),
            mote_cap: Bounds::usize(0, 500, DEFAULT_MOTE_CAP, false),
            tether_lifetime_ms: Bounds::u64(100, 20_000, DEFAULT_TETHER_LIFETIME_MS, false),
            halo_lifetime_ms: Bounds::u64(100, 10_000, DEFAULT_HALO_LIFETIME_MS, false),
            error_glyph_ms: Bounds::u64(100, 10_000, DEFAULT_ERROR_GLYPH_MS, false),

            file_tick_ms: Bounds::u64(1_000, 120_000, DEFAULT_FILE_TICK_MS, false),
            file_tick_cap: Bounds::usize(0, 20, DEFAULT_FILE_TICK_CAP, false),
            bench_flash_ms: Bounds::u64(50, 5_000, DEFAULT_BENCH_FLASH_MS, false),
            window_spill_alpha: Bounds::f32(0.0, 1.0, DEFAULT_WINDOW_SPILL_ALPHA, false),

            pending_ttl_ms: Bounds::u64(1_000, 600_000, DEFAULT_PENDING_TTL_MS, false),
            stop_grace_ms: Bounds::u64(0, 120_000, DEFAULT_STOP_GRACE_MS, false),
            visit_min_ms: Bounds::u64(100, 60_000, DEFAULT_VISIT_MIN_MS, false),
            visit_max_ms: Bounds::u64(1_000, 600_000, DEFAULT_VISIT_MAX_MS, false),
            visit_default_ms: Bounds::u64(1_000, 300_000, DEFAULT_VISIT_DEFAULT_MS, false),

            window_w: Bounds::u32(320, 7_680, DEFAULT_WINDOW_W, true),
            window_h: Bounds::u32(180, 4_320, DEFAULT_WINDOW_H, true),
            fullscreen: BoundsBool {
                default: DEFAULT_FULLSCREEN,
                restart_required: true,
            },
            target_fps: Bounds::u32(5, 120, DEFAULT_TARGET_FPS, false),
        }
    }

    /// Snap every field into its safe range. Bounds mirror the "Range" column
    /// in the plan's Config-fields table. Called after load and after every
    /// incoming POST so a hand-edited JSON with silly values still renders.
    pub fn clamp(&mut self) {
        // Motion
        self.walk_speed_px_per_sec = self.walk_speed_px_per_sec.clamp(10.0, 500.0);
        self.min_segment_ms = self.min_segment_ms.clamp(0, 2_000);
        self.bob_cycle_ms = self.bob_cycle_ms.clamp(300, 10_000);

        // Effects
        self.footstep_lifetime_ms = self.footstep_lifetime_ms.clamp(100, 10_000);
        self.footstep_interval_ms = self.footstep_interval_ms.clamp(30, 2_000);
        self.mote_lifetime_ms = self.mote_lifetime_ms.clamp(100, 10_000);
        self.mote_cap = self.mote_cap.min(500);
        self.tether_lifetime_ms = self.tether_lifetime_ms.clamp(100, 20_000);
        self.halo_lifetime_ms = self.halo_lifetime_ms.clamp(100, 10_000);
        self.error_glyph_ms = self.error_glyph_ms.clamp(100, 10_000);

        // Ticker
        self.file_tick_ms = self.file_tick_ms.clamp(1_000, 120_000);
        self.file_tick_cap = self.file_tick_cap.min(20);
        self.bench_flash_ms = self.bench_flash_ms.clamp(50, 5_000);
        self.window_spill_alpha = self.window_spill_alpha.clamp(0.0, 1.0);

        // TTLs
        self.pending_ttl_ms = self.pending_ttl_ms.clamp(1_000, 600_000);
        self.stop_grace_ms = self.stop_grace_ms.clamp(0, 120_000);
        self.visit_min_ms = self.visit_min_ms.clamp(100, 60_000);
        self.visit_max_ms = self.visit_max_ms.clamp(1_000, 600_000);
        self.visit_default_ms = self.visit_default_ms.clamp(1_000, 300_000);

        // Display
        self.window_w = self.window_w.clamp(320, 7_680);
        self.window_h = self.window_h.clamp(180, 4_320);
        self.target_fps = self.target_fps.clamp(5, 120);
    }
}

/// Numeric field bounds surfaced by `GET /api/config/bounds`. One variant per
/// underlying Rust type so the JSON reads naturally on the wire (f32 stays a
/// float, u64 stays an integer) without forcing the UI to parse a stringly
/// union. `restart_required` is hoisted into every variant so the UI can tag
/// display-only fields without cross-referencing a separate map.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Bounds {
    F32 {
        min: f32,
        max: f32,
        default: f32,
        restart_required: bool,
    },
    U32 {
        min: u32,
        max: u32,
        default: u32,
        restart_required: bool,
    },
    U64 {
        min: u64,
        max: u64,
        default: u64,
        restart_required: bool,
    },
    Usize {
        min: usize,
        max: usize,
        default: usize,
        restart_required: bool,
    },
}

impl Bounds {
    pub fn f32(min: f32, max: f32, default: f32, restart_required: bool) -> Self {
        Self::F32 {
            min,
            max,
            default,
            restart_required,
        }
    }
    pub fn u32(min: u32, max: u32, default: u32, restart_required: bool) -> Self {
        Self::U32 {
            min,
            max,
            default,
            restart_required,
        }
    }
    pub fn u64(min: u64, max: u64, default: u64, restart_required: bool) -> Self {
        Self::U64 {
            min,
            max,
            default,
            restart_required,
        }
    }
    pub fn usize(min: usize, max: usize, default: usize, restart_required: bool) -> Self {
        Self::Usize {
            min,
            max,
            default,
            restart_required,
        }
    }
}

/// Boolean fields have no min/max — just a default. Shaped as a separate
/// struct so the UI can key off the JSON `kind` field and render a toggle
/// rather than a slider.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoundsBool {
    pub default: bool,
    pub restart_required: bool,
}

/// One per `Config` field. Returned by `GET /api/config/bounds` so the UI can
/// populate slider min/max/default without duplicating the table.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ConfigBounds {
    pub walk_speed_px_per_sec: Bounds,
    pub min_segment_ms: Bounds,
    pub bob_cycle_ms: Bounds,

    pub footstep_lifetime_ms: Bounds,
    pub footstep_interval_ms: Bounds,
    pub mote_lifetime_ms: Bounds,
    pub mote_cap: Bounds,
    pub tether_lifetime_ms: Bounds,
    pub halo_lifetime_ms: Bounds,
    pub error_glyph_ms: Bounds,

    pub file_tick_ms: Bounds,
    pub file_tick_cap: Bounds,
    pub bench_flash_ms: Bounds,
    pub window_spill_alpha: Bounds,

    pub pending_ttl_ms: Bounds,
    pub stop_grace_ms: Bounds,
    pub visit_min_ms: Bounds,
    pub visit_max_ms: Bounds,
    pub visit_default_ms: Bounds,

    pub window_w: Bounds,
    pub window_h: Bounds,
    pub fullscreen: BoundsBool,
    pub target_fps: Bounds,
}

/// Shared config handle. Wraps `Config` in an `Arc<RwLock<...>>` so the
/// render thread, the axum server, and the state module can all share one
/// live config. Parallel to the existing `Shared = Arc<RwLock<State>>`.
pub type SharedConfig = std::sync::Arc<parking_lot::RwLock<Config>>;

/// Helper for call sites that already hold a `Config` and want the shared
/// handle. Equivalent to `Arc::new(RwLock::new(cfg))`.
pub fn shared(cfg: Config) -> SharedConfig {
    std::sync::Arc::new(parking_lot::RwLock::new(cfg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_consts() {
        let c = Config::default();
        assert_eq!(c.walk_speed_px_per_sec, DEFAULT_WALK_SPEED_PX_PER_SEC);
        assert_eq!(c.min_segment_ms, DEFAULT_MIN_SEGMENT_MS);
        assert_eq!(c.bob_cycle_ms, DEFAULT_BOB_CYCLE_MS);

        assert_eq!(c.footstep_lifetime_ms, DEFAULT_FOOTSTEP_LIFETIME_MS);
        assert_eq!(c.footstep_interval_ms, DEFAULT_FOOTSTEP_INTERVAL_MS);
        assert_eq!(c.mote_lifetime_ms, DEFAULT_MOTE_LIFETIME_MS);
        assert_eq!(c.mote_cap, DEFAULT_MOTE_CAP);
        assert_eq!(c.tether_lifetime_ms, DEFAULT_TETHER_LIFETIME_MS);
        assert_eq!(c.halo_lifetime_ms, DEFAULT_HALO_LIFETIME_MS);
        assert_eq!(c.error_glyph_ms, DEFAULT_ERROR_GLYPH_MS);

        assert_eq!(c.file_tick_ms, DEFAULT_FILE_TICK_MS);
        assert_eq!(c.file_tick_cap, DEFAULT_FILE_TICK_CAP);
        assert_eq!(c.bench_flash_ms, DEFAULT_BENCH_FLASH_MS);
        assert_eq!(c.window_spill_alpha, DEFAULT_WINDOW_SPILL_ALPHA);

        assert_eq!(c.pending_ttl_ms, DEFAULT_PENDING_TTL_MS);
        assert_eq!(c.stop_grace_ms, DEFAULT_STOP_GRACE_MS);
        assert_eq!(c.visit_min_ms, DEFAULT_VISIT_MIN_MS);
        assert_eq!(c.visit_max_ms, DEFAULT_VISIT_MAX_MS);
        assert_eq!(c.visit_default_ms, DEFAULT_VISIT_DEFAULT_MS);

        assert_eq!(c.window_w, DEFAULT_WINDOW_W);
        assert_eq!(c.window_h, DEFAULT_WINDOW_H);
        assert_eq!(c.fullscreen, DEFAULT_FULLSCREEN);
        assert_eq!(c.target_fps, DEFAULT_TARGET_FPS);
    }

    #[test]
    fn round_trip_default() {
        let c = Config::default();
        let json = serde_json::to_string(&c).expect("serialize default");
        let back: Config = serde_json::from_str(&json).expect("deserialize default");
        assert_eq!(c, back);
    }

    #[test]
    fn partial_json_keeps_defaults_for_missing_fields() {
        let json = r#"{"walk_speed_px_per_sec": 45.0}"#;
        let parsed: Config = serde_json::from_str(json).expect("partial JSON must parse");
        assert_eq!(parsed.walk_speed_px_per_sec, 45.0);

        // Everything else should equal the default.
        let expected = Config {
            walk_speed_px_per_sec: 45.0,
            ..Config::default()
        };
        assert_eq!(parsed, expected);
    }

    #[test]
    fn corrupt_json_returns_err() {
        // Wrong type for a numeric field — serde must reject, not silently
        // default. The persist layer is what translates that Err into a
        // CorruptUsedDefaults load result.
        let json = r#"{"walk_speed_px_per_sec": "fast"}"#;
        let res: Result<Config, _> = serde_json::from_str(json);
        assert!(res.is_err(), "expected parse error, got {:?}", res);
    }

    #[test]
    fn clamp_snaps_out_of_range() {
        let mut c = Config {
            walk_speed_px_per_sec: 9999.0,
            ..Config::default()
        };
        c.clamp();
        assert_eq!(c.walk_speed_px_per_sec, 500.0);

        let mut c = Config {
            walk_speed_px_per_sec: -5.0,
            ..Config::default()
        };
        c.clamp();
        assert_eq!(c.walk_speed_px_per_sec, 10.0);

        let mut c = Config {
            mote_cap: 100_000,
            ..Config::default()
        };
        c.clamp();
        assert_eq!(c.mote_cap, 500);
    }

    #[test]
    fn shared_helper_wraps_config() {
        let c = Config::default();
        let s = shared(c.clone());
        assert_eq!(*s.read(), c);
    }

    /// The `restart_required` flag on `Bounds` and `BoundsBool` must mark the
    /// display-pipeline fields (`window_w`, `window_h`, `fullscreen`) as
    /// `true` — those can't live-apply without a service bounce — and leave
    /// everything else as `false`. The UI uses this flag to tag fields and
    /// decide whether "Save" needs to nudge the user toward the Restart
    /// button.
    #[test]
    fn bounds_mark_display_fields_restart_required() {
        let b = Config::bounds();

        fn restart_flag(b: &Bounds) -> bool {
            match b {
                Bounds::F32 {
                    restart_required, ..
                }
                | Bounds::U32 {
                    restart_required, ..
                }
                | Bounds::U64 {
                    restart_required, ..
                }
                | Bounds::Usize {
                    restart_required, ..
                } => *restart_required,
            }
        }

        assert!(restart_flag(&b.window_w), "window_w should be restart");
        assert!(restart_flag(&b.window_h), "window_h should be restart");
        assert!(
            b.fullscreen.restart_required,
            "fullscreen should be restart"
        );

        // Sample non-display field — must apply live.
        assert!(
            !restart_flag(&b.walk_speed_px_per_sec),
            "walk_speed_px_per_sec should NOT be restart-required"
        );
        assert!(
            !restart_flag(&b.target_fps),
            "target_fps should NOT be restart-required"
        );
    }

    /// `restart_required` must round-trip through serde so the UI sees it on
    /// the wire. Serialize the whole bounds blob to JSON and parse it back as
    /// a plain `Value`; confirm display fields surface `restart_required:
    /// true` and a sample motion field surfaces `false`.
    #[test]
    fn bounds_serialization_includes_restart_required() {
        let b = Config::bounds();
        let json = serde_json::to_string(&b).expect("serialize bounds");
        let v: serde_json::Value = serde_json::from_str(&json).expect("parse as value");
        assert_eq!(v["window_w"]["restart_required"].as_bool(), Some(true));
        assert_eq!(v["window_h"]["restart_required"].as_bool(), Some(true));
        assert_eq!(v["fullscreen"]["restart_required"].as_bool(), Some(true));
        assert_eq!(
            v["walk_speed_px_per_sec"]["restart_required"].as_bool(),
            Some(false),
        );
    }
}
