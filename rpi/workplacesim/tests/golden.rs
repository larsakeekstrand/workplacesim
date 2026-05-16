//! Golden-frame regression for the static scene background. Guards against
//! accidental drift in the procedural-pixel-art port of public/main.js.
//!
//! Deterministic time is captured via explicit `now_ms` arguments. Step-6
//! goldens that exercise `draw_status_readout` stub chrono's local time —
//! that path is skipped in the step6-status golden (time zone dependent).

use workplacesim::config;
use workplacesim::render::classify::{classify, Room};
use workplacesim::render::fx_store::{Footstep, FxLimits, FxStore, Halo, Mote, Tether};
use workplacesim::render::geometry::{lab_stations, Point};
use workplacesim::render::palette::{self, hash_str};
use workplacesim::render::sim_store::{SimAnim, SimState, SimStore};
use workplacesim::render::{scene, RenderFrame, RENDER_H, RENDER_W};

// Default runtime config values; tests use these so the goldens reproduce
// the pre-config hardcoded behaviour exactly.
const SPILL_ALPHA: f32 = config::DEFAULT_WINDOW_SPILL_ALPHA;
const ERROR_GLYPH_MS: u64 = config::DEFAULT_ERROR_GLYPH_MS;
fn fx_limits() -> FxLimits {
    FxLimits::default()
}

const GOLDEN_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/static-bg.raw");
const GOLDEN_SIMS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/three-sims.raw");
const GOLDEN_FX_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/fx-scene.raw");

#[test]
fn static_background_matches_golden() {
    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
    scene::draw_static_background(&mut frame, SPILL_ALPHA);
    let actual: Vec<u8> = frame.rgb_bytes().to_vec();

    if std::env::var_os("REGEN").is_some() {
        std::fs::write(GOLDEN_PATH, &actual).expect("write golden");
        eprintln!("wrote {GOLDEN_PATH} ({} bytes)", actual.len());
        return;
    }

    let expected = std::fs::read(GOLDEN_PATH).unwrap_or_else(|e| {
        panic!("golden file missing: {GOLDEN_PATH} ({e}). run with REGEN=1 to create.")
    });
    assert_eq!(
        actual.len(),
        expected.len(),
        "static background size drift ({} vs {})",
        actual.len(),
        expected.len()
    );
    if actual != expected {
        // Surface the first diverging pixel for quick triage.
        for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
            if a != e {
                let px = i / 3;
                let x = px as u32 % RENDER_W;
                let y = px as u32 / RENDER_W;
                panic!(
                    "static background drift at pixel ({x},{y}) byte {i}: actual={a} expected={e}. run with REGEN=1 to update."
                );
            }
        }
        unreachable!("lengths equal but contents differ")
    }
}

fn seed_three_sims(store: &mut SimStore) {
    // Sim A: mid-walk on the left corridor heading east to a desk.
    let room_a = classify("coder", "", "default");
    let seat_a = store.seats.allocate(room_a, "mid-walk-desk");
    store.anim.insert(
        "mid-walk-desk".into(),
        SimAnim {
            agent_id: "mid-walk-desk".into(),
            session_id: None,
            user: "alice".into(),
            permission_mode: "default".into(),
            is_lab: matches!(room_a, Room::Lab),
            x: 300.0,
            y: 256.0,
            path: vec![Point::new(520, 256), Point::new(520, 224)],
            seat: seat_a,
            room: room_a,
            state: SimState::WalkingIn,
            bob_phase: 0.0,
            spawned_at_ms: 0,
            seated_at_ms: None,
            seated_since_ms: None,
            overflow_hash: hash_str("mid-walk-desk"),
            last_footstep_ms: 0,
            session_label: None,
        },
    );

    // Sim B: mid-walk on the right corridor heading to meeting.
    let room_b = classify("planner", "", "plan");
    let seat_b = store.seats.allocate(room_b, "mid-walk-meeting");
    store.anim.insert(
        "mid-walk-meeting".into(),
        SimAnim {
            agent_id: "mid-walk-meeting".into(),
            session_id: None,
            user: "carol".into(),
            permission_mode: "plan".into(),
            is_lab: matches!(room_b, Room::Lab),
            x: 500.0,
            y: 256.0,
            path: vec![
                Point::new(776, 256),
                Point::new(776, 193),
                Point::new(848, 193),
                Point::new(944, 127),
                Point::new(944, 155),
            ],
            seat: seat_b,
            room: room_b,
            state: SimState::WalkingIn,
            bob_phase: 0.0,
            spawned_at_ms: 0,
            seated_at_ms: None,
            seated_since_ms: None,
            overflow_hash: hash_str("mid-walk-meeting"),
            last_footstep_ms: 0,
            session_label: None,
        },
    );

    // Sim C: seated at the first lab station.
    let lab = lab_stations()[0];
    let room_c = classify("verifier", "", "default");
    let seat_c = store.seats.allocate(room_c, "seated-lab");
    store.anim.insert(
        "seated-lab".into(),
        SimAnim {
            agent_id: "seated-lab".into(),
            session_id: None,
            user: "bob".into(),
            permission_mode: "default".into(),
            is_lab: matches!(room_c, Room::Lab),
            x: lab.seat_x as f32,
            y: lab.seat_y as f32,
            path: vec![],
            seat: seat_c,
            room: room_c,
            state: SimState::Seated,
            bob_phase: 0.0,
            spawned_at_ms: 0,
            seated_at_ms: Some(0),
            seated_since_ms: Some(0),
            overflow_hash: hash_str("seated-lab"),
            last_footstep_ms: 0,
            session_label: None,
        },
    );
}

