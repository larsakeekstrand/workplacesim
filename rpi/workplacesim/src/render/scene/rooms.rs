//! Floor, walls, windows. 1:1 port of drawFloor / drawWalls / drawWindows
//! from public/main.js, halved to the 640x360 render frame.

use super::super::geometry::{
    DoorV, Rect, Spill, DOOR, LAB_DOOR, LAB_ROOM, MEETING_DOOR, MEETING_ROOM, OPEN_ROOM,
    WALL_THICKNESS, WINDOWS, WINDOW_H, WINDOW_W,
};
use super::super::palette::{self, Rgb};
use super::super::{blend, Framebuffer};
use super::{h, hr};

pub fn draw_floor(fb: &mut impl Framebuffer) {
    // OPEN_ROOM: 16-wide x 32-tall alternating bands (halved from 32x64 via
    // the JS tile=32 code, which stepped x by tile/2=16 and y by tile=32 and
    // toggled per (y/tile + x/(tile/2))). In render-frame pixels that becomes
    // 8-wide x 16-tall strips.
    let tile_w = 8; // tile/2/2 in render coords
    let tile_h = 16; // tile/2 in render coords

    let r = hr(OPEN_ROOM);
    let mut y = r.y;
    while y < r.y + r.h {
        let mut x = r.x;
        while x < r.x + r.w {
            let odd = ((y - r.y) / tile_h + (x - r.x) / tile_w) & 1;
            let c = if odd == 1 {
                palette::FLOOR_A
            } else {
                palette::FLOOR_B
            };
            fb.fill_rect(Rect::new(x, y, tile_w, tile_h), c);
            x += tile_w;
        }
        y += tile_h;
    }

    // Horizontal floor-line grid, faint. JS uses alpha 0.4 at full y-tile
    // intervals; we draw a single dark line at tile-height spacing. No blend.
    let mut gy = r.y + tile_h;
    while gy < r.y + r.h {
        fb.draw_hline(r.x, r.x + r.w - 1, gy, palette::FLOOR_LINE);
        gy += tile_h;
    }

    // MEETING_ROOM: cool carpet. Tile is 32x32 in JS → 16x16 in render.
    let room = hr(MEETING_ROOM);
    let mtile = 16;
    let mut y = room.y;
    while y < room.y + room.h {
        let mut x = room.x;
        while x < room.x + room.w {
            let odd = ((y - room.y) / mtile + (x - room.x) / mtile) & 1;
            let c = if odd == 1 {
                palette::FLOOR_MEETING_A
            } else {
                palette::FLOOR_MEETING_B
            };
            fb.fill_rect(Rect::new(x, y, mtile, mtile), c);
            x += mtile;
        }
        y += mtile;
    }

    // LAB_ROOM: clinical tile.
    let room = hr(LAB_ROOM);
    let mut y = room.y;
    while y < room.y + room.h {
        let mut x = room.x;
        while x < room.x + room.w {
            let odd = ((y - room.y) / mtile + (x - room.x) / mtile) & 1;
            let c = if odd == 1 {
                palette::FLOOR_LAB_A
            } else {
                palette::FLOOR_LAB_B
            };
            fb.fill_rect(Rect::new(x, y, mtile, mtile), c);
            x += mtile;
        }
        y += mtile;
    }

    // Lab grid lines.
    let mut gy = room.y + mtile;
    while gy < room.y + room.h {
        fb.draw_hline(room.x, room.x + room.w - 1, gy, palette::LAB_GRID);
        gy += mtile;
    }
    let mut gx = room.x + mtile;
    while gx < room.x + room.w {
        fb.draw_vline(gx, room.y, room.y + room.h - 1, palette::LAB_GRID);
        gx += mtile;
    }
}

pub fn draw_walls(fb: &mut impl Framebuffer) {
    let t = h(WALL_THICKNESS); // 3 in render coords
    for &room in &[OPEN_ROOM, MEETING_ROOM, LAB_ROOM] {
        draw_room_walls(fb, hr(room), t);
    }

    // Exterior west door of OPEN_ROOM — punch a gap in the west wall and
    // draw a faint accent on the outer edge.
    let r = hr(OPEN_ROOM);
    let door_y = h(DOOR.y);
    let door_h = h(DOOR.h);
    fb.fill_rect(Rect::new(r.x - t, door_y, t, door_h), palette::BG);

    // Interior doors to MEETING / LAB punch through the partition strip
    // between OPEN_ROOM and the right column.
    punch_inner_door(fb, MEETING_DOOR, t);
    punch_inner_door(fb, LAB_DOOR, t);
}

fn draw_room_walls(fb: &mut impl Framebuffer, r: Rect, t: i32) {
    // Fill the 4 wall rects. JS stretches each wall past the corner by `T`.
    fb.fill_rect(Rect::new(r.x - t, r.y - t, r.w + 2 * t, t), palette::WALL);
    fb.fill_rect(Rect::new(r.x - t, r.y + r.h, r.w + 2 * t, t), palette::WALL);
    fb.fill_rect(Rect::new(r.x - t, r.y - t, t, r.h + 2 * t), palette::WALL);
    fb.fill_rect(Rect::new(r.x + r.w, r.y - t, t, r.h + 2 * t), palette::WALL);

    // Highlight top-row + left-col (JS uses a 2-px band; in render coords
    // that's 1 px on the thin walls).
    fb.draw_hline(r.x - t, r.x + r.w + t - 1, r.y - t, palette::WALL_HI);
    fb.draw_vline(r.x - t, r.y - t, r.y + r.h + t - 1, palette::WALL_HI);
}

