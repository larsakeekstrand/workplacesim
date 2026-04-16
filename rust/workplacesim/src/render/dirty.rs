//! Dirty-rect compositor helpers. Used by the /dev/fb0 backend to cut memory
//! bandwidth on Pi 1: only rects whose contents changed between frames are
//! re-blitted to the framebuffer. Desktop (minifb) still does full-frame.
//!
//! Coordinates here are render-frame px (half JS world). Rects include the
//! sim and effect sprite bounds — exact pixel-perfect bounds aren't required,
//! a conservative AABB just rasterises some extra background per frame.

use super::fx_store::FxStore;
use super::geometry::Rect;
use super::sim_store::{SimAnim, SimStore};
use super::scene::h;
use super::{RENDER_H, RENDER_W};

/// AABB padding (render px). Sprites outgrow their nominal bounds during
/// walk-cycle leg swings, halo expansion, mote lift, etc. Pad conservatively
/// to avoid tearing; the cost is a few extra background pixels per dirty
/// region, which is negligible next to the blit-avoidance win.
const SIM_PADDING: i32 = 4;
const FX_PADDING: i32 = 3;

/// Rect dimensions that bound a sim sprite (body + head + hair + shadow),
/// plus padding. Mirrors `scene::sim::draw_sim` geometry. Expressed as half-
/// widths / half-heights around the sim anchor.
const SIM_HALF_W: i32 = 6;
const SIM_UP: i32 = 10;
const SIM_DOWN: i32 = 8;

/// Radius (render px) bounding a halo at maximum expansion (3 + 8 * t, t=1).
const HALO_MAX_R: i32 = 12;

/// Motes drift up by 24 world px over their lifetime → 12 render px.
const MOTE_UP: i32 = 12;

/// Keeps the previous-frame dirties so the new frame can erase them back to
/// background before redrawing dynamics.
pub struct DirtyTracker {
    last: Vec<Rect>,
}

impl Default for DirtyTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl DirtyTracker {
    pub fn new() -> Self {
        Self { last: Vec::new() }
    }

    /// Next frame's dirty set. Union of: this frame's dynamics AABBs + the
    /// previous frame's AABBs (so we repaint areas the sim just vacated).
    /// Returned rects are merged and clipped to the render viewport.
    pub fn step(&mut self, sim_store: &SimStore, fx: &FxStore) -> Vec<Rect> {
        let this_frame = collect_dynamics(sim_store, fx);
        let mut combined = this_frame.clone();
        combined.extend(self.last.iter().copied());
        let merged = merge_rects(combined);
        // Save only this frame's AABBs as "last", so next iteration's union
        // covers this frame's current positions for the erase pass.
        self.last = this_frame;
        merged
    }

    /// Inspection hook for tests and debug logging.
    #[allow(dead_code)]
    pub fn last(&self) -> &[Rect] {
        &self.last
    }
}

/// Collect AABBs for everything that moves: sims + motes + footsteps +
/// tethers + halos. Clipped to the viewport; empty rects are dropped.
pub fn collect_dynamics(sim_store: &SimStore, fx: &FxStore) -> Vec<Rect> {
    let mut out: Vec<Rect> = Vec::with_capacity(sim_store.anim.len() + fx.footsteps.len() + fx.motes.len());

    for sim in sim_store.iter() {
        if let Some(r) = sim_aabb(sim) {
            out.push(r);
        }
    }

    for f in &fx.footsteps {
        let cx = h(f.x as i32);
        let cy = h(f.y as i32);
        out.push(clip(Rect::new(cx - FX_PADDING, cy - FX_PADDING, FX_PADDING * 2, FX_PADDING * 2)));
    }

    for m in &fx.motes {
        let cx = h(m.x as i32);
        let cy = h(m.y as i32);
        // Motes drift upward over their lifetime; include the full travel.
        out.push(clip(Rect::new(
            cx - FX_PADDING,
            cy - MOTE_UP - FX_PADDING,
            FX_PADDING * 2,
            MOTE_UP + FX_PADDING * 2,
        )));
    }

    for t in &fx.tethers {
        let (Some(parent), Some(child)) = (
            sim_store.anim.get(&t.parent),
            sim_store.anim.get(&t.child),
        ) else {
            continue;
        };
        out.push(line_aabb(parent, child));
    }

    for halo in &fx.halos {
        let Some(sim) = sim_store.anim.get(&halo.agent_id) else {
            continue;
        };
        let cx = h(sim.x as i32);
        let cy = h(sim.y as i32);
        out.push(clip(Rect::new(
            cx - HALO_MAX_R,
            cy - HALO_MAX_R,
            HALO_MAX_R * 2,
            HALO_MAX_R * 2,
        )));
    }

    out.retain(|r| r.w > 0 && r.h > 0);
    out
}