#[test]
fn three_sims_matches_golden() {
    let mut store = SimStore::new();
    seed_three_sims(&mut store);

    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
    scene::draw_static_background(&mut frame, SPILL_ALPHA);
    scene::sim::draw_sims(&mut frame, &store);
    let actual: Vec<u8> = frame.rgb_bytes().to_vec();

    if std::env::var_os("REGEN").is_some() {
        std::fs::write(GOLDEN_SIMS_PATH, &actual).expect("write golden");
        eprintln!("wrote {GOLDEN_SIMS_PATH} ({} bytes)", actual.len());
        return;
    }

    let expected = std::fs::read(GOLDEN_SIMS_PATH).unwrap_or_else(|e| {
        panic!("golden file missing: {GOLDEN_SIMS_PATH} ({e}). run with REGEN=1 to create.")
    });
    assert_eq!(
        actual.len(),
        expected.len(),
        "three-sims size drift ({} vs {})",
        actual.len(),
        expected.len()
    );
    if actual != expected {
        for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
            if a != e {
                let px = i / 3;
                let x = px as u32 % RENDER_W;
                let y = px as u32 / RENDER_W;
                panic!(
                    "three-sims drift at pixel ({x},{y}) byte {i}: actual={a} expected={e}. run with REGEN=1 to update."
                );
            }
        }
        unreachable!("lengths equal but contents differ")
    }
}

fn seed_fx_scene(store: &mut SimStore) -> FxStore {
    seed_three_sims(store);
    let now_ms: u64 = 1_000;
    let mut fx = FxStore::new();
    // Footstep trail behind walker A and walker B (born at staggered times so
    // each step lands at a distinct alpha in the 0.25 → 0 envelope).
    let shirt_a = palette::sim_colors("alice").shirt;
    let shirt_b = palette::sim_colors("carol").shirt;
    for (i, dx) in [0i32, -22, -44, -66].iter().enumerate() {
        let born = now_ms.saturating_sub(200 + (i as u64) * 200);
        fx.footsteps.push(Footstep {
            agent_id: "mid-walk-desk".into(),
            x: 300.0 + *dx as f32,
            y: 256.0 + 10.0,
            color: shirt_a,
            born_ms: born,
        });
        fx.footsteps.push(Footstep {
            agent_id: "mid-walk-meeting".into(),
            x: 500.0 + *dx as f32,
            y: 256.0 + 10.0,
            color: shirt_b,
            born_ms: born,
        });
    }
    fx.motes.push_back(Mote {
        agent_id: "mid-walk-desk".into(),
        x: 300.0,
        y: 256.0 - 18.0,
        color: palette::mote_color("Read"),
        born_ms: now_ms - 200,
    });
    fx.motes.push_back(Mote {
        agent_id: "mid-walk-meeting".into(),
        x: 500.0,
        y: 256.0 - 18.0,
        color: palette::mote_color("Bash"),
        born_ms: now_ms - 400,
    });
    fx.tethers.push(Tether {
        parent: "mid-walk-desk".into(),
        child: "mid-walk-meeting".into(),
        born_ms: now_ms - 300,
    });
    fx.halos.push(Halo {
        agent_id: "seated-lab".into(),
        born_ms: now_ms - 600,
    });
    fx
}

