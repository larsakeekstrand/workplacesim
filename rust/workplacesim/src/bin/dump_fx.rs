//! Seed a sims+effects scene, render one frame, write a PNG. Mirrors the
//! step 4d verification harness; not part of the shipped binary.

use std::collections::VecDeque;

use workplacesim::render::classify::{classify, Room};
use workplacesim::render::fx_store::{Footstep, FxStore, Halo, Mote, Tether};
use workplacesim::render::geometry::Point;
use workplacesim::render::palette;
use workplacesim::render::scene;
use workplacesim::render::sim_store::{SimAnim, SimState, SimStore};
use workplacesim::render::{RenderFrame, RENDER_H, RENDER_W};

#[allow(clippy::too_many_arguments)]
fn seed_sim(
    store: &mut SimStore,
    id: &str,
    user: &str,
    ty: &str,
    mode: &str,
    state: SimState,
    pos: Point,
    path: Vec<Point>,
) {
    let room = classify(ty, "", mode);
    let overflow_hash = palette::hash_str(id);
    let seat = store.seats.allocate(room, id);
    let sim = SimAnim {
        agent_id: id.into(),
        session_id: None,
        user: user.into(),
        permission_mode: mode.into(),
        is_lab: matches!(room, Room::Lab),
        x: pos.x as f32,
        y: pos.y as f32,
        path,
        seat,
        room,
        state,
        bob_phase: 0.0,
        spawned_at_ms: 0,
        seated_at_ms: if matches!(state, SimState::Seated) {
            Some(0)
        } else {
            None
        },
        overflow_hash,
        last_footstep_ms: 0,
    };
    store.anim.insert(id.into(), sim);
}

fn main() -> anyhow::Result<()> {
    let mut store = SimStore::new();

    seed_sim(
        &mut store,
        "walker-a",
        "alice",
        "coder",
        "default",
        SimState::WalkingIn,
        Point::new(300, 256),
        vec![Point::new(520, 256), Point::new(520, 224)],
    );

    seed_sim(
        &mut store,
        "walker-b",
        "carol",
        "planner",
        "plan",
        SimState::WalkingIn,
        Point::new(500, 256),
        vec![
            Point::new(776, 256),
            Point::new(776, 193),
            Point::new(848, 193),
        ],
    );

    let lab_seats = workplacesim::render::geometry::lab_stations();
    let seated = lab_seats[0];
    seed_sim(
        &mut store,
        "errored-lab",
        "bob",
        "verifier",
        "default",
        SimState::Seated,
        Point::new(seated.seat_x, seated.seat_y),
        vec![],
    );

    // Synthesise a populated FxStore. Footstep trail behind the two walkers,
    // one mote per walker, a tether between A (parent) and B (child),
    // an error halo around the seated lab sim.
    let now_ms = 1_000;
    let mut fx = FxStore {
        footsteps: Vec::new(),
        motes: VecDeque::new(),
        tethers: Vec::new(),
        halos: Vec::new(),
    };

    for (offset_ms, dx) in [(0, 0), (120, -22), (240, -44), (360, -66)] {
        let shirt_a = palette::sim_colors("alice").shirt;
        fx.footsteps.push(Footstep {
            agent_id: "walker-a".into(),
            x: 300.0 + dx as f32,
            y: 266.0,
            color: shirt_a,
            born_ms: now_ms - (FOOTSTEP_AGE_MAX - offset_ms),
        });
        let shirt_b = palette::sim_colors("carol").shirt;
        fx.footsteps.push(Footstep {
            agent_id: "walker-b".into(),
            x: 500.0 + dx as f32,
            y: 266.0,
            color: shirt_b,
            born_ms: now_ms - (FOOTSTEP_AGE_MAX - offset_ms),
        });
    }

    fx.motes.push_back(Mote {
        agent_id: "walker-a".into(),
        x: 300.0,
        y: 256.0 - 18.0,
        color: palette::mote_color("Read"),
        born_ms: now_ms - 200,
    });
    fx.motes.push_back(Mote {
        agent_id: "walker-b".into(),
        x: 500.0,
        y: 256.0 - 18.0,
        color: palette::mote_color("Bash"),
        born_ms: now_ms - 400,
    });

    fx.tethers.push(Tether {
        parent: "walker-a".into(),
        child: "walker-b".into(),
        born_ms: now_ms - 200,
    });

    fx.halos.push(Halo {
        agent_id: "errored-lab".into(),
        born_ms: now_ms - 600,
    });

    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
    frame.clear(palette::BG);
    scene::draw_static_background(&mut frame);
    scene::effects::draw_below(&mut frame, &fx, &store, now_ms);
    scene::sim::draw_sims(&mut frame, &store);
    scene::effects::draw_above(&mut frame, &fx, &store, now_ms);

    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/workplacesim-fx.png".to_string());
    let buf = image::RgbImage::from_raw(RENDER_W, RENDER_H, frame.rgb_bytes().to_vec())
        .ok_or_else(|| anyhow::anyhow!("RgbImage::from_raw — buffer size mismatch"))?;
    buf.save(&path)?;
    println!("wrote {path}");
    Ok(())
}

const FOOTSTEP_AGE_MAX: u64 = 800;
