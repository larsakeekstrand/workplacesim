//! Six small bitmap sprites blitted above sim heads. Priority ladder matches
//! `simGlyph` in public/main.js — first hit wins:
//! `!` > `flask` > `idle` > `walking` > `plan` > `long-seated`.
//!
//! Sprites are ASCII art: `#` draws, `.` skips. No unicode in the art.

use super::super::fx_store::FxStore;
use super::super::geometry::Rect;
use super::super::palette::{self, Rgb};
use super::super::sim_store::{SimAnim, SimState, SimStore};
use super::super::Framebuffer;
use super::h;
use crate::state::Agent;

pub const LONG_SEATED_MS: u64 = 60_000;

// `ERROR_GLYPH_MS` now comes from `Config::error_glyph_ms`; callers pass it
// as a parameter. Not re-exported as a module const because the value is
// ephemeral (a config toggle can change it between frames).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlyphKind {
    Error,
    VisitingLab,
    Idle,
    Walking,
    Plan,
    LongSeated,
}

/// Per-frame glyph resolution for a single sim. Priority ladder:
/// `!` (fresh tool-error) > flask (visit.room=="test") > idle (seated+idle) >
/// walking (…) > plan (seated+plan) > Z (seated > LONG_SEATED_MS). None = hide.
///
/// `error_glyph_ms` is pulled from `Config::error_glyph_ms` by the caller and
/// gates how long the red `!` stays up after a tool-error or halo.
pub fn sim_glyph(
    sim: &SimAnim,
    agent: Option<&Agent>,
    fx: &FxStore,
    now_ms: u64,
    error_glyph_ms: u64,
) -> Option<GlyphKind> {
    if let Some(halo) = fx.halos.iter().find(|h| h.agent_id == sim.agent_id) {
        if now_ms.saturating_sub(halo.born_ms) < error_glyph_ms {
            return Some(GlyphKind::Error);
        }
    }
    if let Some(a) = agent {
        if let Some(ce) = a.current_error.as_ref() {
            if now_ms.saturating_sub(ce.ts) < error_glyph_ms {
                return Some(GlyphKind::Error);
            }
        }
        if let Some(v) = a.visit.as_ref() {
            if v.room == "test" {
                return Some(GlyphKind::VisitingLab);
            }
        }
        let seated = matches!(sim.state, SimState::Seated);
        if a.idle == Some(true) && seated {
            return Some(GlyphKind::Idle);
        }
    }
    if matches!(sim.state, SimState::WalkingIn | SimState::WalkingOut) {
        return Some(GlyphKind::Walking);
    }
    if matches!(sim.state, SimState::Seated) && sim.permission_mode == "plan" {
        return Some(GlyphKind::Plan);
    }
    if let Some(since) = sim.seated_since_ms {
        if matches!(sim.state, SimState::Seated) && now_ms.saturating_sub(since) >= LONG_SEATED_MS {
            return Some(GlyphKind::LongSeated);
        }
    }
    None
}

/// Paint every alive sim's glyph. Called from `scene::sim::draw_sims` after the
/// body draws so the glyph sits on top.
pub fn draw_glyphs<F: Framebuffer>(
    fb: &mut F,
    sim_store: &SimStore,
    agents: &[&Agent],
    fx: &FxStore,
    now_ms: u64,
    error_glyph_ms: u64,
) {
    for sim in sim_store.iter() {
        if !sim.is_alive() {
            continue;
        }
        let agent = agents.iter().copied().find(|a| a.agent_id == sim.agent_id);
        let Some(kind) = sim_glyph(sim, agent, fx, now_ms, error_glyph_ms) else {
            continue;
        };
        let (art, color) = sprite(kind);
        // Anchor: above the sim's head. Offset is tuned for the 1280x720
        // canvas + SIM_SCALE=3.6 geometry — head extends ~16 render-px above
        // the sim anchor, plus the hair on top.
        let cx = h(sim.x as i32);
        let top = h(sim.y as i32) - 24;
        let w = art[0].len() as i32;
        draw_sprite(fb, art, cx - w / 2, top, color);
    }
}

fn sprite(kind: GlyphKind) -> (&'static [&'static str], Rgb) {
    match kind {
        GlyphKind::Error => (GLYPH_ERROR, palette::GLYPH_ERR),
        GlyphKind::VisitingLab => (GLYPH_FLASK, palette::GLYPH_LAB),
        GlyphKind::Idle => (GLYPH_ZZZ, palette::GLYPH_IDLE),
        GlyphKind::Walking => (GLYPH_DOTS, palette::GLYPH_WALK),
        GlyphKind::Plan => (GLYPH_CLIPBOARD, palette::GLYPH_PLAN),
        GlyphKind::LongSeated => (GLYPH_LONG, palette::GLYPH_LONG),
    }
}

fn draw_sprite<F: Framebuffer>(fb: &mut F, art: &[&str], x: i32, y: i32, color: Rgb) {
    for (row, line) in art.iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            if ch == '#' {
                fb.fill_rect(Rect::new(x + col as i32, y + row as i32, 1, 1), color);
            }
        }
    }
}

// Exclamation — red vertical bar + dot, 2x6. Rendered at 2px wide for punch.
pub const GLYPH_ERROR: &[&str] = &["##", "##", "##", "##", "..", "##"];

// Flask — small bottle silhouette, 7x8.
pub const GLYPH_FLASK: &[&str] = &[
    "..###..", "..#.#..", "..#.#..", ".##.##.", ".#...#.", ".#...#.", ".##.##.", "..###..",
];

// Three z letters, descending. 8x7.
pub const GLYPH_ZZZ: &[&str] = &[
    "....####", "...#..#.", "..###...", ".###....", "##......", "####....", "........",
];