#[test]
fn fx_scene_matches_golden() {
    let mut store = SimStore::new();
    let fx = seed_fx_scene(&mut store);
    let now_ms: u64 = 1_000;

    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
    scene::draw_static_background(&mut frame, SPILL_ALPHA);
    scene::effects::draw_below(&mut frame, &fx, &store, now_ms, &fx_limits());
    scene::sim::draw_sims(&mut frame, &store);
    scene::effects::draw_above(&mut frame, &fx, &store, now_ms, &fx_limits());
    let actual: Vec<u8> = frame.rgb_bytes().to_vec();

    if std::env::var_os("REGEN").is_some() {
        std::fs::write(GOLDEN_FX_PATH, &actual).expect("write golden");
        eprintln!("wrote {GOLDEN_FX_PATH} ({} bytes)", actual.len());
        return;
    }

    let expected = std::fs::read(GOLDEN_FX_PATH).unwrap_or_else(|e| {
        panic!("golden file missing: {GOLDEN_FX_PATH} ({e}). run with REGEN=1 to create.")
    });
    assert_eq!(
        actual.len(),
        expected.len(),
        "fx-scene size drift ({} vs {})",
        actual.len(),
        expected.len()
    );
    if actual != expected {
        for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
            if a != e {
                let px = i / 3;
                let x = px as u32 % RENDER_W;
                let y = px as u32 / RENDER_W;
                panic!(
                    "fx-scene drift at pixel ({x},{y}) byte {i}: actual={a} expected={e}. run with REGEN=1 to update."
                );
            }
        }
        unreachable!("lengths equal but contents differ")
    }
}

// -------- Step 6 goldens --------

use workplacesim::render::fx_store::{BenchFlash, FileTick};
use workplacesim::render::geometry::meeting_seats;
use workplacesim::state::{Agent, CurrentError, Visit};

const GOLDEN_WB_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/step6-whiteboard.raw"
);
const GOLDEN_TICKER_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/step6-ticker.raw");
const GOLDEN_GLYPHS_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/step6-glyphs.raw");
const GOLDEN_BENCH_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/step6-bench.raw");

fn compare_or_regen(path: &str, actual: &[u8]) {
    if std::env::var_os("REGEN").is_some() {
        std::fs::write(path, actual).expect("write golden");
        eprintln!("wrote {path} ({} bytes)", actual.len());
        return;
    }
    let expected = std::fs::read(path).unwrap_or_else(|e| {
        panic!("golden file missing: {path} ({e}). run with REGEN=1 to create.")
    });
    assert_eq!(actual.len(), expected.len(), "golden size drift at {path}");
    if actual != expected {
        for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
            if a != e {
                let px = i / 3;
                let x = px as u32 % RENDER_W;
                let y = px as u32 / RENDER_W;
                panic!(
                    "golden drift at {path} pixel ({x},{y}) byte {i}: actual={a} expected={e}. run with REGEN=1 to update."
                );
            }
        }
        unreachable!("lengths equal but contents differ");
    }
}

#[test]
fn step6_whiteboard_matches_golden() {
    let mut store = SimStore::new();
    let ms = meeting_seats();
    let room = classify("claude", "", "plan");
    let seat = store.seats.allocate(room, "sess");
    store.anim.insert(
        "sess".into(),
        SimAnim {
            agent_id: "sess".into(),
            session_id: None,
            user: "alice".into(),
            permission_mode: "plan".into(),
            is_lab: false,
            x: ms[0].seat_x as f32,
            y: ms[0].seat_y as f32,
            path: vec![],
            seat,
            room,
            state: SimState::Seated,
            bob_phase: 0.0,
            spawned_at_ms: 0,
            seated_at_ms: Some(0),
            seated_since_ms: Some(0),
            overflow_hash: hash_str("sess"),
            last_footstep_ms: 0,
            session_label: None,
        },
    );
    let agent = Agent {
        agent_id: "sess".into(),
        agent_type: "claude".into(),
        session_prompt: Some("port text surfaces".into()),
        permission_mode: "plan".into(),
        ..Default::default()
    };
    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
    scene::draw_static_background(&mut frame, SPILL_ALPHA);
    scene::sim::draw_sims(&mut frame, &store);
    let agents = [&agent];
    scene::text::draw_whiteboard(&mut frame, &store, &agents);
    compare_or_regen(GOLDEN_WB_PATH, frame.rgb_bytes());
}

