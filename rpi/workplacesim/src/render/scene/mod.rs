//! Static scene drawing. Ports `drawFloor` / `drawWalls` / `drawWindows` /
//! `drawDesk` / `drawMeetingRoom` / `drawLabRoom` from `public/main.js`.
//!
//! Coordinate convention: JS world coords (1280x640) land 1:1 in the
//! 1280x720 render frame — `h()` is identity, kept as a callsite for symmetry
//! and to leave the call graph stable if we ever rescale again. The world's
//! 80 px of vertical headroom (720 - 640) sits unused at the bottom of the
//! render canvas; the fb/desktop backends upscale aspect-preserving so that
//! headroom becomes letterboxing.

pub mod effects;
pub mod furniture;
pub mod glyph;
pub mod rooms;
pub mod sim;
pub mod text;

// Paint order is: static bg → fx::draw_below (footsteps + tethers) → sims →
// fx::draw_above (motes + halos). Mirrors `public/main.js`, where the shared
// `effects` Graphics layer renders trails and tethers below sim sprites while
// motes and halos are independent display objects placed above. Keeping motes
// on top makes them legible against bodies; tethers under bodies prevents the
// dashed line from striping the parent's silhouette.

use super::{palette, Framebuffer, RenderFrame};

/// Paint the static world (floor, walls, windows, fixed furniture, room
/// labels). The `window_spill_alpha` argument comes from
/// `Config::window_spill_alpha`; golden-frame tests pass
/// `crate::config::DEFAULT_WINDOW_SPILL_ALPHA` to keep the hardcoded default
/// reproducible.
///
/// Takes `&mut RenderFrame` (not `&mut impl Framebuffer`) because room labels
/// route through embedded-graphics' `DrawTarget`, which is only implemented
/// on the concrete type.
pub fn draw_static_background(fb: &mut RenderFrame, window_spill_alpha: f32) {
    fb.fill_rect(
        super::Rect::new(0, 0, fb.width() as i32, fb.height() as i32),
        palette::BG,
    );
    rooms::draw_floor(fb);
    rooms::draw_windows(fb, window_spill_alpha);
    rooms::draw_walls(fb);
    // Labels go between walls and room furniture so the meeting whiteboard
    // and lab bench paint over the labels (mirrors JS display-list order,
    // where `drawWalls` adds the label text before `drawMeetingRoom` /
    // `drawLabRoom` add the panel graphics).
    text::draw_room_labels(fb);
    furniture::draw_desks(fb);
    furniture::draw_meeting_room(fb);
    furniture::draw_lab_room(fb);
}

/// Map a JS world coordinate to the render-frame coordinate. Identity since
/// RENDER_W == WORLD_W; preserved as a callsite for symmetry.
#[inline]
pub(crate) fn h(v: i32) -> i32 {
    v
}

#[inline]
pub(crate) fn hr(r: super::Rect) -> super::Rect {
    super::Rect::new(h(r.x), h(r.y), h(r.w), h(r.h))
}