// Three dots, horizontal. 7x2.
pub const GLYPH_DOTS: &[&str] = &["#.#.#.#", "#.#.#.#"];

// Clipboard outline, 6x8.
pub const GLYPH_CLIPBOARD: &[&str] = &[
    "..##..", ".####.", "######", "#....#", "#.##.#", "#....#", "#.##.#", "######",
];

// Stylised Z, 5x6.
pub const GLYPH_LONG: &[&str] = &["#####", "...#.", "..#..", ".#...", "#....", "#####"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::render::classify::Room;
    use crate::render::fx_store::{FxStore, Halo};
    use crate::render::geometry::Point;
    use crate::render::sim_store::SimAnim;
    use crate::state::{Agent, CurrentError, Visit};

    const ERROR_GLYPH_MS: u64 = config::DEFAULT_ERROR_GLYPH_MS;

    fn make_sim(state: SimState, mode: &str, seated_since: Option<u64>) -> SimAnim {
        SimAnim {
            agent_id: "a1".into(),
            session_id: None,
            user: "u".into(),
            permission_mode: mode.into(),
            is_lab: false,
            x: 0.0,
            y: 0.0,
            path: vec![Point::new(10, 10)],
            seat: None,
            room: Room::Desk,
            state,
            bob_phase: 0.0,
            spawned_at_ms: 0,
            seated_at_ms: seated_since,
            seated_since_ms: seated_since,
            overflow_hash: 0,
            last_footstep_ms: 0,
            session_label: None,
        }
    }

    fn agent_with(mut a: Agent) -> Agent {
        a.agent_id = "a1".into();
        a
    }

    #[test]
    fn error_beats_all() {
        let sim = make_sim(SimState::Seated, "plan", Some(0));
        let mut fx = FxStore::new();
        fx.halos.push(Halo {
            agent_id: "a1".into(),
            born_ms: 0,
        });
        let a = agent_with(Agent {
            visit: Some(Visit {
                room: "test".into(),
                until: 9_999,
            }),
            idle: Some(true),
            ..Default::default()
        });
        assert_eq!(
            sim_glyph(&sim, Some(&a), &fx, 500, ERROR_GLYPH_MS),
            Some(GlyphKind::Error)
        );
    }

    #[test]
    fn error_via_current_error_timestamp() {
        let sim = make_sim(SimState::Seated, "default", Some(0));
        let fx = FxStore::new();
        let a = agent_with(Agent {
            current_error: Some(CurrentError {
                tool_name: "Bash".into(),
                message: "boom".into(),
                ts: 1_000,
            }),
            ..Default::default()
        });
        assert_eq!(
            sim_glyph(&sim, Some(&a), &fx, 1_500, ERROR_GLYPH_MS),
            Some(GlyphKind::Error)
        );
        // Age past ERROR_GLYPH_MS: no longer an error.
        assert_ne!(
            sim_glyph(&sim, Some(&a), &fx, 5_000, ERROR_GLYPH_MS),
            Some(GlyphKind::Error)
        );
    }

    #[test]
    fn visiting_beats_idle() {
        let sim = make_sim(SimState::Seated, "default", Some(0));
        let fx = FxStore::new();
        let a = agent_with(Agent {
            visit: Some(Visit {
                room: "test".into(),
                until: 9_999,
            }),
            idle: Some(true),
            ..Default::default()
        });
        assert_eq!(
            sim_glyph(&sim, Some(&a), &fx, 500, ERROR_GLYPH_MS),
            Some(GlyphKind::VisitingLab)
        );
    }

    #[test]
    fn idle_beats_walking_for_seated_sim() {
        let sim = make_sim(SimState::Seated, "default", Some(0));
        let fx = FxStore::new();
        let a = agent_with(Agent {
            idle: Some(true),
            ..Default::default()
        });
        assert_eq!(
            sim_glyph(&sim, Some(&a), &fx, 500, ERROR_GLYPH_MS),
            Some(GlyphKind::Idle)
        );
    }

    #[test]
    fn walking_state_beats_plan() {
        let sim = make_sim(SimState::WalkingIn, "plan", None);
        let fx = FxStore::new();
        let a = agent_with(Agent::default());
        assert_eq!(
            sim_glyph(&sim, Some(&a), &fx, 500, ERROR_GLYPH_MS),
            Some(GlyphKind::Walking)
        );
    }

    #[test]
    fn plan_beats_long_seated() {
        let sim = make_sim(SimState::Seated, "plan", Some(0));
        let fx = FxStore::new();
        let a = agent_with(Agent::default());
        // Past LONG_SEATED_MS but plan glyph wins.
        assert_eq!(
            sim_glyph(&sim, Some(&a), &fx, LONG_SEATED_MS + 10, ERROR_GLYPH_MS),
            Some(GlyphKind::Plan)
        );
    }

    #[test]
    fn long_seated_only_when_threshold_met() {
        let sim = make_sim(SimState::Seated, "default", Some(0));
        let fx = FxStore::new();
        let a = agent_with(Agent::default());
        assert_eq!(
            sim_glyph(&sim, Some(&a), &fx, LONG_SEATED_MS - 1, ERROR_GLYPH_MS),
            None
        );
        assert_eq!(
            sim_glyph(&sim, Some(&a), &fx, LONG_SEATED_MS, ERROR_GLYPH_MS),
            Some(GlyphKind::LongSeated)
        );
    }

    #[test]
    fn no_glyph_for_freshly_seated_default_mode() {
        let sim = make_sim(SimState::Seated, "default", Some(0));
        let fx = FxStore::new();
        let a = agent_with(Agent::default());
        assert_eq!(sim_glyph(&sim, Some(&a), &fx, 100, ERROR_GLYPH_MS), None);
    }
}