#[test]
fn step6_ticker_matches_golden() {
    let store = SimStore::new();
    let now_ms = 5_000u64;
    let mut fx = FxStore::new();
    fx.file_ticks.push_back(FileTick {
        path: "src/a.rs".into(),
        born_ms: now_ms - 4_000,
    });
    fx.file_ticks.push_back(FileTick {
        path: "src/b.rs".into(),
        born_ms: now_ms - 2_000,
    });
    fx.file_ticks.push_back(FileTick {
        path: "src/c.rs".into(),
        born_ms: now_ms - 500,
    });
    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
    scene::draw_static_background(&mut frame, SPILL_ALPHA);
    scene::sim::draw_sims(&mut frame, &store);
    scene::text::draw_file_ticker(&mut frame, &fx, now_ms, &fx_limits());
    compare_or_regen(GOLDEN_TICKER_PATH, frame.rgb_bytes());
}

#[test]
fn step6_glyphs_matches_golden() {
    let mut store = SimStore::new();
    let mut agents: Vec<Agent> = Vec::new();
    let now_ms = 120_000u64;
    let mk_sim = |store: &mut SimStore,
                  id: &str,
                  user: &str,
                  room: Room,
                  pos: (i32, i32),
                  mode: &str,
                  state: SimState,
                  seated_since: Option<u64>| {
        let seat = store.seats.allocate(room, id);
        store.anim.insert(
            id.into(),
            SimAnim {
                agent_id: id.into(),
                session_id: None,
                user: user.into(),
                permission_mode: mode.into(),
                is_lab: matches!(room, Room::Lab),
                x: pos.0 as f32,
                y: pos.1 as f32,
                path: if matches!(state, SimState::Seated) {
                    vec![]
                } else {
                    vec![Point::new(pos.0 + 20, pos.1)]
                },
                seat,
                room,
                state,
                bob_phase: 0.0,
                spawned_at_ms: 0,
                seated_at_ms: seated_since,
                seated_since_ms: seated_since,
                overflow_hash: hash_str(id),
                last_footstep_ms: 0,
                session_label: None,
            },
        );
    };

    let labs = lab_stations();
    mk_sim(
        &mut store,
        "lab",
        "bob",
        Room::Lab,
        (labs[1].seat_x, labs[1].seat_y),
        "default",
        SimState::Seated,
        Some(now_ms - 5_000),
    );
    agents.push(Agent {
        agent_id: "lab".into(),
        visit: Some(Visit {
            room: "test".into(),
            until: now_ms + 10_000,
        }),
        ..Default::default()
    });

    mk_sim(
        &mut store,
        "walker",
        "carol",
        Room::Desk,
        (300, 256),
        "default",
        SimState::WalkingIn,
        None,
    );
    agents.push(Agent {
        agent_id: "walker".into(),
        ..Default::default()
    });

    mk_sim(
        &mut store,
        "veteran",
        "dave",
        Room::Desk,
        (360, 384),
        "default",
        SimState::Seated,
        Some(now_ms - 90_000),
    );
    agents.push(Agent {
        agent_id: "veteran".into(),
        ..Default::default()
    });

    mk_sim(
        &mut store,
        "idler",
        "eve",
        Room::Desk,
        (520, 384),
        "default",
        SimState::Seated,
        Some(now_ms - 10_000),
    );
    agents.push(Agent {
        agent_id: "idler".into(),
        idle: Some(true),
        ..Default::default()
    });

    let ms = meeting_seats();
    mk_sim(
        &mut store,
        "plan",
        "flora",
        Room::Meeting,
        (ms[0].seat_x, ms[0].seat_y),
        "plan",
        SimState::Seated,
        Some(now_ms - 100),
    );
    agents.push(Agent {
        agent_id: "plan".into(),
        ..Default::default()
    });

    mk_sim(
        &mut store,
        "erred",
        "frank",
        Room::Desk,
        (680, 384),
        "default",
        SimState::Seated,
        Some(now_ms - 2_000),
    );
    agents.push(Agent {
        agent_id: "erred".into(),
        current_error: Some(CurrentError {
            tool_name: "Bash".into(),
            message: "e".into(),
            ts: now_ms - 500,
        }),
        ..Default::default()
    });

    let fx = FxStore::new();
    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
    scene::draw_static_background(&mut frame, SPILL_ALPHA);
    scene::sim::draw_sims(&mut frame, &store);
    let agent_refs: Vec<&Agent> = agents.iter().collect();
    scene::glyph::draw_glyphs(&mut frame, &store, &agent_refs, &fx, now_ms, ERROR_GLYPH_MS);
    compare_or_regen(GOLDEN_GLYPHS_PATH, frame.rgb_bytes());
}

