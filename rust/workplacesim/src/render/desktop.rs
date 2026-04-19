//! minifb-backed desktop renderer. macOS/Linux-friendly path for visual QA
//! and parity with the /dev/fb0 backend (`render::fb`).
//!
//! Must run on the main thread on macOS (AppKit window-management rule). The
//! axum server lives on a background tokio runtime; see `src/main.rs` for
//! the bootstrap.
//!
//! Task #5 added live window recreation: `Config::{window_w, window_h,
//! fullscreen}` are polled each frame and the minifb `Window` is dropped +
//! rebuilt when any of them changes. The sim/fx stores and the in-memory
//! `RenderFrame` survive recreation — only the OS surface and the u32 pixel
//! buffer (whose size tracks the window) are rebuilt. A fit-aware blit
//! replaces the old hardcoded 2× scaler, so windows of any size get an
//! aspect-preserving letterboxed image.
//!
//! Fullscreen semantics: minifb 0.27 has no dedicated fullscreen flag, so
//! `fullscreen=true` builds a borderless, topmost, non-resizable window
//! sized to the configured `window_w`/`window_h`. The user is responsible
//! for setting those to the panel's native size (or a size the compositor
//! can comfortably blow up); the renderer doesn't try to query the display.

use std::thread;
use std::time::{Duration, Instant};

use minifb::{Key, Scale, ScaleMode, Window, WindowOptions};
use tokio::sync::broadcast;

use super::fit::{compute_scale_fit, ScaleFit};
use super::fx_store::{FxLimits, FxStore};
use super::sim_store::SimStore;
use super::world::RenderWorld;
use super::{palette, scene, Framebuffer, RenderFrame, RENDER_H, RENDER_W};
use crate::config::{SharedConfig, DEFAULT_WINDOW_H, DEFAULT_WINDOW_W};
use crate::server::routes::FbInfo;
use crate::server::Shared;
use crate::state::{clock, Agent, Event};

/// Parameters that force a window rebuild when any differ from the active
/// window's parameters. `target_fps` is NOT part of this: it's live-applied
/// via `window.set_target_fps` every frame and doesn't require recreation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DisplaySnapshot {
    w: u32,
    h: u32,
    fullscreen: bool,
}

impl DisplaySnapshot {
    /// Read from the live config. Applies defensive defaults on nonsense
    /// dimensions so a bad config never crashes the renderer — the config
    /// module's `clamp()` already guards this, but a bypass (direct RwLock
    /// write in a test, future mutation path) stays safe.
    fn from_config(config: &SharedConfig) -> Self {
        let c = config.read();
        let w = if c.window_w == 0 {
            DEFAULT_WINDOW_W
        } else {
            c.window_w
        };
        let h = if c.window_h == 0 {
            DEFAULT_WINDOW_H
        } else {
            c.window_h
        };
        Self {
            w,
            h,
            fullscreen: c.fullscreen,
        }
    }
}

/// Build a minifb window for the given snapshot. Fullscreen mode here is
/// borderless, topmost, non-resizable at the configured size (minifb 0.27 has
/// no native fullscreen flag). On failure returns an error so the caller can
/// decide whether to fall back or bail.
fn create_window(snap: DisplaySnapshot) -> Result<Window, minifb::Error> {
    let opts = if snap.fullscreen {
        WindowOptions {
            borderless: true,
            title: false,
            resize: false,
            scale: Scale::X1,
            scale_mode: ScaleMode::Stretch,
            topmost: true,
            // `none` disables decorations on Windows; harmless on macOS/Linux
            // where `borderless` already does the job.
            none: true,
            transparency: false,
        }
    } else {
        WindowOptions::default()
    };
    Window::new("workplacesim", snap.w as usize, snap.h as usize, opts)
}

/// Try `snap`, then fall back to `DEFAULT_*`-sized non-fullscreen window.
/// Returns the successful snapshot alongside the window so the caller can
/// cache the live parameters even when it had to fall back.
fn create_window_with_fallback(snap: DisplaySnapshot) -> anyhow::Result<(Window, DisplaySnapshot)> {
    match create_window(snap) {
        Ok(w) => Ok((w, snap)),
        Err(e) => {
            tracing::warn!(
                "minifb window init failed for {}x{} fullscreen={}: {e}; \
                 falling back to {}x{} windowed",
                snap.w,
                snap.h,
                snap.fullscreen,
                DEFAULT_WINDOW_W,
                DEFAULT_WINDOW_H
            );
            let fallback = DisplaySnapshot {
                w: DEFAULT_WINDOW_W,
                h: DEFAULT_WINDOW_H,
                fullscreen: false,
            };
            match create_window(fallback) {
                Ok(w) => Ok((w, fallback)),
                Err(e2) => Err(anyhow::anyhow!(
                    "minifb window init failed even at defaults {}x{}: {e2}",
                    DEFAULT_WINDOW_W,
                    DEFAULT_WINDOW_H
                )),
            }
        }
    }
}