fn punch_inner_door(fb: &mut impl Framebuffer, door: DoorV, t: i32) {
    // Gap spans from just inside OPEN_ROOM's east wall to just inside the
    // right-column room's west wall; width mirrors the JS helper.
    let inner_gap_x = h(OPEN_ROOM.x + OPEN_ROOM.w) - t;
    let total_span = h(MEETING_ROOM.x) - h(OPEN_ROOM.x + OPEN_ROOM.w) + 2 * t;
    let dy = h(door.y - door.h / 2);
    let dh = h(door.h);
    fb.fill_rect(Rect::new(inner_gap_x, dy, total_span, dh), palette::BG);
}

pub fn draw_windows(fb: &mut impl Framebuffer, spill_alpha: f32) {
    // Two passes: first the light-spill trapezoids so walls and later furniture
    // paint over them. JS does spill *before* frame/glass — we do the same.
    // TODO(step 6): EMA-modulated spill alpha driven by recent-event rate
    //   (see WINDOW_SPILL_BASE_ALPHA / WINDOW_SPILL_PEAK_ALPHA in main.js).
    for w in &WINDOWS {
        draw_window_spill(fb, w, spill_alpha);
    }
    for w in &WINDOWS {
        draw_window_frame(fb, w);
    }
}

fn draw_window_spill(
    fb: &mut impl Framebuffer,
    w: &super::super::geometry::WindowRec,
    spill_alpha: f32,
) {
    // Trapezoid: narrow edge at the wall, wide edge 10 render-px into the
    // room (JS uses depth=20 pre-halve). Painted as blended horizontal lines.
    let edge_y = h(w.room_edge_y);
    let depth = h(20);
    let (near_y, far_y) = match w.spill {
        Spill::South => (edge_y, edge_y + depth),
        Spill::North => (edge_y, edge_y - depth),
    };
    let half_near = h(WINDOW_W) / 2;
    let half_far = h(WINDOW_W) / 2 + h(24); // JS adds 12 pre-halve to each side
    let cx = h(w.x);
    let (y0, y1) = if near_y <= far_y {
        (near_y, far_y)
    } else {
        (far_y, near_y)
    };
    let span = (y1 - y0).max(1);

    for y in y0..=y1 {
        // Linear interpolate half-width between near and far.
        let t = match w.spill {
            Spill::South => (y - near_y) as f32 / span as f32,
            Spill::North => (near_y - y) as f32 / span as f32,
        };
        let half = (half_near as f32 + (half_far - half_near) as f32 * t).round() as i32;
        let xa = cx - half;
        let xb = cx + half;
        for x in xa..=xb {
            fb.set_pixel(x, y, tinted_floor(x, y, spill_alpha));
        }
    }
}

/// Cheap approximation: pick the average floor tone for the room containing
/// `(x, y)` and blend the window-glass colour over it at the spill alpha.
/// Avoids a `get_pixel` trait method while preserving the "blue wash on the
/// floor near a window" look. Draw order puts spills before walls/furniture,
/// so nothing has been painted there yet anyway.
fn tinted_floor(x: i32, y: i32, spill_alpha: f32) -> Rgb {
    let base = pick_floor(x, y);
    blend(base, palette::WINDOW_GLASS, spill_alpha)
}

fn pick_floor(x: i32, y: i32) -> Rgb {
    let open = hr(OPEN_ROOM);
    let meeting = hr(MEETING_ROOM);
    let lab = hr(LAB_ROOM);
    if in_rect(x, y, meeting) {
        // Average the two meeting tones — the static spill can't know which
        // tile it covers without reading back, and the two tones are only
        // ~2% apart in luminance so the average is visually indistinguishable.
        return avg(palette::FLOOR_MEETING_A, palette::FLOOR_MEETING_B);
    }
    if in_rect(x, y, lab) {
        return avg(palette::FLOOR_LAB_A, palette::FLOOR_LAB_B);
    }
    if in_rect(x, y, open) {
        return avg(palette::FLOOR_A, palette::FLOOR_B);
    }
    palette::BG
}

fn in_rect(x: i32, y: i32, r: Rect) -> bool {
    x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
}

fn avg(a: Rgb, b: Rgb) -> Rgb {
    Rgb(
        ((a.0 as u16 + b.0 as u16) / 2) as u8,
        ((a.1 as u16 + b.1 as u16) / 2) as u8,
        ((a.2 as u16 + b.2 as u16) / 2) as u8,
    )
}

fn draw_window_frame(fb: &mut impl Framebuffer, w: &super::super::geometry::WindowRec) {
    let fw = h(WINDOW_W);
    let fh = h(WINDOW_H).max(2);
    let fx = h(w.x) - fw / 2;
    let fy = h(w.y) - fh / 2;

    fb.fill_rect(Rect::new(fx, fy, fw, fh), palette::WINDOW_FRAME);
    // Glass inset.
    fb.fill_rect(
        Rect::new(fx + 1, fy + 1, (fw - 2).max(1), (fh - 2).max(1)),
        palette::WINDOW_GLASS,
    );
    // Vertical mullion through middle.
    fb.draw_vline(fx + fw / 2, fy + 1, fy + fh - 2, palette::WINDOW_FRAME);
}
