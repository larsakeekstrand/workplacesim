//! Per-sim animation state, lives only in the render thread. Drives the
//! walk-in / seated / walk-out state machine by diffing a `RenderWorld` each
//! frame against the `SimAnim`s it already knows about.
//!
//! Ownership: no locks here. `State` is the authoritative hook-ingest store;
//! `SimStore` is derived state. Bugs here do not corrupt the agent record;
//! worst case a sim mis-animates on screen.

use std::collections::HashMap;

use super::classify::{classify, Room};
use super::geometry::{
    desk_seats, lab_stations, meeting_seats, Point, DOOR, LAB_QUEUE_SPOTS, MEETING_QUEUE_SPOTS,
    OUTSIDE_X, QUEUE_SPOTS,
};
use super::palette::hash_str;
use super::routing::{compute_route, path_to_door_from, Target};
use super::world::RenderWorld;

// Walk speed, min-segment, and bob cycle now live on `Config` (see
// `crate::config`). These used to be module-level consts; Task #2 of the
// Ethereal Thimble plan threads them through per-frame so the config website
// can retune motion live. Call sites take the values as parameters.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SeatId {
    Desk(usize),
    Meeting(usize),
    Lab(usize),
}

/// Occupancy table. One slot per physical seat; first-free allocation. Queue
/// overflow lives outside this struct — see `QueueSpot`.
#[derive(Clone, Debug, Default)]
pub struct SeatRegistry {
    desks: [Option<String>; 12],
    meeting: [Option<String>; 4],
    lab: [Option<String>; 3],
}

impl SeatRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allocate(&mut self, room: Room, agent_id: &str) -> Option<SeatId> {
        match room {
            Room::Desk => allocate_in(&mut self.desks, agent_id).map(SeatId::Desk),
            Room::Meeting => allocate_in(&mut self.meeting, agent_id).map(SeatId::Meeting),
            Room::Lab => allocate_in(&mut self.lab, agent_id).map(SeatId::Lab),
        }
    }

    pub fn release(&mut self, seat: SeatId) {
        match seat {
            SeatId::Desk(i) => self.desks[i] = None,
            SeatId::Meeting(i) => self.meeting[i] = None,
            SeatId::Lab(i) => self.lab[i] = None,
        }
    }

    #[cfg(test)]
    pub fn holder(&self, seat: SeatId) -> Option<&str> {
        match seat {
            SeatId::Desk(i) => self.desks[i].as_deref(),
            SeatId::Meeting(i) => self.meeting[i].as_deref(),
            SeatId::Lab(i) => self.lab[i].as_deref(),
        }
    }
}

fn allocate_in(slots: &mut [Option<String>], agent_id: &str) -> Option<usize> {
    for (i, slot) in slots.iter_mut().enumerate() {
        if slot.is_none() {
            *slot = Some(agent_id.to_string());
            return Some(i);
        }
    }
    None
}