#[test]
fn step6_bench_matches_golden() {
    let mut store = SimStore::new();
    let labs = lab_stations();
    let room = classify("verifier", "", "default");
    let seat = store.seats.allocate(room, "lab");
    store.anim.insert(
        "lab".into(),
        SimAnim {
            agent_id: "lab".into(),
            session_id: None,
            user: "bob".into(),
            permission_mode: "default".into(),
            is_lab: true,
            x: labs[1].seat_x as f32,
            y: labs[1].seat_y as f32,
            path: vec![],
            seat,
            room,
            state: SimState::Seated,
            bob_phase: 0.0,
            spawned_at_ms: 0,
            seated_at_ms: Some(0),
            seated_since_ms: Some(0),
            overflow_hash: hash_str("lab"),
            last_footstep_ms: 0,
            session_label: None,
        },
    );
    let now_ms = 1_000u64;
    let mut fx = FxStore::new();
    fx.bench_flashes.push(BenchFlash {
        station_idx: 1,
        ok: true,
        born_ms: now_ms - 200,
    });
    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
    scene::draw_static_background(&mut frame, SPILL_ALPHA);
    scene::sim::draw_sims(&mut frame, &store);
    scene::text::draw_bench_flashes(&mut frame, &fx, now_ms, &fx_limits());
    compare_or_regen(GOLDEN_BENCH_PATH, frame.rgb_bytes());
}

// Session-label chest glyphs. Painted in `scene::sim::draw_sim` so this
// golden also gates the per-session char position, contour, and pool order.
const GOLDEN_SESSION_LABEL_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/session-labels.raw"
);

#[test]
fn session_labels_paint_on_chest() {
    let mut store = SimStore::new();
    let labs = lab_stations();
    let ms = meeting_seats();
    // Three sims, three distinct session_labels, three rooms — exercises
    // the chest-text path against shirt/lab/meeting backgrounds.
    let room_a = classify("coder", "", "default");
    let seat_a = store.seats.allocate(room_a, "lbl-1");
    store.anim.insert(
        "lbl-1".into(),
        SimAnim {
            agent_id: "lbl-1".into(),
            session_id: Some("sess-1".into()),
            user: "alice".into(),
            permission_mode: "default".into(),
            is_lab: matches!(room_a, Room::Lab),
            x: 360.0,
            y: 384.0,
            path: vec![],
            seat: seat_a,
            room: room_a,
            state: SimState::Seated,
            bob_phase: 0.0,
            spawned_at_ms: 0,
            seated_at_ms: Some(0),
            seated_since_ms: Some(0),
            overflow_hash: hash_str("lbl-1"),
            last_footstep_ms: 0,
            session_label: Some("1".into()),
        },
    );
    let room_b = classify("verifier", "", "default");
    let seat_b = store.seats.allocate(room_b, "lbl-7");
    store.anim.insert(
        "lbl-7".into(),
        SimAnim {
            agent_id: "lbl-7".into(),
            session_id: Some("sess-2".into()),
            user: "bob".into(),
            permission_mode: "default".into(),
            is_lab: true,
            x: labs[0].seat_x as f32,
            y: labs[0].seat_y as f32,
            path: vec![],
            seat: seat_b,
            room: room_b,
            state: SimState::Seated,
            bob_phase: 0.0,
            spawned_at_ms: 0,
            seated_at_ms: Some(0),
            seated_since_ms: Some(0),
            overflow_hash: hash_str("lbl-7"),
            last_footstep_ms: 0,
            session_label: Some("7".into()),
        },
    );
    let room_c = classify("planner", "", "plan");
    let seat_c = store.seats.allocate(room_c, "lbl-A");
    store.anim.insert(
        "lbl-A".into(),
        SimAnim {
            agent_id: "lbl-A".into(),
            session_id: Some("sess-3".into()),
            user: "carol".into(),
            permission_mode: "plan".into(),
            is_lab: false,
            x: ms[0].seat_x as f32,
            y: ms[0].seat_y as f32,
            path: vec![],
            seat: seat_c,
            room: room_c,
            state: SimState::Seated,
            bob_phase: 0.0,
            spawned_at_ms: 0,
            seated_at_ms: Some(0),
            seated_since_ms: Some(0),
            overflow_hash: hash_str("lbl-A"),
            last_footstep_ms: 0,
            session_label: Some("A".into()),
        },
    );
    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
    scene::draw_static_background(&mut frame, SPILL_ALPHA);
    scene::sim::draw_sims(&mut frame, &store);
    compare_or_regen(GOLDEN_SESSION_LABEL_PATH, frame.rgb_bytes());
}

// Status readout uses local-time via chrono; we test the logic rather than a
// golden frame so the snapshot is TZ-independent. See unit tests in text.rs.
