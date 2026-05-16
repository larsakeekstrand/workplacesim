//! Per-sim body-part drawing. Ports `makeSim` from `public/main.js` onto the
//! procedural pixel-art path — shadow / legs / body / head / hair / badge
//! rendered as primitives, no sprite textures.
//!
//! Paint order (back to front): shadow → legs → body → head → hair → badge.
//! Y-sort across sims so a closer body occludes a farther one.

use super::super::geometry::Rect;
use super::super::palette::{self, Rgb};
use super::super::sim_store::{SimAnim, SimState, SimStore};
use super::super::Framebuffer;
use super::h;

/// JS `sprite.setScale(1.8)` against 14x28 parts. We render at 640x360 (half
/// JS world), and in practice a scale of 1.8 at render resolution reads much
/// better on a TV-sized display — each sim occupies ~11x22 render pixels,
/// doubling on the fb after upscale to roughly 22x44 TV pixels.
const SIM_SCALE: f32 = 1.8;

/// Paint every alive sim. Y-sorted so north bodies paint before south ones.
pub fn draw_sims<F: Framebuffer>(fb: &mut F, store: &SimStore) {
    let mut sims: Vec<&SimAnim> = store.iter().filter(|s| s.is_alive()).collect();
    sims.sort_by(|a, b| {
        (a.y as i32)
            .cmp(&(b.y as i32))
            .then_with(|| a.agent_id.cmp(&b.agent_id))
    });
    for sim in sims {
        draw_sim(fb, sim);
    }
}

fn draw_sim<F: Framebuffer>(fb: &mut F, sim: &SimAnim) {
    // Convert JS world coords → render coords, then apply the seated bob.
    let cx = h(sim.x as i32);
    let bob_py = if matches!(sim.state, SimState::Seated) {
        // JS bob tween moves the sprite y -2 over 900 ms yoyo. We store phase
        // in radians; amplitude is 1 render px (2 world px / 2).
        -((sim.bob_phase.sin().abs() * 1.0).round() as i32)
    } else {
        0
    };
    let cy = h(sim.y as i32) + bob_py;

    let colors = palette::sim_colors(&sim.user);

    // Sizes, in render px. JS parts at scale 1.8 were 14x16 body / 10x10 head /
    // 10x6 hair / 12x5 legs / 24x8 shadow. Halved + SIM_SCALE-tuned to fit the
    // 640x360 frame while remaining legible. All literal sizes go through
    // scaled() so the proportions survive any future SIM_SCALE tweak.
    let body_w = scaled(6);
    let body_h = scaled(7);
    let head_r = scaled(2);
    let hair_h = scaled(2);
    let hair_w = scaled(5);
    let leg_w = scaled(1);
    let leg_h = scaled(3);
    let leg_gap = scaled(1);
    let shadow_w = scaled(8);
    let shadow_h = scaled(1);

    // Shadow — widest rect, sits under the feet.
    let shadow_y = cy + scaled(6);
    fb.fill_rect(
        Rect::new(cx - shadow_w / 2, shadow_y, shadow_w, shadow_h),
        blend_toward_bg(palette::BG, Rgb(0, 0, 0), 0.45),
    );

    // Legs — two small rects side-by-side. Walking swings them; seated is flat.
    // Top below cy by ~scaled(4) so ~3 render-px of leg sticks out below the
    // body bottom (visible legs read at ~13% of sim height, matching Node).
    let (left_leg_h, right_leg_h) = leg_lengths(sim, leg_h);
    let legs_top_y = cy + scaled(4);
    fb.fill_rect(
        Rect::new(
            cx - leg_w - leg_gap / 2 - leg_w + 1,
            legs_top_y,
            leg_w,
            left_leg_h,
        ),
        colors.pants,
    );
    fb.fill_rect(
        Rect::new(cx + leg_gap / 2, legs_top_y, leg_w, right_leg_h),
        colors.pants,
    );

    // Body — torso rect in shirt colour.
    let body_y = cy - body_h / 2 + scaled(1);
    fb.fill_rect(
        Rect::new(cx - body_w / 2, body_y, body_w, body_h),
        colors.shirt,
    );

    // Head — round-ish square in skin colour.
    let head_cx = cx;
    let head_cy = body_y - head_r - 1;
    fb.fill_circle(head_cx, head_cy, head_r, colors.skin);

    // Hair — thin strip on top of the head.
    let hair_y = head_cy - head_r;
    fb.fill_rect(
        Rect::new(cx - hair_w / 2, hair_y, hair_w, hair_h),
        colors.hair,
    );

    // Badge — small coloured pip on the chest for plan-mode or lab-keyword.
    let badge = badge_color(sim);
    if let Some(c) = badge {
        // Upper-left of body.
        fb.fill_rect(Rect::new(cx - body_w / 2 + 1, body_y + 1, 1, 1), c);
    }

    // Chest label — one legible char per Claude session, painted with a 1-px
    // black contour so it reads against any shirt color.
    draw_session_label(fb, sim, cx, body_y, body_h);
}

