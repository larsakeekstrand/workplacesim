//! Desks, meeting table, lab bench + stations. 1:1 port of drawDesk /
//! drawMeetingRoom / drawLabRoom / drawLabStation from public/main.js, halved
//! to the 640x360 render frame.

use super::super::geometry::{
    desk_seats, lab_stations, meeting_seats, DeskSeat, LabStation, MeetingSeat, MeetingSide, Rect,
    BENCH, DESK_H, DESK_W, LAB_ROOM, LAB_STATION_XS, MEETING_ROOM, TABLE,
};
use super::super::palette;
use super::super::Framebuffer;
use super::{h, hr};

pub fn draw_desks(fb: &mut impl Framebuffer) {
    for d in desk_seats() {
        draw_desk(fb, &d);
        draw_desk_chair(fb, &d);
    }
}

fn draw_desk(fb: &mut impl Framebuffer, d: &DeskSeat) {
    let x = h(d.x - DESK_W / 2);
    let y = h(d.y - DESK_H / 2);
    let w = h(DESK_W);
    let dh = h(DESK_H);

    // Desk body — two-tone slab for the bevelled edge JS draws with rounded
    // rectangles.
    fb.fill_rect(Rect::new(x, y + dh - 3, w, 3), palette::DESK_SHADE);
    fb.fill_rect(Rect::new(x, y, w, dh - 2), palette::DESK_TOP);
    fb.stroke_rect(Rect::new(x, y, w, dh - 2), palette::DESK_EDGE);

    // Monitor + glow. JS dimensions (34x16) halve to (17x8).
    let mw = h(34);
    let mh = h(16);
    let mx = h(d.x) - mw / 2;
    let my = y + 2;
    fb.fill_rect(Rect::new(mx - 1, my - 1, mw + 2, mh + 2), palette::WALL);
    fb.fill_rect(Rect::new(mx, my, mw, mh), palette::MONITOR);
    fb.fill_rect(
        Rect::new(mx + 1, my + 1, mw - 2, mh - 2),
        palette::MONITOR_GLOW,
    );
    // Monitor stand.
    fb.fill_rect(Rect::new(h(d.x) - 2, my + mh, 4, 2), palette::MOUSE);

    // Keyboard: JS 36x7 halved → 18x3.
    let kw = h(36);
    let kh = 3;
    let kx = h(d.x) - kw / 2;
    let ky = y + dh - 6;
    fb.fill_rect(Rect::new(kx, ky, kw, kh), palette::KEYBOARD);
}

fn draw_desk_chair(fb: &mut impl Framebuffer, d: &DeskSeat) {
    let cx = h(d.x);
    let cy = h(d.y + DESK_H / 2 + 16);
    draw_round_chair(fb, cx, cy, false);
}

/// Small round chair, optionally with a backrest above. `north_backrest`
/// puts the back north of the seat; meeting north-side chairs flip this.
fn draw_round_chair(fb: &mut impl Framebuffer, cx: i32, cy: i32, north_backrest: bool) {
    // Backrest plate.
    let bw = h(24);
    let bh = 2;
    let bx = cx - bw / 2;
    let by = if north_backrest { cy + 3 } else { cy - 5 };
    fb.fill_rect(Rect::new(bx, by, bw, bh), palette::CHAIR_HI);

    // Seat disk.
    let r = h(9);
    fb.fill_circle(cx, cy, r, palette::CHAIR);
    // Highlight pip at centre.
    fb.fill_circle(cx, cy, 1, palette::CHAIR_HI);
}

pub fn draw_meeting_room(fb: &mut impl Framebuffer) {
    // Whiteboard on north wall. JS: 200x22 centred 10px below top.
    let wb_w = h(200);
    let wb_h = h(22).max(6);
    let wb_x = h(MEETING_ROOM.x) + (h(MEETING_ROOM.w) - wb_w) / 2;
    let wb_y = h(MEETING_ROOM.y) + h(10);
    fb.fill_rect(
        Rect::new(wb_x - 1, wb_y - 1, wb_w + 2, wb_h + 2),
        palette::WHITEBOARD_FRAME,
    );
    fb.fill_rect(Rect::new(wb_x, wb_y, wb_w, wb_h), palette::WHITEBOARD_BODY);
    // TODO(step 6): diagram strokes + session_prompt text overlay.
    // Tray ledge beneath the whiteboard (JS draws marker-stripes; static panel here).
    fb.fill_rect(
        Rect::new(wb_x, wb_y + wb_h + 1, wb_w, 1),
        palette::WHITEBOARD_FRAME,
    );

    // Conference table. Smaller of the two JS slabs (deskShade under deskTop).
    let tx = h(TABLE.cx - TABLE.w / 2);
    let ty = h(TABLE.cy - TABLE.h / 2);
    let tw = h(TABLE.w);
    let th = h(TABLE.h);
    fb.fill_rect(Rect::new(tx, ty + th - 3, tw, 3), palette::DESK_SHADE);
    fb.fill_rect(Rect::new(tx, ty, tw, th - 2), palette::DESK_TOP);
    fb.stroke_rect(Rect::new(tx, ty, tw, th - 2), palette::DESK_EDGE);

    // Two laptop props on the table.
    fb.fill_rect(
        Rect::new(tx + h(12), ty + h(10), h(18), h(12).max(4)),
        palette::WHITEBOARD_BODY,
    );
    fb.fill_rect(
        Rect::new(tx + tw - h(30), ty + h(10), h(18), h(12).max(4)),
        palette::WHITEBOARD_BODY,
    );

    // Chairs.
    for seat in meeting_seats() {
        draw_meeting_chair(fb, &seat);
    }
}

