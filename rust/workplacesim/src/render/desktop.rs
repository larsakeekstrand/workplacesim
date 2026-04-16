//! minifb-backed desktop renderer. macOS/Linux-friendly path for visual QA
//! while the Pi /dev/fb0 backend (step 5) is still offline.
//!
//! Must run on the main thread on macOS (AppKit window-management rule). The
//! axum server lives on a background tokio runtime; see `src/main.rs` for
//! the bootstrap.

use std::thread;
use std::time::{Duration, Instant};

use minifb::{Key, Window, WindowOptions};
use tokio::sync::broadcast;

use super::fx_store::FxStore;
use super::sim_store::SimStore;
use super::world::RenderWorld;
use super::{nearest_neighbour_blit_2x, palette, scene, RenderFrame, RENDER_H, RENDER_W};
use crate::server::Shared;
use crate::state::{clock, Event};

const WINDOW_W: usize = (RENDER_W * 2) as usize;
const WINDOW_H: usize = (RENDER_H * 2) as usize;
const TARGET_FPS: u64 = 30;

pub fn run_desktop(state: Shared, mut rx: broadcast::Receiver<Event>) -> anyhow::Result<()> {
    let mut window = Window::new(
        "workplacesim",
        WINDOW_W,
        WINDOW_H,
        WindowOptions::default(),
    )
    .map_err(|e| anyhow::anyhow!("minifb window init failed: {e}"))?;

    window.set_target_fps(TARGET_FPS as usize);

    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
    let mut pixels: Vec<u32> = vec![0; WINDOW_W * WINDOW_H];
    let frame_budget = Duration::from_millis(1000 / TARGET_FPS);

    let mut store = SimStore::new();
    let mut fx = FxStore::new();
    let mut last_tick = Instant::now();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let t0 = Instant::now();
        let now_ms = clock::now_ms();

        // Snapshot under a brief read lock, drop immediately.
        let world = {
            let s = state.read();
            RenderWorld::from_state(&s, now_ms)
        };
        // Advance time-based state transitions (visit expiry, stop grace, etc).
        {
            let mut s = state.write();
            s.tick(now_ms);
        }

        store.reconcile(&world);
        let dt_ms = last_tick.elapsed().as_millis() as u64;
        last_tick = t0;
        store.tick(dt_ms, now_ms);
        // Drain after sim_store knows about the new sims so motes can resolve
        // their anchor positions and tethers can verify parents exist.
        fx.drain_events(&mut rx, &store, now_ms);
        fx.tick(now_ms, &mut store);

        frame.clear(palette::BG);
        scene::draw_static_background(&mut frame);
        scene::effects::draw_below(&mut frame, &fx, &store, now_ms);
        scene::sim::draw_sims(&mut frame, &store);
        scene::effects::draw_above(&mut frame, &fx, &store, now_ms);

        nearest_neighbour_blit_2x(&frame, &mut pixels);
        window
            .update_with_buffer(&pixels, WINDOW_W, WINDOW_H)
            .map_err(|e| anyhow::anyhow!("minifb update: {e}"))?;
        let elapsed = t0.elapsed();
        if elapsed < frame_budget {
            thread::sleep(frame_budget - elapsed);
        }
    }

    Ok(())
}