// 3x5 pixel font, just the legible-pool chars from `state::SESSION_LABEL_POOL`.
// `.` is bg, `#` is fg. Drawn once in black at the four ±1 neighbours so the
// outline survives shirt colour, then once in white on top.
fn label_glyph(c: char) -> Option<&'static [&'static str; 5]> {
    Some(match c {
        '1' => &[".#.", "##.", ".#.", ".#.", "###"],
        '2' => &["##.", "..#", ".#.", "#..", "###"],
        '3' => &["##.", "..#", ".#.", "..#", "##."],
        '4' => &["#.#", "#.#", "###", "..#", "..#"],
        '5' => &["###", "#..", "##.", "..#", "##."],
        '6' => &[".##", "#..", "###", "#.#", "###"],
        '7' => &["###", "..#", ".#.", ".#.", ".#."],
        '8' => &["###", "#.#", "###", "#.#", "###"],
        '9' => &["###", "#.#", "###", "..#", "##."],
        'A' => &[".#.", "#.#", "###", "#.#", "#.#"],
        'B' => &["##.", "#.#", "##.", "#.#", "##."],
        'C' => &[".##", "#..", "#..", "#..", ".##"],
        'D' => &["##.", "#.#", "#.#", "#.#", "##."],
        'E' => &["###", "#..", "##.", "#..", "###"],
        'F' => &["###", "#..", "##.", "#..", "#.."],
        'G' => &[".##", "#..", "#.#", "#.#", ".##"],
        'H' => &["#.#", "#.#", "###", "#.#", "#.#"],
        'J' => &["..#", "..#", "..#", "#.#", ".#."],
        'K' => &["#.#", "#.#", "##.", "#.#", "#.#"],
        'L' => &["#..", "#..", "#..", "#..", "###"],
        'M' => &["#.#", "###", "###", "#.#", "#.#"],
        'N' => &["#.#", "###", "###", "###", "#.#"],
        'P' => &["##.", "#.#", "##.", "#..", "#.."],
        'Q' => &[".#.", "#.#", "#.#", "###", ".##"],
        'R' => &["##.", "#.#", "##.", "#.#", "#.#"],
        'S' => &[".##", "#..", ".#.", "..#", "##."],
        'T' => &["###", ".#.", ".#.", ".#.", ".#."],
        'U' => &["#.#", "#.#", "#.#", "#.#", ".#."],
        'V' => &["#.#", "#.#", "#.#", "#.#", ".#."],
        'W' => &["#.#", "#.#", "###", "###", "#.#"],
        'X' => &["#.#", "#.#", ".#.", "#.#", "#.#"],
        'Y' => &["#.#", "#.#", ".#.", ".#.", ".#."],
        'Z' => &["###", "..#", ".#.", "#..", "###"],
        _ => return None,
    })
}

fn draw_session_label<F: Framebuffer>(
    fb: &mut F,
    sim: &SimAnim,
    cx: i32,
    body_y: i32,
    body_h: i32,
) {
    let Some(label) = sim.session_label.as_deref() else {
        return;
    };
    let Some(ch) = label.chars().next() else {
        return;
    };
    let Some(glyph) = label_glyph(ch) else {
        return;
    };
    // Glyph envelope is 3x5; with the 1-px cardinal outline the painted
    // footprint is 5x7. Center inside the 11x13 body so the outline never
    // overruns it.
    let glyph_w = 3;
    let glyph_h = 5;
    let gx = cx - glyph_w / 2;
    let gy = body_y + (body_h - glyph_h) / 2;
    let outline = Rgb(0, 0, 0);
    let fill = Rgb(0xff, 0xff, 0xff);
    // Outline pass: paint each fg cell at its four cardinal neighbours so
    // every white pixel is bordered. Diagonals stay clear, which keeps the
    // glyph crisp rather than blobby.
    for (row, line) in glyph.iter().enumerate() {
        for (col, c) in line.chars().enumerate() {
            if c != '#' {
                continue;
            }
            let px = gx + col as i32;
            let py = gy + row as i32;
            fb.set_pixel(px - 1, py, outline);
            fb.set_pixel(px + 1, py, outline);
            fb.set_pixel(px, py - 1, outline);
            fb.set_pixel(px, py + 1, outline);
        }
    }
    for (row, line) in glyph.iter().enumerate() {
        for (col, c) in line.chars().enumerate() {
            if c != '#' {
                continue;
            }
            fb.set_pixel(gx + col as i32, gy + row as i32, fill);
        }
    }
}

fn scaled(v: i32) -> i32 {
    ((v as f32) * SIM_SCALE).round().max(1.0) as i32
}

/// Walking: legs swap lengths on a 400 ms cycle, 180° out of phase. Seated:
/// both legs rest at full length.
fn leg_lengths(sim: &SimAnim, full: i32) -> (i32, i32) {
    if !matches!(sim.state, SimState::WalkingIn | SimState::WalkingOut) {
        return (full, full);
    }
    // Use the bob_phase only for seated; for walking we derive a local phase
    // from spawned_at_ms + a hash of position. Simpler: use x position as the
    // oscillator — every ~WALK_SPEED_PX_PER_SEC * 0.4 = 22 render-px, one
    // swing. That's approx the natural step length.
    let phase = (sim.x * 0.15).sin();
    let swing = (phase * (full as f32 * 0.5)).round() as i32;
    let l = full - swing.max(0);
    let r = full - (-swing).max(0);
    (l.max(1), r.max(1))
}

fn badge_color(sim: &SimAnim) -> Option<Rgb> {
    if sim.is_lab {
        return Some(Rgb(0x7f, 0xff, 0xb5));
    }
    if sim.permission_mode == "plan" {
        return Some(Rgb(0x7f, 0xc7, 0xff));
    }
    None
}

fn blend_toward_bg(base: Rgb, over: Rgb, alpha: f32) -> Rgb {
    super::super::blend(base, over, alpha)
}
