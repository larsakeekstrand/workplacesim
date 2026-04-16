//! minifb-backed desktop renderer. macOS/Linux-friendly path for visual QA
//! while the Pi /dev/fb0 backend (step 5) is still offline.
//!
//! Must run on the main thread on macOS (AppKit window-management rule). The
//! axum server lives on a background tokio runtime; see `src/main.rs` for
//! the bootstrap.

use std::thread;
use std::time::{Duration, Instant};

use minifb::{Key, Window, WindowOptions};

use super::{nearest_neighbour_blit_2x, palette, scene, RenderFrame, RENDER_H, RENDER_W};
use crate::server::Shared;

const WINDOW_W: usize = (RENDER_W * 2) as usize;
const WINDOW_H: usize = (RENDER_H * 2) as usize;
const TARGET_FPS: u64 = 30;

pub fn run_desktop(state: Shared) -> anyhow::Result<()> {
    // state is unused in step 4a (static scene only) but plumbed through so
    // step 4b+ can pull active agents without churning the signature.
    // TODO(step 4b): take a read lock each frame and draw `state.list_active()`
    // sims on top of the static background.
    let _ = state;

    let mut window = Window::new(
        "workplacesim",
        WINDOW_W,
        WINDOW_H,
        WindowOptions::default(),
    )
    .map_err(|e| anyhow::anyhow!("minifb window init failed: {e}"))?;

    // Cap update rate to ~30 fps; minifb otherwise spins at vsync which on a
    // laptop hits 60–120 Hz.
    window.set_target_fps(TARGET_FPS as usize);

    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
    let mut pixels: Vec<u32> = vec![0; WINDOW_W * WINDOW_H];
    let frame_budget = Duration::from_millis(1000 / TARGET_FPS);

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let t0 = Instant::now();
        frame.clear(palette::BG);
        scene::draw_static_background(&mut frame);
        nearest_neighbour_blit_2x(&frame, &mut pixels);
        window
            .update_with_buffer(&pixels, WINDOW_W, WINDOW_H)
            .map_err(|e| anyhow::anyhow!("minifb update: {e}"))?;
        // Sleep any slack. set_target_fps caps the upper bound; this is belt-
        // and-braces for CPU use while the scene is static.
        let elapsed = t0.elapsed();
        if elapsed < frame_budget {
            thread::sleep(frame_budget - elapsed);
        }
    }

    Ok(())
}
