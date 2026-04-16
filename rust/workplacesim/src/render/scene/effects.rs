//! Ambient FX rendering — footstep trails, motes, parent→child tethers, error
//! halos. Reads `FxStore` ring buffers; writes alpha-blended pixels into the
//! framebuffer. Coordinates incoming from `FxStore` are JS world px (matching
//! the `SimAnim` contract); we halve at draw time the same way `scene::sim`
//! does.

use super::super::fx_store::{
    FxStore, FOOTSTEP_LIFETIME_MS, HALO_LIFETIME_MS, MOTE_LIFETIME_MS, TETHER_LIFETIME_MS,
};
use super::super::geometry::Point;
use super::super::palette::Rgb;
use super::super::sim_store::SimStore;
use super::super::{blend, Framebuffer, RenderFrame};
use super::h;

const HALO_COLOR: Rgb = Rgb(0xff, 0x64, 0x64);

/// Painted under sims so a body occludes the trail beneath its feet, matching
/// the JS `effects` Graphics depth (it sits below sim sprites). Tethers go
/// here too so a parent body draws over the line stub at its anchor.
pub fn draw_below(fb: &mut RenderFrame, fx: &FxStore, sim_store: &SimStore, now_ms: u64) {
    draw_footsteps(fb, fx, now_ms);
    draw_tethers(fb, fx, sim_store, now_ms);
}

/// Painted on top of sims — motes float above heads, halos ring the body.
/// JS uses Phaser depth ordering for the same effect.
pub fn draw_above(fb: &mut RenderFrame, fx: &FxStore, sim_store: &SimStore, now_ms: u64) {
    draw_motes(fb, fx, now_ms);
    draw_halos(fb, fx, sim_store, now_ms);
}

fn draw_footsteps(fb: &mut RenderFrame, fx: &FxStore, now_ms: u64) {
    for f in &fx.footsteps {
        let age = now_ms.saturating_sub(f.born_ms) as f32;
        let t = (age / FOOTSTEP_LIFETIME_MS as f32).clamp(0.0, 1.0);
        // JS uses 0.25 * (1 - t); we keep the same envelope.
        let alpha = 0.25 * (1.0 - t);
        if alpha <= 0.0 {
            continue;
        }
        // 2x2 dot in render coords.
        let cx = h(f.x as i32);
        let cy = h(f.y as i32);
        for dy in 0..2 {
            for dx in 0..2 {
                blend_pixel(fb, cx + dx - 1, cy + dy - 1, f.color, alpha);
            }
        }
    }
}

fn draw_motes(fb: &mut RenderFrame, fx: &FxStore, now_ms: u64) {
    for m in &fx.motes {
        let age = now_ms.saturating_sub(m.born_ms) as f32;
        let t = (age / MOTE_LIFETIME_MS as f32).clamp(0.0, 1.0);
        // JS tweens y by -24 over MOTE_LIFETIME_MS; halved for render.
        let lift = (24.0 * t).round() as i32;
        let cx = h(m.x as i32);
        let cy = h(m.y as i32) - h(lift);
        // Alpha 0.9 → 0 linearly.
        let alpha = 0.9 * (1.0 - t);
        if alpha <= 0.0 {
            continue;
        }
        for dy in 0..2 {
            for dx in 0..2 {
                blend_pixel(fb, cx + dx - 1, cy + dy - 1, m.color, alpha);
            }
        }
    }
}

fn draw_tethers(fb: &mut RenderFrame, fx: &FxStore, sim_store: &SimStore, now_ms: u64) {
    for t in &fx.tethers {
        let age = now_ms.saturating_sub(t.born_ms) as f32;
        let k = (age / TETHER_LIFETIME_MS as f32).clamp(0.0, 1.0);
        let alpha = 0.4 * (1.0 - k);
        if alpha <= 0.0 {
            continue;
        }
        let Some(parent) = sim_store.anim.get(&t.parent) else {
            continue;
        };
        let Some(child) = sim_store.anim.get(&t.child) else {
            continue;
        };
        let color = super::super::palette::sim_colors(&child.user).shirt;
        let p0 = Point::new(h(parent.x as i32), h(parent.y as i32));
        let p1 = Point::new(h(child.x as i32), h(child.y as i32));
        // Dashed line — walk the segment in (dash + gap) chunks of render px.
        let dash = 3.0;
        let gap = 2.0;
        let dx = (p1.x - p0.x) as f32;
        let dy = (p1.y - p0.y) as f32;
        let len = (dx * dx + dy * dy).sqrt().max(1.0);
        let nx = dx / len;
        let ny = dy / len;
        let mut d = 0.0;
        while d < len {
            let x0 = p0.x as f32 + nx * d;
            let y0 = p0.y as f32 + ny * d;
            let end_d = (d + dash).min(len);
            let x1 = p0.x as f32 + nx * end_d;
            let y1 = p0.y as f32 + ny * end_d;
            blend_line(fb, x0 as i32, y0 as i32, x1 as i32, y1 as i32, color, alpha);
            d += dash + gap;
        }
    }
}

fn draw_halos(fb: &mut RenderFrame, fx: &FxStore, sim_store: &SimStore, now_ms: u64) {
    for h_entry in &fx.halos {
        let age = now_ms.saturating_sub(h_entry.born_ms) as f32;
        let t = (age / HALO_LIFETIME_MS as f32).clamp(0.0, 1.0);
        let alpha = 0.6 * (1.0 - t);
        if alpha <= 0.0 {
            continue;
        }
        let Some(sim) = sim_store.anim.get(&h_entry.agent_id) else {
            continue;
        };
        // Spec: radius 3 + 8 * t (render px). Parity note: JS draws a fixed
        // radius-18 ring; the expanding form here is the spec's call.
        let radius = (3.0 + 8.0 * t).round() as i32;
        let cx = h(sim.x as i32);
        let cy = h(sim.y as i32);
        stroke_circle(fb, cx, cy, radius, HALO_COLOR, alpha);
    }
}

fn blend_pixel(fb: &mut RenderFrame, x: i32, y: i32, c: Rgb, alpha: f32) {
    if x < 0 || y < 0 || (x as u32) >= fb.width() || (y as u32) >= fb.height() {
        return;
    }
    let base = fb.get_pixel(x, y);
    fb.set_pixel(x, y, blend(base, c, alpha));
}

fn blend_line(fb: &mut RenderFrame, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgb, alpha: f32) {
    // Bresenham with per-pixel blend.
    let mut x = x0;
    let mut y = y0;
    let dx = (x1 - x).abs();
    let sx = if x < x1 { 1 } else { -1 };
    let dy = -(y1 - y).abs();
    let sy = if y < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        blend_pixel(fb, x, y, c, alpha);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Midpoint circle, blended. Skips degenerate radii.
fn stroke_circle(fb: &mut RenderFrame, cx: i32, cy: i32, r: i32, c: Rgb, alpha: f32) {
    if r <= 0 {
        blend_pixel(fb, cx, cy, c, alpha);
        return;
    }
    let mut x = r;
    let mut y = 0;
    let mut err = 1 - r;
    while x >= y {
        for (px, py) in [
            (cx + x, cy + y),
            (cx + y, cy + x),
            (cx - y, cy + x),
            (cx - x, cy + y),
            (cx - x, cy - y),
            (cx - y, cy - x),
            (cx + y, cy - x),
            (cx + x, cy - y),
        ] {
            blend_pixel(fb, px, py, c, alpha);
        }
        y += 1;
        if err < 0 {
            err += 2 * y + 1;
        } else {
            x -= 1;
            err += 2 * (y - x) + 1;
        }
    }
}

