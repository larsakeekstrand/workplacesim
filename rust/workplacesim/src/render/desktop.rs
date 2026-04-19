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
use crate::state::{clock, Agent, Event};

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
    let started_at_ms = clock::now_ms();
    let mut idle_since_ms: Option<u64> = None;
    let mut last_session_ended_ms: Option<u64> = None;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let t0 = Instant::now();
        let now_ms = clock::now_ms();

        // Snapshot under a brief read lock, drop immediately. list_active is
        // the set used to gate the idle-since/status readout: we purposely
        // exclude finished (walk-out) records so the readout reappears the
        // moment Claude restarts, without waiting for STOP_GRACE.
        let (world, agents) = {
            let s = state.read();
            (RenderWorld::from_state(&s, now_ms), s.list_active())
        };
        // Track the last session end for the readout's "last HH:MM:SS" line.
        for a in &agents {
            if a.agent_type == "claude" {
                if let Some(fa) = a.finished_at {
                    last_session_ended_ms =
                        Some(last_session_ended_ms.unwrap_or(0).max(fa));
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
        store.tick(dt_ms, now_ms);
        // Drain after sim_store knows about the new sims so motes can resolve
        // their anchor positions and tethers can verify parents exist.
        fx.drain_events(&mut rx, &store, now_ms);
        fx.tick(now_ms, &mut store);

        frame.clear(palette::BG);
        scene::draw_static_background(&mut frame);
        scene::effects::draw_below(&mut frame, &fx, &store, now_ms);
        scene::sim::draw_sims(&mut frame, &store);
        let agent_refs: Vec<&Agent> = agents.iter().collect();
        scene::glyph::draw_glyphs(&mut frame, &store, &agent_refs, &fx, now_ms);
        scene::effects::draw_above(&mut frame, &fx, &store, now_ms);
        scene::text::draw_whiteboard(&mut frame, &store, &agent_refs);
        scene::text::draw_file_ticker(&mut frame, &fx, now_ms);
        scene::text::draw_bench_flashes(&mut frame, &fx, now_ms);
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