/// Populate `fb_info` with the current window + fit metrics. Desktop always
/// reports bpp=32 because minifb's `update_with_buffer` consumes an
/// XRGB-packed u32 buffer regardless of host.
fn publish_fb_info(
    fb_info: &std::sync::Arc<parking_lot::RwLock<Option<FbInfo>>>,
    snap: DisplaySnapshot,
    fit: &ScaleFit,
) {
    *fb_info.write() = Some(FbInfo {
        panel_w: snap.w,
        panel_h: snap.h,
        bpp: 32,
        scaled_w: fit.letterbox.visible_w.max(0) as u32,
        scaled_h: fit.letterbox.visible_h.max(0) as u32,
        letterbox_x: fit.letterbox.dst_x.max(0) as u32,
        letterbox_y: fit.letterbox.dst_y.max(0) as u32,
    });
}

/// Nearest-neighbour scale + letterbox blit into a minifb `u32` buffer.
/// `dst` is `dst_w * dst_h` u32s where each pixel is packed as
/// `0xff_RR_GG_BB` (minifb's expected format with opaque alpha). Pixels
/// outside the letterbox rect are filled with 0 (black). `fit` carries the
/// precomputed source-col / source-row lookup tables so the hot loop stays
/// divide-free.
pub fn scaled_blit_u32(src: &RenderFrame, dst: &mut [u32], dst_w: u32, fit: &ScaleFit) {
    let dst_w = dst_w as usize;
    let sw = src.width() as usize;
    let bytes = src.rgb_bytes();
    let lb = fit.letterbox;
    let vis_w = lb.visible_w.max(0) as usize;
    let vis_h = lb.visible_h.max(0) as usize;
    let dst_x = lb.dst_x.max(0) as usize;
    let dst_y = lb.dst_y.max(0) as usize;

    // Clear the full dst to black first. Cheap relative to the blit itself
    // and correct in corner cases where the fit is smaller than the buffer
    // (any non-16:9 target, or a window resize before the fit updates).
    for p in dst.iter_mut() {
        *p = 0;
    }

    for dy in 0..vis_h {
        let sy = fit.row_src[dy] as usize;
        let row_dst = (dst_y + dy) * dst_w + dst_x;
        let src_row_base = sy * sw * 3;
        for dx in 0..vis_w {
            let sx = fit.col_src[dx] as usize;
            let si = src_row_base + sx * 3;
            let pixel = 0xff00_0000
                | ((bytes[si] as u32) << 16)
                | ((bytes[si + 1] as u32) << 8)
                | (bytes[si + 2] as u32);
            dst[row_dst + dx] = pixel;
        }
    }
}

/// Run the desktop render loop. Backwards-compatible signature — callers
/// that don't have an `fb_info` handle (older tests, the dump_* binaries)
/// pass nothing and the per-frame metrics publication is a no-op.
pub fn run_desktop(
    state: Shared,
    config: SharedConfig,
    rx: broadcast::Receiver<Event>,
) -> anyhow::Result<()> {
    run_desktop_with_fb_info(state, config, rx, None)
}

