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