fn sim_aabb(sim: &SimAnim) -> Option<Rect> {
    let cx = h(sim.x as i32);
    let cy = h(sim.y as i32);
    let r = Rect::new(
        cx - SIM_HALF_W - SIM_PADDING,
        cy - SIM_UP - SIM_PADDING,
        (SIM_HALF_W * 2) + SIM_PADDING * 2,
        (SIM_UP + SIM_DOWN) + SIM_PADDING * 2,
    );
    let clipped = clip(r);
    if clipped.w <= 0 || clipped.h <= 0 {
        None
    } else {
        Some(clipped)
    }
}

fn line_aabb(a: &SimAnim, b: &SimAnim) -> Rect {
    let x0 = h(a.x as i32).min(h(b.x as i32));
    let x1 = h(a.x as i32).max(h(b.x as i32));
    let y0 = h(a.y as i32).min(h(b.y as i32));
    let y1 = h(a.y as i32).max(h(b.y as i32));
    clip(Rect::new(
        x0 - FX_PADDING,
        y0 - FX_PADDING,
        (x1 - x0) + FX_PADDING * 2,
        (y1 - y0) + FX_PADDING * 2,
    ))
}

/// Compute the full dirty set for a frame: dynamics AABBs ∪ last frame's
/// dynamics AABBs, merged. Pure function — the caller owns `last_dirty`. Use
/// `DirtyTracker::step` for the common stateful path.
pub fn compute_frame_dirties(
    sim_store: &SimStore,
    fx: &FxStore,
    last_dirty: &[Rect],
) -> Vec<Rect> {
    let mut combined = collect_dynamics(sim_store, fx);
    combined.extend(last_dirty.iter().copied());
    merge_rects(combined)
}

/// Clip a rect to the render viewport. May return zero-area if fully outside.
fn clip(r: Rect) -> Rect {
    let x0 = r.x.max(0);
    let y0 = r.y.max(0);
    let x1 = (r.x + r.w).min(RENDER_W as i32);
    let y1 = (r.y + r.h).min(RENDER_H as i32);
    Rect::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
}

/// Two rects touch or overlap if their expanded-by-one bounds intersect.
/// Adjacent rects merge so we don't push two fb blits where one will do.
fn touching(a: Rect, b: Rect) -> bool {
    !(a.x + a.w < b.x || b.x + b.w < a.x || a.y + a.h < b.y || b.y + b.h < a.y)
}