/// Desktop render loop with fb_info plumbing. `fb_info` (if `Some`) is
/// populated with live window + fit metrics on each window create/recreate so
/// `/api/status` can report them.
pub fn run_desktop_with_fb_info(
    state: Shared,
    config: SharedConfig,
    mut rx: broadcast::Receiver<Event>,
    fb_info: Option<std::sync::Arc<parking_lot::RwLock<Option<FbInfo>>>>,
) -> anyhow::Result<()> {
    // Initial window: read config once, try to honor it, fall back to
    // defaults on minifb init failure, bail if even defaults fail.
    let mut snap = DisplaySnapshot::from_config(&config);
    let (mut window, active_snap) = create_window_with_fallback(snap)?;
    snap = active_snap;

    // Read the initial target FPS under a brief lock. `set_target_fps` is
    // live-applied below when the value changes between frames and on every
    // window recreation.
    let mut target_fps: u64 = config.read().target_fps as u64;
    window.set_target_fps(target_fps.max(1) as usize);

    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
    let mut pixels: Vec<u32> = vec![0; (snap.w as usize) * (snap.h as usize)];
    let mut fit = compute_scale_fit(
        RENDER_W as i32,
        RENDER_H as i32,
        snap.w as i32,
        snap.h as i32,
    );

    if let Some(ref h) = fb_info {
        publish_fb_info(h, snap, &fit);
    }

    let mut store = SimStore::new();
    let mut fx = FxStore::new();
    let mut last_tick = Instant::now();
    let started_at_ms = clock::now_ms();
    let mut idle_since_ms: Option<u64> = None;
    let mut last_session_ended_ms: Option<u64> = None;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let t0 = Instant::now();
        let now_ms = clock::now_ms();

        // Single short read-lock snapshot so no downstream helper touches the
        // RwLock directly. `FxLimits` gathers every ambient-FX knob into one
        // Copy struct; motion + error-glyph live alongside as simple scalars.
        let (
            fx_limits,
            walk_speed_px_per_sec,
            bob_cycle_ms,
            error_glyph_ms,
            window_spill_alpha,
            frame_target_fps,
        ) = {
            let c = config.read();
            (
                FxLimits::from_config(&c),
                c.walk_speed_px_per_sec,
                c.bob_cycle_ms,
                c.error_glyph_ms,
                c.window_spill_alpha,
                c.target_fps as u64,
            )
        };
        let frame_budget = Duration::from_millis(1000 / frame_target_fps.max(1));

        // Live window-recreation: if window_w/window_h/fullscreen changed,
        // rebuild the window and the pixel buffer and recompute the fit.
        // SimStore/FxStore/RenderFrame are intentionally NOT recreated so
        // sim motion continues across the one-frame init hiccup.
        let desired = DisplaySnapshot::from_config(&config);
        if desired != snap {
            // Drop-and-rebuild. `window` going out of scope via shadowing
            // closes the previous OS surface before the new one is created;
            // on macOS/Linux this is fine for a brief interval. If creation
            // fails, keep the old window/pixels/fit alive and try again
            // next frame after the config is corrected.
            match create_window(desired) {
                Ok(mut new_window) => {
                    new_window.set_target_fps(frame_target_fps.max(1) as usize);
                    window = new_window;
                    snap = desired;
                    pixels = vec![0; (snap.w as usize) * (snap.h as usize)];
                    fit = compute_scale_fit(
                        RENDER_W as i32,
                        RENDER_H as i32,
                        snap.w as i32,
                        snap.h as i32,
                    );
                    // Keep target_fps in sync — the `set_target_fps` above
                    // applied to the fresh window, so update the cache too.
                    target_fps = frame_target_fps;
                    if let Some(ref h) = fb_info {
                        publish_fb_info(h, snap, &fit);
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "minifb recreate to {}x{} fullscreen={} failed: {e}; keeping previous window",
                        desired.w,
                        desired.h,
                        desired.fullscreen
                    );
                    // Don't touch `snap` — keep the last successful config
                    // so we don't spam this warning every frame. The user
                    // must POST a different value to retry.
                    snap = desired;
                }
            }
        }

        // Live-apply a target_fps change. Cheap call on minifb; no visual
        // hitch. Already re-applied when a window is rebuilt (above).
        if frame_target_fps != target_fps {
            window.set_target_fps(frame_target_fps.max(1) as usize);
            target_fps = frame_target_fps;
        }

        // Snapshot under a brief read lock, drop immediately. list_active is
        // the set used to gate the idle-since/status readout: we purposely
        // exclude finished (walk-out) records so the readout reappears the
        // moment Claude restarts, without waiting for the stop grace window.
        let (world, agents) = {
            let s = state.read();
            (RenderWorld::from_state(&s, now_ms), s.list_active())
        };
        // Track the last session end for the readout's "last HH:MM:SS" line.
        for a in &agents {
            if a.agent_type == "claude" {
                if let Some(fa) = a.finished_at {
                    last_session_ended_ms = Some(last_session_ended_ms.unwrap_or(0).max(fa));
                }
            }
        }
        if agents.is_empty() {
            idle_since_ms.get_or_insert(now_ms);
        } else {
            idle_since_ms = None;
        }

        // Advance time-based state transitions (visit expiry, stop grace, etc).
        {
            let mut s = state.write();
            s.tick(now_ms);
        }

        store.reconcile(&world);
        let dt_ms = last_tick.elapsed().as_millis() as u64;
        last_tick = t0;
        store.tick(dt_ms, now_ms, walk_speed_px_per_sec, bob_cycle_ms);
        // Drain after sim_store knows about the new sims so motes can resolve
        // their anchor positions and tethers can verify parents exist.
        fx.drain_events(&mut rx, &store, now_ms, &fx_limits);
        fx.tick(now_ms, &mut store, &fx_limits);

        frame.clear(palette::BG);
        scene::draw_static_background(&mut frame, window_spill_alpha);
        scene::effects::draw_below(&mut frame, &fx, &store, now_ms, &fx_limits);
        scene::sim::draw_sims(&mut frame, &store);
        let agent_refs: Vec<&Agent> = agents.iter().collect();
        scene::glyph::draw_glyphs(&mut frame, &store, &agent_refs, &fx, now_ms, error_glyph_ms);
        scene::effects::draw_above(&mut frame, &fx, &store, now_ms, &fx_limits);
        scene::text::draw_whiteboard(&mut frame, &store, &agent_refs);
        scene::text::draw_file_ticker(&mut frame, &fx, now_ms, &fx_limits);
        scene::text::draw_bench_flashes(&mut frame, &fx, now_ms, &fx_limits);
        if let Some(since) = idle_since_ms {
            if now_ms.saturating_sub(since) > 2_000 {
                scene::text::draw_status_readout(
                    &mut frame,
                    now_ms,
                    started_at_ms,
                    last_session_ended_ms,
                );
            }
        }

        scaled_blit_u32(&frame, &mut pixels, snap.w, &fit);
        window
            .update_with_buffer(&pixels, snap.w as usize, snap.h as usize)
            .map_err(|e| anyhow::anyhow!("minifb update: {e}"))?;
        let elapsed = t0.elapsed();
        if elapsed < frame_budget {
            thread::sleep(frame_budget - elapsed);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::nearest_neighbour_blit_2x;

    /// `scaled_blit_u32` at exact 2× must match the existing
    /// `nearest_neighbour_blit_2x` path pixel-for-pixel. Guards against
    /// accidental drift when the hot loop is touched later.
    #[test]
    fn scaled_blit_u32_matches_2x_reference() {
        let mut src = RenderFrame::new(4, 3);
        // Deterministic checkerboard-ish pattern that exercises every channel.
        for y in 0..3 {
            for x in 0..4 {
                let r = (x as u8) * 64;
                let g = (y as u8) * 80;
                let b = ((x + y) as u8) * 32;
                src.set_pixel(x, y, super::super::palette::Rgb(r, g, b));
            }
        }
        let dst_w = 8;
        let dst_h = 6;

        let mut reference = vec![0u32; dst_w * dst_h];
        nearest_neighbour_blit_2x(&src, &mut reference);

        let mut actual = vec![0u32; dst_w * dst_h];
        let fit = compute_scale_fit(4, 3, dst_w as i32, dst_h as i32);
        scaled_blit_u32(&src, &mut actual, dst_w as u32, &fit);

        assert_eq!(
            actual, reference,
            "scaled_blit_u32 must agree with nearest_neighbour_blit_2x on exact 2× scale"
        );
    }

    #[test]
    fn scaled_blit_u32_letterboxes_mismatched_aspect() {
        // 2×1 source into 4×4 dst: scaled 4×2, centred at dst_y=1. Rows 0
        // and 3 must be black (zero), rows 1–2 hold the content doubled.
        let mut src = RenderFrame::new(2, 1);
        src.set_pixel(0, 0, super::super::palette::Rgb(0xff, 0, 0));
        src.set_pixel(1, 0, super::super::palette::Rgb(0, 0, 0xff));
        let dst_w = 4;
        let dst_h = 4;
        let mut dst = vec![0u32; dst_w * dst_h];
        let fit = compute_scale_fit(2, 1, dst_w as i32, dst_h as i32);
        scaled_blit_u32(&src, &mut dst, dst_w as u32, &fit);

        let red = 0xffff_0000u32;
        let blue = 0xff00_00ffu32;
        // Row 0 letterbox.
        assert_eq!(&dst[0..4], &[0, 0, 0, 0]);
        // Rows 1 and 2.
        for row in [1usize, 2] {
            let off = row * dst_w;
            assert_eq!(dst[off], red);
            assert_eq!(dst[off + 1], red);
            assert_eq!(dst[off + 2], blue);
            assert_eq!(dst[off + 3], blue);
        }
        // Row 3 letterbox.
        assert_eq!(&dst[3 * dst_w..4 * dst_w], &[0, 0, 0, 0]);
    }

    #[test]
    fn scaled_blit_u32_clears_previous_contents() {
        // The blit must repaint non-letterbox pixels and black-out the rest
        // even if the dst buffer was dirty. This is the guard that makes
        // window recreation visually clean.
        let mut src = RenderFrame::new(2, 2);
        src.set_pixel(0, 0, super::super::palette::Rgb(0xff, 0xff, 0xff));
        src.set_pixel(1, 0, super::super::palette::Rgb(0xff, 0xff, 0xff));
        src.set_pixel(0, 1, super::super::palette::Rgb(0xff, 0xff, 0xff));
        src.set_pixel(1, 1, super::super::palette::Rgb(0xff, 0xff, 0xff));
        let dst_w = 2;
        let dst_h = 2;
        let mut dst = vec![0xdead_beefu32; dst_w * dst_h];
        let fit = compute_scale_fit(2, 2, dst_w as i32, dst_h as i32);
        scaled_blit_u32(&src, &mut dst, dst_w as u32, &fit);

        let white = 0xffff_ffffu32;
        assert!(
            dst.iter().all(|&p| p == white),
            "dirty bits survived: {dst:?}"
        );
    }
}
