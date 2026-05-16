//! Seed a known 3-sim scene into a `SimStore`, render one frame, write PNG.
//! Matches the step 4b verification harness; not part of the shipped binary.

use workplacesim::config;
use workplacesim::render::classify::{classify, Room};
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
    let overflow_hash = workplacesim::render::palette::hash_str(id);
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
        seated_since_ms: if matches!(state, SimState::Seated) {
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

    // Sim 1: mid-walk toward a desk. Position on the left corridor heading east.
    seed_sim(
        &mut store,
        "mid-walk-desk",
        "alice",
        "coder",
        "default",
        SimState::WalkingIn,
        Point::new(300, 256),
        vec![Point::new(520, 256), Point::new(520, 224)],
    );

    // Sim 2: mid-walk toward the meeting room.
    seed_sim(
        &mut store,
        "mid-walk-meeting",
        "carol",
        "planner",
        "plan",
        SimState::WalkingIn,
        Point::new(500, 256),
        vec![
            Point::new(776, 256),
            Point::new(776, 193),
            Point::new(848, 193),
            Point::new(944, 127),
            Point::new(944, 155),
        ],
    );

    // Sim 3: seated in the lab.
    let lab_seats = workplacesim::render::geometry::lab_stations();
    let seated = lab_seats[0];
    seed_sim(
        &mut store,
        "seated-lab",
        "bob",
        "verifier",
        "default",
        SimState::Seated,
        Point::new(seated.seat_x, seated.seat_y),
        vec![],
    );

    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
    frame.clear(palette::BG);
    scene::draw_static_background(&mut frame, config::DEFAULT_WINDOW_SPILL_ALPHA);
    scene::sim::draw_sims(&mut frame, &store);

    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/workplacesim-sims.png".to_string());
    let buf = image::RgbImage::from_raw(RENDER_W, RENDER_H, frame.rgb_bytes().to_vec())
        .ok_or_else(|| anyhow::anyhow!("RgbImage::from_raw — buffer size mismatch"))?;
    buf.save(&path)?;
    println!("wrote {path}");
    Ok(())
}