fn union(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.w).max(b.x + b.w);
    let y1 = (a.y + a.h).max(b.y + b.h);
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// O(n²) sweep merging touching/overlapping rects. Typical n < 60, so this
/// is fine; a sweep-line would be needed at n > a few hundred.
pub fn merge_rects(mut rects: Vec<Rect>) -> Vec<Rect> {
    // Drop zero-area; they don't contribute but break `touching` logic.
    rects.retain(|r| r.w > 0 && r.h > 0);

    let mut changed = true;
    while changed {
        changed = false;
        let mut i = 0;
        while i < rects.len() {
            let mut j = i + 1;
            while j < rects.len() {
                if touching(rects[i], rects[j]) {
                    rects[i] = union(rects[i], rects[j]);
                    rects.swap_remove(j);
                    changed = true;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::classify::Room;
    use crate::render::fx_store::{Footstep, Halo, Mote, Tether};
    use crate::render::geometry::Point;
    use crate::render::palette::Rgb;
    use crate::render::sim_store::{SimAnim, SimState};

    fn mk_sim(id: &str, x: f32, y: f32) -> SimAnim {
        SimAnim {
            agent_id: id.into(),
            session_id: None,
            user: id.into(),
            permission_mode: "default".into(),
            is_lab: false,
            x,
            y,
            path: vec![Point::new(200, 100)],
            seat: None,
            room: Room::Desk,
            state: SimState::WalkingIn,
            bob_phase: 0.0,
            spawned_at_ms: 0,
            seated_at_ms: None,
            seated_since_ms: None,
            overflow_hash: 0,
            last_footstep_ms: 0,
        }
    }

    #[test]
    fn merge_touching_rects() {
        // Two rects sharing an edge (a.right + 1 == b.x is still "touching"
        // per the expand-by-one rule).
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(10, 0, 10, 10);
        let m = merge_rects(vec![a, b]);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0], Rect::new(0, 0, 20, 10));
    }

    #[test]
    fn merge_overlapping_rects() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 5, 10, 10);
        let m = merge_rects(vec![a, b]);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0], Rect::new(0, 0, 15, 15));
    }

    #[test]
    fn merge_disjoint_rects_keeps_both() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(50, 50, 10, 10);
        let m = merge_rects(vec![a, b]);
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn merge_chain_of_rects_collapses_to_one() {
        // Three rects, a-b touch, b-c touch, a-c don't — one pass must still
        // collapse all three (the while-changed loop handles this).
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(10, 0, 10, 10);
        let c = Rect::new(20, 0, 10, 10);
        let m = merge_rects(vec![a, b, c]);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0], Rect::new(0, 0, 30, 10));
    }

    #[test]
    fn merge_drops_zero_area_rects() {
        let a = Rect::new(0, 0, 0, 0);
        let b = Rect::new(5, 5, 5, 0);
        let c = Rect::new(20, 20, 10, 10);
        let m = merge_rects(vec![a, b, c]);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0], c);
    }

    #[test]
    fn compute_frame_dirties_empty_on_empty_stores() {
        let sim_store = SimStore::new();
        let fx = FxStore::new();
        let d = compute_frame_dirties(&sim_store, &fx, &[]);
        assert!(d.is_empty(), "empty stores + empty last = no dirty rects");
    }

    #[test]
    fn compute_frame_dirties_covers_sim_position() {
        let mut sim_store = SimStore::new();
        sim_store.anim.insert("a".into(), mk_sim("a", 200.0, 200.0));
        let fx = FxStore::new();
        let d = compute_frame_dirties(&sim_store, &fx, &[]);
        assert_eq!(d.len(), 1);
        // (200,200) JS world → (100,100) render. AABB must contain (100,100).
        let r = d[0];
        assert!(r.contains(Point::new(100, 100)), "rect {:?} should contain (100,100)", r);
    }

    #[test]
    fn compute_frame_dirties_covers_sim_positions_and_last_frame() {
        // Simulate a walk: sim was at (100,100) last frame (last_dirty),
        // now at (200,100). Dirty set must cover both old and new render
        // positions so the old pixels get erased.
        let mut sim_store = SimStore::new();
        sim_store.anim.insert("a".into(), mk_sim("a", 200.0, 100.0));
        let fx = FxStore::new();
        // Last frame's AABB — conservative rect at (old_x/2, old_y/2).
        let last = vec![Rect::new(40, 40, 20, 20)];
        let d = compute_frame_dirties(&sim_store, &fx, &last);
        // The last rect is far enough from the new sim position that they
        // shouldn't merge: two distinct rects expected.
        assert!(!d.is_empty(), "at least one rect");
        // Either merged (one rect covers both) or separate (both present).
        let covers_old = d.iter().any(|r| r.contains(Point::new(50, 50)));
        // new sim at (100, 50) render coords
        let covers_new = d.iter().any(|r| r.contains(Point::new(100, 50)));
        assert!(covers_old, "must cover old AABB at (50,50)");
        assert!(covers_new, "must cover new sim at (100,50)");
    }

    #[test]
    fn compute_frame_dirties_includes_footstep() {
        let sim_store = SimStore::new();
        let mut fx = FxStore::new();
        fx.footsteps.push(Footstep {
            agent_id: "a".into(),
            x: 400.0,
            y: 300.0,
            color: Rgb(0, 0, 0),
            born_ms: 0,
        });
        let d = compute_frame_dirties(&sim_store, &fx, &[]);
        assert_eq!(d.len(), 1);
        // (400,300) JS → (200,150) render.
        assert!(d[0].contains(Point::new(200, 150)));
    }

    #[test]
    fn compute_frame_dirties_includes_mote() {
        let sim_store = SimStore::new();
        let mut fx = FxStore::new();
        fx.motes.push_back(Mote {
            agent_id: "a".into(),
            x: 400.0,
            y: 300.0,
            color: Rgb(0, 0, 0),
            born_ms: 0,
        });
        let d = compute_frame_dirties(&sim_store, &fx, &[]);
        assert_eq!(d.len(), 1);
        // Mote AABB includes the upward drift band: y from (render_y - MOTE_UP - pad).
        assert!(d[0].contains(Point::new(200, 150)));
    }

    #[test]
    fn compute_frame_dirties_includes_halo() {
        let mut sim_store = SimStore::new();
        sim_store.anim.insert("a".into(), mk_sim("a", 400.0, 300.0));
        let mut fx = FxStore::new();
        fx.halos.push(Halo {
            agent_id: "a".into(),
            born_ms: 0,
        });
        let d = compute_frame_dirties(&sim_store, &fx, &[]);
        // Sim + halo AABBs will merge (halo centred at sim).
        assert_eq!(d.len(), 1);
        // Halo radius (max) extends the AABB beyond the sim body.
        let r = d[0];
        assert!(r.w >= HALO_MAX_R * 2, "halo AABB width {} < {}", r.w, HALO_MAX_R * 2);
    }

    #[test]
    fn compute_frame_dirties_skips_tether_with_missing_endpoint() {
        // Tether referencing a non-existent child sim → ignored.
        let mut sim_store = SimStore::new();
        sim_store.anim.insert("parent".into(), mk_sim("parent", 100.0, 100.0));
        let mut fx = FxStore::new();
        fx.tethers.push(Tether {
            parent: "parent".into(),
            child: "missing".into(),
            born_ms: 0,
        });
        let d = compute_frame_dirties(&sim_store, &fx, &[]);
        // Only the parent sim's AABB; no line AABB for the broken tether.
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn clip_removes_out_of_viewport_rects() {
        // Sim far outside the viewport → AABB clips to empty → dropped.
        let mut sim_store = SimStore::new();
        sim_store
            .anim
            .insert("out".into(), mk_sim("out", 10_000.0, 10_000.0));
        let fx = FxStore::new();
        let d = compute_frame_dirties(&sim_store, &fx, &[]);
        assert!(d.is_empty());
    }

    #[test]
    fn dirty_tracker_saves_and_unions_last() {
        let mut tracker = DirtyTracker::new();
        let mut sim_store = SimStore::new();
        let fx = FxStore::new();

        sim_store.anim.insert("a".into(), mk_sim("a", 100.0, 100.0));
        let frame1 = tracker.step(&sim_store, &fx);
        assert_eq!(frame1.len(), 1);

        // Move the sim far enough that the old rect and new rect don't merge.
        sim_store.anim.get_mut("a").unwrap().x = 600.0;
        let frame2 = tracker.step(&sim_store, &fx);
        // frame2 should contain both the old (50,50-ish) and new (300,50-ish) positions.
        let covers_old = frame2.iter().any(|r| r.contains(Point::new(50, 50)));
        let covers_new = frame2.iter().any(|r| r.contains(Point::new(300, 50)));
        assert!(covers_old);
        assert!(covers_new);

        // Third frame: tracker.last should now only contain the new position.
        // So if the sim doesn't move, frame3 contains exactly one rect around new.
        let frame3 = tracker.step(&sim_store, &fx);
        assert_eq!(frame3.len(), 1);
        assert!(frame3[0].contains(Point::new(300, 50)));
        assert!(!frame3[0].contains(Point::new(50, 50)));
    }
}
