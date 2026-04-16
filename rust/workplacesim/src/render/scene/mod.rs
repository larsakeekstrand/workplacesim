//! Static scene drawing. Ports `drawFloor` / `drawWalls` / `drawWindows` /
//! `drawDesk` / `drawMeetingRoom` / `drawLabRoom` from `public/main.js`.
//!
//! Coordinate convention: the JS world is 1280x640, but the render frame is
//! 640x360 — every incoming JS coordinate is halved at draw time (`h()` /
//! `Rect::half()`). Layout constants stay in their JS-native values so the
//! geometry port stays byte-for-byte comparable with `public/main.js`.

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

use super::{palette, Framebuffer};

pub fn draw_static_background(fb: &mut impl Framebuffer) {
    fb.fill_rect(
        super::Rect::new(0, 0, fb.width() as i32, fb.height() as i32),
        palette::BG,
    );
    rooms::draw_floor(fb);
    rooms::draw_windows(fb);
    rooms::draw_walls(fb);
    furniture::draw_desks(fb);
    furniture::draw_meeting_room(fb);
    furniture::draw_lab_room(fb);
}

/// BUILD/OK panel text — separate from the rest of `draw_static_background` so
/// it renders through embedded-graphics (concrete `RenderFrame` only).
pub fn draw_static_text(fb: &mut super::RenderFrame) {
    text::draw_build_board(fb);
}

/// Halve a JS world coordinate to the render-frame coordinate.
#[inline]
pub(crate) fn h(v: i32) -> i32 {
    // Integer divide mirrors Phaser rasterisation for even inputs and rounds
    // toward zero for odd — good enough since our layout is grid-aligned.
    v / 2
}

#[inline]
pub(crate) fn hr(r: super::Rect) -> super::Rect {
    super::Rect::new(h(r.x), h(r.y), h(r.w), h(r.h))
}