fn draw_meeting_chair(fb: &mut impl Framebuffer, seat: &MeetingSeat) {
    let cx = h(seat.x);
    let cy = h(seat.y);
    let north_backrest = matches!(seat.side, MeetingSide::North);
    draw_round_chair(fb, cx, cy, north_backrest);
}

pub fn draw_lab_room(fb: &mut impl Framebuffer) {
    // Server rack SE corner.
    let rack_x = h(LAB_ROOM.x + LAB_ROOM.w - 30);
    let rack_y = h(LAB_ROOM.y + LAB_ROOM.h - 70);
    let rack_w = h(22);
    let rack_h = h(52);
    fb.fill_rect(
        Rect::new(rack_x, rack_y, rack_w, rack_h),
        palette::BUILD_BOARD_BG,
    );
    fb.stroke_rect(
        Rect::new(rack_x, rack_y, rack_w, rack_h),
        palette::BENCH_EDGE,
    );
    // Rack slots — JS draws 5 rows at 9-px spacing.
    for i in 0..5 {
        let ry = rack_y + h(4 + i * 9);
        fb.fill_rect(
            Rect::new(rack_x + 1, ry, rack_w - 2, h(6).max(2)),
            palette::WALL,
        );
        // Two LEDs per slot.
        fb.fill_rect(Rect::new(rack_x + 2, ry + 1, 1, 1), palette::LED);
        fb.fill_rect(Rect::new(rack_x + 4, ry + 1, 1, 1), palette::DESK_EDGE);
    }

    // Workbench on north interior wall.
    let b = hr(BENCH);
    fb.fill_rect(Rect::new(b.x, b.y + b.h - 3, b.w, 3), palette::BENCH_SHADE);
    fb.fill_rect(Rect::new(b.x, b.y, b.w, b.h - 2), palette::BENCH_TOP);
    fb.stroke_rect(Rect::new(b.x, b.y, b.w, b.h - 2), palette::BENCH_EDGE);

    // Three bench-top stations.
    for &cx in &LAB_STATION_XS {
        draw_lab_station(fb, cx, BENCH.y);
    }

    // Chairs south of the bench.
    for s in lab_stations() {
        draw_lab_chair(fb, &s);
    }
}

fn draw_lab_station(fb: &mut impl Framebuffer, cx_js: i32, by_js: i32) {
    let mw = h(26);
    let mh = h(14).max(4);
    let mx = h(cx_js) - mw / 2;
    let my = h(by_js) + 2;
    // Monitor body.
    fb.fill_rect(Rect::new(mx - 1, my - 1, mw + 2, mh + 2), palette::WALL);
    fb.fill_rect(Rect::new(mx, my, mw, mh), palette::MONITOR);
    // Green test-output fill.
    fb.fill_rect(Rect::new(mx + 1, my + 1, mw - 2, mh - 2), palette::SCOPE);
    // A few SCOPE_TRACE scanlines for the "terminal log" look.
    for off in [1i32, 3, 5] {
        let y = my + 1 + off;
        if y < my + mh - 1 {
            fb.draw_hline(mx + 1, mx + mw - 3, y, palette::SCOPE_TRACE);
        }
    }

    // Oscilloscope to the right.
    let ow = h(20);
    let oh = h(14).max(4);
    let ox = mx + mw + 3;
    let oy = my;
    fb.fill_rect(Rect::new(ox, oy, ow, oh), palette::SCOPE);
    fb.stroke_rect(Rect::new(ox, oy, ow, oh), palette::BENCH_EDGE);
    // Faint centre trace.
    let ty = oy + oh / 2;
    fb.draw_hline(ox + 1, ox + ow - 2, ty, palette::SCOPE_TRACE);
}

fn draw_lab_chair(fb: &mut impl Framebuffer, s: &LabStation) {
    let cx = h(s.x);
    let cy = h(s.y);
    draw_round_chair(fb, cx, cy, true);
}