/// A target resolved into a concrete position. For MVP, overflow queues fold
/// into the same `Target` enum from routing; we only track whether we hold a
/// seat allocation (and which) so release is easy.
fn target_for(room: Room, seat: Option<SeatId>, overflow_hash: u32) -> Target {
    match (room, seat) {
        (Room::Desk, Some(SeatId::Desk(i))) => Target::Desk(desk_seats()[i]),
        (Room::Meeting, Some(SeatId::Meeting(i))) => Target::Meeting(meeting_seats()[i]),
        (Room::Lab, Some(SeatId::Lab(i))) => Target::Lab(lab_stations()[i]),
        (Room::Desk, _) => {
            let spot = QUEUE_SPOTS[overflow_hash as usize % QUEUE_SPOTS.len()];
            Target::Queue(spot)
        }
        (Room::Meeting, _) => {
            let spot = MEETING_QUEUE_SPOTS[overflow_hash as usize % MEETING_QUEUE_SPOTS.len()];
            Target::MeetingQueue(spot)
        }
        (Room::Lab, _) => {
            let spot = LAB_QUEUE_SPOTS[overflow_hash as usize % LAB_QUEUE_SPOTS.len()];
            Target::LabQueue(spot)
        }
        // SeatId mismatches shouldn't happen, but treat as overflow fallback.
        #[allow(unreachable_patterns)]
        _ => {
            let spot = QUEUE_SPOTS[overflow_hash as usize % QUEUE_SPOTS.len()];
            Target::Queue(spot)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimState {
    WalkingIn,
    Seated,
    WalkingOut,
    Gone,
}

#[derive(Clone, Debug)]
pub struct SimAnim {
    pub agent_id: String,
    pub session_id: Option<String>,
    pub user: String,
    /// "plan" / "test" / "default" — used by body-part drawing for the badge.
    pub permission_mode: String,
    pub is_lab: bool,
    pub x: f32,
    pub y: f32,
    /// Remaining waypoints in JS world coords; sim walks to path[0] next.
    pub path: Vec<Point>,
    pub seat: Option<SeatId>,
    pub room: Room,
    pub state: SimState,
    pub bob_phase: f32,
    pub spawned_at_ms: u64,
    pub seated_at_ms: Option<u64>,
    /// Set when the sim *transitions* to Seated; cleared on transition out.
    /// Distinct from `seated_at_ms` (which is recorded once at first seating
    /// and we keep for external inspection): this one drives the `Z` glyph.
    pub seated_since_ms: Option<u64>,
    /// For overflow queue-spot selection when seats are full.
    pub overflow_hash: u32,
    /// Last frame ms at which a footstep was dropped for this sim. Owned here
    /// so FxStore stays a pure ring-buffer with no per-sim shadow state.
    pub last_footstep_ms: u64,
}

impl SimAnim {
    /// Convenience — is the sim still actively participating in the scene.
    pub fn is_alive(&self) -> bool {
        !matches!(self.state, SimState::Gone)
    }
}

pub struct SimStore {
    pub anim: HashMap<String, SimAnim>,
    pub seats: SeatRegistry,
}

impl Default for SimStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SimStore {
    pub fn new() -> Self {
        Self {
            anim: HashMap::new(),
            seats: SeatRegistry::new(),
        }
    }

    /// Diff the world against known sims and mutate the store so the tick step
    /// has something to animate. Called each frame before `tick`.
    pub fn reconcile(&mut self, world: &RenderWorld) {
        // 1. Spawn new sims.
        for a in &world.agents {
            if self.anim.contains_key(&a.agent_id) {
                continue;
            }
            // Skip already-finished agents we never saw — they'd walk in and
            // immediately walk back out. Just skip; `state.tick()` will drop
            // them shortly.
            if a.finished_at.is_some() {
                continue;
            }
            let room = classify(&a.agent_type, &a.description, &a.permission_mode);
            let overflow_hash = hash_str(&a.agent_id);
            let seat = self.seats.allocate(room, &a.agent_id);
            let target = target_for(room, seat, overflow_hash);
            let start = Point::new(OUTSIDE_X, DOOR.y);
            let path = compute_route(start, &target);

            let sim = SimAnim {
                agent_id: a.agent_id.clone(),
                session_id: a.session_id.clone(),
                user: a.user.clone(),
                permission_mode: a.permission_mode.clone(),
                is_lab: matches!(room, Room::Lab),
                x: start.x as f32,
                y: start.y as f32,
                path,
                seat,
                room,
                state: SimState::WalkingIn,
                bob_phase: 0.0,
                spawned_at_ms: a.started_at,
                seated_at_ms: None,
                seated_since_ms: None,
                overflow_hash,
                last_footstep_ms: 0,
            };
            self.anim.insert(a.agent_id.clone(), sim);
        }

        // 2. Mark newly-finished sims as walking out.
        // Collect ids first — iterating with mutable &self.anim breaks borrow.
        let finishing: Vec<String> = world
            .agents
            .iter()
            .filter_map(|a| {
                a.finished_at?;
                let sim = self.anim.get(&a.agent_id)?;
                if matches!(sim.state, SimState::WalkingOut | SimState::Gone) {
                    return None;
                }
                Some(a.agent_id.clone())
            })
            .collect();
        for id in finishing {
            if let Some(seat) = self.anim.get(&id).and_then(|s| s.seat) {
                self.seats.release(seat);
            }
            if let Some(sim) = self.anim.get_mut(&id) {
                sim.seat = None;
                sim.state = SimState::WalkingOut;
                sim.seated_since_ms = None;
                // Synthesise a walk-out from the sim's current position. The
                // simplest faithful port: snap to the sim's last seat target
                // and use `path_to_door_from`. We reconstruct a Target from
                // room + overflow_hash; since the seat is released, use a
                // queue spot at the sim's room as the "home" — path_to_door
                // only needs a room-shaped point to compute the door route.
                let door_target = target_for(sim.room, None, sim.overflow_hash);
                sim.path = path_to_door_from(&door_target);
            }
        }

        // 3. Drop sims that finished walking out AND vanished from the world.
        let present: std::collections::HashSet<&str> =
            world.agents.iter().map(|a| a.agent_id.as_str()).collect();
        self.anim.retain(|id, sim| {
            if matches!(sim.state, SimState::Gone) && !present.contains(id.as_str()) {
                return false;
            }
            true
        });
    }

    /// Advance each sim's position / phase by `dt_ms`. Reads nothing from the
    /// outside world; `reconcile` is responsible for feeding fresh paths.
    ///
    /// `walk_speed_px_per_sec` and `bob_cycle_ms` come from `Config` — the
    /// caller snapshots them under a single short read-lock per frame. The
    /// `MIN_SEGMENT_MS` floor from the JS port lives only in the route builder;
    /// the inner per-ms loop here isn't segment-aware so the min-segment knob
    /// is a no-op for the Rust renderer (unchanged from the pre-config code,
    /// which also didn't reference it in `tick`).
    pub fn tick(&mut self, dt_ms: u64, now_ms: u64, walk_speed_px_per_sec: f32, bob_cycle_ms: u64) {
        // Derive bob progression per ms. `.max(1)` guards against a clamp
        // bypass that could otherwise divide by zero.
        let bob_rad_per_ms = std::f32::consts::TAU / bob_cycle_ms.max(1) as f32;
        for sim in self.anim.values_mut() {
            match sim.state {
                SimState::WalkingIn | SimState::WalkingOut => {
                    advance_along_path(sim, dt_ms, walk_speed_px_per_sec);
                    if sim.path.is_empty() {
                        match sim.state {
                            SimState::WalkingIn => {
                                sim.state = SimState::Seated;
                                sim.seated_at_ms = Some(now_ms);
                                sim.seated_since_ms = Some(now_ms);
                            }
                            SimState::WalkingOut => {
                                sim.state = SimState::Gone;
                                sim.seated_since_ms = None;
                            }
                            _ => {}
                        }
                    }
                }
                SimState::Seated => {
                    sim.bob_phase =
                        (sim.bob_phase + bob_rad_per_ms * dt_ms as f32) % std::f32::consts::TAU;
                }
                SimState::Gone => {}
            }
        }
    }

    /// Iterator over all sims in arbitrary order. Paint order in the renderer
    /// is y-sorted (north-first) so the closer body occludes the farther one.
    pub fn iter(&self) -> impl Iterator<Item = &SimAnim> {
        self.anim.values()
    }
}

/// Walk a sim toward its next waypoint. If it reaches it, pop and keep moving
/// with remaining budget. Frame cadence is ~30 fps so dt ~= 33 ms; segments can
/// be much shorter so we loop.
fn advance_along_path(sim: &mut SimAnim, dt_ms: u64, walk_speed_px_per_sec: f32) {
    // Guard: a zero speed would stall forever; clamp lives in `Config::clamp`
    // but the hot path has to be safe too.
    let speed = walk_speed_px_per_sec.max(1.0);
    let mut remaining_ms = dt_ms as f32;
    while remaining_ms > 0.0 && !sim.path.is_empty() {
        let wp = sim.path[0];
        let dx = wp.x as f32 - sim.x;
        let dy = wp.y as f32 - sim.y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < 0.001 {
            sim.path.remove(0);
            continue;
        }
        let step = speed * remaining_ms / 1000.0;
        if step >= dist {
            sim.x = wp.x as f32;
            sim.y = wp.y as f32;
            let consumed_ms = dist * 1000.0 / speed;
            remaining_ms = (remaining_ms - consumed_ms).max(0.0);
            sim.path.remove(0);
        } else {
            let k = step / dist;
            sim.x += dx * k;
            sim.y += dy * k;
            remaining_ms = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::render::world::AgentView;

    // Pin tick values to the defaults so the old test expectations still
    // hold. Exposed as test-only constants so `store.tick(...)` calls stay
    // readable after the live consts were folded into `Config`.
    const WALK_SPEED_PX_PER_SEC: f32 = config::DEFAULT_WALK_SPEED_PX_PER_SEC;
    const BOB_CYCLE_MS: u64 = config::DEFAULT_BOB_CYCLE_MS;

    fn agent(
        id: &str,
        user: &str,
        agent_type: &str,
        mode: &str,
        started_at: u64,
        finished_at: Option<u64>,
    ) -> AgentView {
        AgentView {
            agent_id: id.into(),
            session_id: None,
            user: user.into(),
            agent_type: agent_type.into(),
            description: String::new(),
            permission_mode: mode.into(),
            started_at,
            finished_at,
        }
    }

    fn world(agents: Vec<AgentView>, now_ms: u64) -> RenderWorld {
        RenderWorld { agents, now_ms }
    }

    #[test]
    fn reconcile_spawns_new_agent() {
        let mut store = SimStore::new();
        let w = world(vec![agent("a1", "alice", "coder", "default", 0, None)], 0);
        store.reconcile(&w);
        let sim = store.anim.get("a1").expect("sim exists");
        assert_eq!(sim.state, SimState::WalkingIn);
        assert!(!sim.path.is_empty(), "path should be non-empty");
        assert_eq!(sim.x, OUTSIDE_X as f32);
        assert_eq!(sim.y, DOOR.y as f32);
    }

    #[test]
    fn reconcile_marks_finished_as_walking_out() {
        let mut store = SimStore::new();
        let a0 = agent("a1", "alice", "coder", "default", 0, None);
        store.reconcile(&world(vec![a0], 0));
        // Fast-forward to seated so we know the seat is allocated.
        let sim_seat = store.anim["a1"].seat;
        assert!(sim_seat.is_some());

        // Now mark finished.
        let finished = agent("a1", "alice", "coder", "default", 0, Some(1_000));
        store.reconcile(&world(vec![finished], 1_000));
        let sim = &store.anim["a1"];
        assert_eq!(sim.state, SimState::WalkingOut);
        assert!(sim.seat.is_none(), "seat released on walk-out");
        assert!(!sim.path.is_empty(), "walk-out path should be non-empty");
        assert!(store.seats.holder(sim_seat.unwrap()).is_none());
    }

    #[test]
    fn tick_advances_position_along_path() {
        let mut store = SimStore::new();
        // Hand-seed — skip reconcile so we control the path exactly.
        let mut sim = SimAnim {
            agent_id: "t".into(),
            session_id: None,
            user: "t".into(),
            permission_mode: "default".into(),
            is_lab: false,
            x: 0.0,
            y: 0.0,
            path: vec![Point::new(110, 0), Point::new(110, 55), Point::new(220, 55)],
            seat: None,
            room: Room::Desk,
            state: SimState::WalkingIn,
            bob_phase: 0.0,
            spawned_at_ms: 0,
            seated_at_ms: None,
            seated_since_ms: None,
            overflow_hash: 0,
            last_footstep_ms: 0,
        };
        sim.path.reserve(0);
        store.anim.insert("t".into(), sim);
        // Path starts at (0,0) with first waypoint at (110,0). Tick slightly under
        // the time needed to reach it so we can verify advancement without pops.
        let almost_a_leg_ms = (109.0 * 1000.0 / WALK_SPEED_PX_PER_SEC) as u64;
        store.tick(
            almost_a_leg_ms,
            almost_a_leg_ms,
            WALK_SPEED_PX_PER_SEC,
            BOB_CYCLE_MS,
        );
        let sim = &store.anim["t"];
        assert!(sim.x > 0.0 && sim.x < 110.0, "x={}", sim.x);
        assert_eq!(sim.y, 0.0);
        assert_eq!(sim.path.len(), 3, "haven't reached first waypoint");
        // Another big tick: should pass the first waypoint.
        store.tick(
            2_000,
            almost_a_leg_ms + 2_000,
            WALK_SPEED_PX_PER_SEC,
            BOB_CYCLE_MS,
        );
        let sim = &store.anim["t"];
        assert!(sim.path.len() < 3, "advanced past first waypoint");
    }

    #[test]
    fn tick_transitions_to_seated_on_path_end() {
        let mut store = SimStore::new();
        let sim = SimAnim {
            agent_id: "t".into(),
            session_id: None,
            user: "t".into(),
            permission_mode: "default".into(),
            is_lab: false,
            x: 0.0,
            y: 0.0,
            path: vec![Point::new(10, 0)],
            seat: None,
            room: Room::Desk,
            state: SimState::WalkingIn,
            bob_phase: 0.0,
            spawned_at_ms: 0,
            seated_at_ms: None,
            seated_since_ms: None,
            overflow_hash: 0,
            last_footstep_ms: 0,
        };
        store.anim.insert("t".into(), sim);
        // 10 px at 55 px/s => ~182 ms. Tick 500 ms to be safe.
        store.tick(500, 500, WALK_SPEED_PX_PER_SEC, BOB_CYCLE_MS);
        let sim = &store.anim["t"];
        assert_eq!(sim.state, SimState::Seated);
        assert_eq!(sim.x, 10.0);
        assert_eq!(sim.y, 0.0);
        assert_eq!(sim.seated_at_ms, Some(500));
        assert_eq!(sim.seated_since_ms, Some(500));
    }

    #[test]
    fn reconcile_clears_seated_since_on_walk_out() {
        let mut store = SimStore::new();
        let a0 = agent("a1", "alice", "coder", "default", 0, None);
        store.reconcile(&world(vec![a0], 0));
        // Manually mark seated (skip tick) so seated_since_ms is populated.
        let sim = store.anim.get_mut("a1").unwrap();
        sim.state = SimState::Seated;
        sim.seated_since_ms = Some(500);
        let finished = agent("a1", "alice", "coder", "default", 0, Some(1_000));
        store.reconcile(&world(vec![finished], 1_000));
        assert_eq!(store.anim["a1"].state, SimState::WalkingOut);
        assert_eq!(store.anim["a1"].seated_since_ms, None);
    }

    #[test]
    fn seat_registry_allocates_and_releases() {
        let mut seats = SeatRegistry::new();
        // Lab has 3 slots.
        let a = seats.allocate(Room::Lab, "a").unwrap();
        let b = seats.allocate(Room::Lab, "b").unwrap();
        let c = seats.allocate(Room::Lab, "c").unwrap();
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
        // Fourth returns None.
        assert!(seats.allocate(Room::Lab, "d").is_none());
        // Release b; next allocate gets b.
        seats.release(b);
        let d = seats.allocate(Room::Lab, "d").unwrap();
        assert_eq!(d, b);
    }

    #[test]
    fn classification_picks_correct_room() {
        let mut store = SimStore::new();
        store.reconcile(&world(
            vec![
                agent("a1", "u", "verifier", "default", 0, None),
                agent("a2", "u", "coder", "plan", 0, None),
                agent("a3", "u", "coder", "default", 0, None),
            ],
            0,
        ));
        assert_eq!(store.anim["a1"].room, Room::Lab);
        assert_eq!(store.anim["a2"].room, Room::Meeting);
        assert_eq!(store.anim["a3"].room, Room::Desk);
        assert!(matches!(store.anim["a1"].seat, Some(SeatId::Lab(_))));
        assert!(matches!(store.anim["a2"].seat, Some(SeatId::Meeting(_))));
        assert!(matches!(store.anim["a3"].seat, Some(SeatId::Desk(_))));
    }

    #[test]
    fn bob_phase_advances_when_seated() {
        let mut store = SimStore::new();
        let sim = SimAnim {
            agent_id: "t".into(),
            session_id: None,
            user: "t".into(),
            permission_mode: "default".into(),
            is_lab: false,
            x: 10.0,
            y: 0.0,
            path: vec![],
            seat: None,
            room: Room::Desk,
            state: SimState::Seated,
            bob_phase: 0.0,
            spawned_at_ms: 0,
            seated_at_ms: Some(0),
            seated_since_ms: Some(0),
            overflow_hash: 0,
            last_footstep_ms: 0,
        };
        store.anim.insert("t".into(), sim);
        store.tick(900, 900, WALK_SPEED_PX_PER_SEC, BOB_CYCLE_MS);
        assert!(store.anim["t"].bob_phase > 0.0);
    }

    #[test]
    fn fifth_desk_agent_with_full_seats_goes_to_queue() {
        // MVP: once all 12 desk seats are full, further agents get a queue spot.
        let mut store = SimStore::new();
        let mut agents = Vec::new();
        for i in 0..13 {
            agents.push(agent(
                &format!("a{i}"),
                "u",
                "coder",
                "default",
                i as u64,
                None,
            ));
        }
        store.reconcile(&world(agents, 0));
        assert_eq!(store.anim.len(), 13);
        assert!(store.anim["a12"].seat.is_none(), "overflow has no seat");
    }
}
