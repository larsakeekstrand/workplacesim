//! Golden-frame regression for the static scene background. Guards against
//! accidental drift in the procedural-pixel-art port of public/main.js.

use workplacesim::render::classify::{classify, Room};
use workplacesim::render::geometry::{lab_stations, Point};
use workplacesim::render::palette::hash_str;
use workplacesim::render::sim_store::{SimAnim, SimState, SimStore};
use workplacesim::render::{scene, RenderFrame, RENDER_H, RENDER_W};

const GOLDEN_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/static-bg.raw");
const GOLDEN_SIMS_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/three-sims.raw");

#[test]
fn static_background_matches_golden() {
    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
    scene::draw_static_background(&mut frame);
    let actual: Vec<u8> = frame.rgb_bytes().to_vec();

    if std::env::var_os("REGEN").is_some() {
        std::fs::write(GOLDEN_PATH, &actual).expect("write golden");
        eprintln!("wrote {GOLDEN_PATH} ({} bytes)", actual.len());
        return;
    }

    let expected = std::fs::read(GOLDEN_PATH).unwrap_or_else(|e| {
        panic!(
            "golden file missing: {GOLDEN_PATH} ({e}). run with REGEN=1 to create."
        )
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
            overflow_hash: hash_str("mid-walk-desk"),
        },
    );

    // Sim B: mid-walk on the right corridor heading to meeting.
    let room_b = classify("planner", "", "plan");
    let seat_b = store.seats.allocate(room_b, "mid-walk-meeting");
    store.anim.insert(
        "mid-walk-meeting".into(),
        SimAnim {
            agent_id: "mid-walk-meeting".into(),
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
            overflow_hash: hash_str("mid-walk-meeting"),
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
            overflow_hash: hash_str("seated-lab"),
        },
    );
}

#[test]
fn three_sims_matches_golden() {
    let mut store = SimStore::new();
    seed_three_sims(&mut store);

    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
    scene::draw_static_background(&mut frame);
    scene::sim::draw_sims(&mut frame, &store);
    let actual: Vec<u8> = frame.rgb_bytes().to_vec();

    if std::env::var_os("REGEN").is_some() {
        std::fs::write(GOLDEN_SIMS_PATH, &actual).expect("write golden");
        eprintln!("wrote {GOLDEN_SIMS_PATH} ({} bytes)", actual.len());
        return;
    }

    let expected = std::fs::read(GOLDEN_SIMS_PATH).unwrap_or_else(|e| {
        panic!(
            "golden file missing: {GOLDEN_SIMS_PATH} ({e}). run with REGEN=1 to create."
        )
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
