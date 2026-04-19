//! Consolidated text + glyph + bench-flash + status readout demo frame. Seeds
//! a scene exercising every step-6 surface and writes a PNG to /tmp. Only used
//! for visual verification — not part of the shipped binary.

use workplacesim::render::classify::{classify, Room};
use workplacesim::render::fx_store::{BenchFlash, FileTick, FxStore, Halo};
use workplacesim::render::geometry::{lab_stations, meeting_seats, Point};
use workplacesim::render::palette;
use workplacesim::render::scene;
use workplacesim::render::sim_store::{SimAnim, SimState, SimStore};
use workplacesim::render::{RenderFrame, RENDER_H, RENDER_W};
use workplacesim::state::{Agent, CurrentError, Visit};

#[allow(clippy::too_many_arguments)]
fn seed(
    store: &mut SimStore,
    agents: &mut Vec<Agent>,
    id: &str,
    user: &str,
    ty: &str,
    mode: &str,
    state: SimState,
    pos: Point,
    seated_since: Option<u64>,
    agent_extras: impl FnOnce(&mut Agent),
) {
    let room = classify(ty, "", mode);
    let overflow = palette::hash_str(id);
    let seat = store.seats.allocate(room, id);
    store.anim.insert(
        id.into(),
        SimAnim {
            agent_id: id.into(),
            session_id: None,
            user: user.into(),
            permission_mode: mode.into(),
            is_lab: matches!(room, Room::Lab),
            x: pos.x as f32,
            y: pos.y as f32,
            path: if matches!(state, SimState::Seated) {
                vec![]
            } else {
                vec![Point::new(pos.x + 20, pos.y)]
            },
            seat,
            room,
            state,
            bob_phase: 0.0,
            spawned_at_ms: 0,
            seated_at_ms: seated_since,
            seated_since_ms: seated_since,
            overflow_hash: overflow,
            last_footstep_ms: 0,
        },
    );
    let mut a = Agent {
        agent_id: id.into(),
        agent_type: ty.into(),
        user: user.into(),
        permission_mode: mode.into(),
        ..Default::default()
    };
    agent_extras(&mut a);
    agents.push(a);
}

fn main() -> anyhow::Result<()> {
    let mut store = SimStore::new();
    let mut agents: Vec<Agent> = Vec::new();
    let now_ms: u64 = 120_000;

    // Meeting sim with a session_prompt for the whiteboard.
    let ms = meeting_seats();
    seed(
        &mut store,
        &mut agents,
        "sess-claude",
        "alice",
        "claude",
        "plan",
        SimState::Seated,
        Point::new(ms[0].seat_x, ms[0].seat_y),
        Some(now_ms),
        |a| {
            a.session_prompt = Some("port text surfaces and sim glyphs".into());
        },
    );

    // Lab sim visiting "test" -> flask glyph + bench flash on its station.
    let labs = lab_stations();
    seed(
        &mut store,
        &mut agents,
        "lab-verify",
        "bob",
        "verifier",
        "default",
        SimState::Seated,
        Point::new(labs[1].seat_x, labs[1].seat_y),
        Some(now_ms - 5_000),
        |a| {
            a.visit = Some(Visit {
                room: "test".into(),
                until: now_ms + 10_000,
            });
        },
    );

    // Walking sim -> walking glyph.
    seed(
        &mut store,
        &mut agents,
        "walker",
        "carol",
        "planner",
        "default",
        SimState::WalkingIn,
        Point::new(300, 256),
        None,
        |_| {},
    );

    // Long-seated desk sim.
    seed(
        &mut store,
        &mut agents,
        "veteran",
        "dave",
        "coder",
        "default",
        SimState::Seated,
        Point::new(360, 384),
        Some(now_ms - 90_000),
        |_| {},
    );

    // Idle-and-seated sim.
    seed(
        &mut store,
        &mut agents,
        "idle-sess",
        "eve",
        "coder",
        "default",
        SimState::Seated,
        Point::new(520, 384),
        Some(now_ms - 10_000),
        |a| {
            a.idle = Some(true);
        },
    );

    // Error sim — recent tool-error -> `!` glyph + halo.
    seed(
        &mut store,
        &mut agents,
        "erred",
        "frank",
        "coder",
        "default",
        SimState::Seated,
        Point::new(680, 384),
        Some(now_ms - 2_000),
        |a| {
            a.current_error = Some(CurrentError {
                tool_name: "Bash".into(),
                message: "exit 1".into(),
                ts: now_ms - 500,
            });
        },
    );

    // File-touch ticker + one bench flash pre-seeded.
    let mut fx = FxStore::new();
    fx.file_ticks.push_back(FileTick {
        path: "src/render/scene/text.rs".into(),
        born_ms: now_ms - 3_000,
    });
    fx.file_ticks.push_back(FileTick {
        path: "src/render/scene/glyph.rs".into(),
        born_ms: now_ms - 1_500,
    });
    fx.file_ticks.push_back(FileTick {
        path: "tests/golden.rs".into(),
        born_ms: now_ms - 300,
    });
    fx.bench_flashes.push(BenchFlash {
        station_idx: 1,
        ok: true,
        born_ms: now_ms - 200,
    });
    fx.halos.push(Halo {
        agent_id: "erred".into(),
        born_ms: now_ms - 500,
    });

    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
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
    scene::text::draw_status_readout(&mut frame, now_ms, 0, Some(now_ms - 45_000));

    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/workplacesim-text.png".to_string());
    let buf = image::RgbImage::from_raw(RENDER_W, RENDER_H, frame.rgb_bytes().to_vec())
        .ok_or_else(|| anyhow::anyhow!("RgbImage::from_raw — buffer size mismatch"))?;
    buf.save(&path)?;
    println!("wrote {path}");
    Ok(())
}
