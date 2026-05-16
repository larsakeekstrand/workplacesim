//! Ambient effect ring buffers — footstep trails, tool-event motes,
//! parent→child tethers, error halos. Lives only in the render thread, fed
//! from the broadcast channel each frame; ports the per-sim arrays + global
//! `tethers` / `motes` lists from `public/main.js`.
//!
//! State here is derived: dropping the whole struct loses recent FX but the
//! authoritative agent state in `crate::state::State` is untouched.

use std::collections::VecDeque;

use tokio::sync::broadcast::{self, error::TryRecvError};

use super::geometry::LAB_STATION_XS;
use super::palette::{self, Rgb};
use super::sim_store::{SimState, SimStore};
use crate::state::Event;

// FX lifetimes and caps now live on `Config` (see `crate::config`). Task #2
// of the Ethereal Thimble plan threads them through each call site so the
// config website can tune ambient-effect behaviour live. Each API below takes
// the value it needs as a parameter; the desktop/fb loops snapshot a
// `FxLimits` from `Config` once per frame and pass it down.

/// Snapshot of config-driven limits for one frame. Copied under a single
/// short read-lock by the render loop, then passed down to drain/tick/scene
/// so none of the rendering hot path touches the RwLock directly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxLimits {
    pub footstep_lifetime_ms: u64,
    pub footstep_interval_ms: u64,
    pub mote_lifetime_ms: u64,
    pub mote_cap: usize,
    pub tether_lifetime_ms: u64,
    pub halo_lifetime_ms: u64,
    pub file_tick_ms: u64,
    pub file_tick_cap: usize,
    pub bench_flash_ms: u64,
}

impl FxLimits {
    /// Build a snapshot from a `Config`. Called once per frame under the
    /// config read-lock.
    pub fn from_config(c: &crate::config::Config) -> Self {
        Self {
            footstep_lifetime_ms: c.footstep_lifetime_ms,
            footstep_interval_ms: c.footstep_interval_ms,
            mote_lifetime_ms: c.mote_lifetime_ms,
            mote_cap: c.mote_cap,
            tether_lifetime_ms: c.tether_lifetime_ms,
            halo_lifetime_ms: c.halo_lifetime_ms,
            file_tick_ms: c.file_tick_ms,
            file_tick_cap: c.file_tick_cap,
            bench_flash_ms: c.bench_flash_ms,
        }
    }
}

impl Default for FxLimits {
    fn default() -> Self {
        Self::from_config(&crate::config::Config::default())
    }
}

#[derive(Clone, Debug)]
pub struct Footstep {
    pub agent_id: String,
    pub x: f32,
    pub y: f32,
    pub color: Rgb,
    pub born_ms: u64,
}

#[derive(Clone, Debug)]
pub struct Mote {
    pub agent_id: String,
    pub x: f32,
    pub y: f32,
    pub color: Rgb,
    pub born_ms: u64,
}

#[derive(Clone, Debug)]
pub struct Tether {
    pub parent: String,
    pub child: String,
    pub born_ms: u64,
}

#[derive(Clone, Debug)]
pub struct Halo {
    pub agent_id: String,
    pub born_ms: u64,
}

#[derive(Clone, Debug)]
pub struct FileTick {
    pub path: String,
    pub born_ms: u64,
}

#[derive(Clone, Debug)]
pub struct BenchFlash {
    pub station_idx: usize,
    pub ok: bool,
    pub born_ms: u64,
}

pub struct FxStore {
    pub footsteps: Vec<Footstep>,
    pub motes: VecDeque<Mote>,
    pub tethers: Vec<Tether>,
    pub halos: Vec<Halo>,
    pub file_ticks: VecDeque<FileTick>,
    pub bench_flashes: Vec<BenchFlash>,
}

impl Default for FxStore {
    fn default() -> Self {
        Self::new()
    }
}

impl FxStore {
    pub fn new() -> Self {
        Self {
            footsteps: Vec::new(),
            motes: VecDeque::new(),
            tethers: Vec::new(),
            halos: Vec::new(),
            file_ticks: VecDeque::new(),
            bench_flashes: Vec::new(),
        }
    }

    /// Drain pending events from the broadcast channel and translate them into
    /// FX entries. Lagged subscribers warn-and-continue: state is authoritative
    /// so the next frame resyncs from `sim_store`.
    pub fn drain_events(
        &mut self,
        rx: &mut broadcast::Receiver<Event>,
        sim_store: &SimStore,
        now_ms: u64,
        limits: &FxLimits,
    ) {
        loop {
            match rx.try_recv() {
                Ok(ev) => self.ingest(&ev, sim_store, now_ms, limits),
                Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => break,
                Err(TryRecvError::Lagged(n)) => {
                    tracing::warn!("fx broadcast lagged by {n} events");
                    continue;
                }
            }
        }
    }

    fn ingest(&mut self, event: &Event, sim_store: &SimStore, now_ms: u64, limits: &FxLimits) {
        match event {
            Event::Tool {
                agent_id,
                tool_name,
                ..
            } => {
                let Some(sim) = sim_store.anim.get(agent_id) else {
                    return;
                };
                while self.motes.len() >= limits.mote_cap {
                    if self.motes.pop_front().is_none() {
                        break;
                    }
                }
                // A clamp-bypass mote_cap of 0 is still a valid request;
                // skip the push so we don't insert only to be popped.
                if limits.mote_cap == 0 {
                    return;
                }
                self.motes.push_back(Mote {
                    agent_id: agent_id.clone(),
                    x: sim.x,
                    // JS spawns at sim.y - 18 (world px). We store world coords
                    // and halve at draw time, matching the sim sprite contract.
                    y: sim.y - 18.0,
                    color: palette::mote_color(tool_name),
                    born_ms: now_ms,
                });
            }
            Event::Start { agent } => {
                let Some(parent_sid) = agent.session_id.as_deref() else {
                    return;
                };
                // Self-spawn (main session sim) — no tether to itself.
                if parent_sid == agent.agent_id {
                    return;
                }
                if !sim_store.anim.contains_key(parent_sid) {
                    return;
                }
                self.tethers.push(Tether {
                    parent: parent_sid.to_string(),
                    child: agent.agent_id.clone(),
                    born_ms: now_ms,
                });
            }
            Event::ToolError { agent_id, .. } => {
                if let Some(existing) = self.halos.iter_mut().find(|h| &h.agent_id == agent_id) {
                    existing.born_ms = now_ms;
                } else {
                    self.halos.push(Halo {
                        agent_id: agent_id.clone(),
                        born_ms: now_ms,
                    });
                }
            }
            Event::FileTouch { path, .. } => {
                if path.is_empty() {
                    return;
                }
                while self.file_ticks.len() >= limits.file_tick_cap {
                    if self.file_ticks.pop_front().is_none() {
                        break;
                    }
                }
                if limits.file_tick_cap == 0 {
                    return;
                }
                self.file_ticks.push_back(FileTick {
                    path: path.clone(),
                    born_ms: now_ms,
                });
            }
            Event::BashResult { agent_id, ok } => {
                let Some(sim) = sim_store.anim.get(agent_id) else {
                    return;
                };
                let idx = nearest_station_idx(sim.x);
                if let Some(existing) = self.bench_flashes.iter_mut().find(|b| b.station_idx == idx)
                {
                    existing.ok = *ok;
                    existing.born_ms = now_ms;
                } else {
                    self.bench_flashes.push(BenchFlash {
                        station_idx: idx,
                        ok: *ok,
                        born_ms: now_ms,
                    });
                }
            }
            _ => {}
        }
    }

    /// Per-frame maintenance: drop expired entries; emit footsteps for sims that
    /// are walking and haven't dropped one in `limits.footstep_interval_ms`.
    pub fn tick(&mut self, now_ms: u64, sim_store: &mut SimStore, limits: &FxLimits) {
        // Footsteps from walking sims. SimStore is authoritative; we mutate
        // `last_footstep_ms` on the sim itself so this is naturally idempotent
        // across rapid ticks. `last_footstep_ms == 0` is the never-dropped
        // sentinel so the first tick after a sim starts walking lays a step.
        for sim in sim_store.anim.values_mut() {
            if !matches!(sim.state, SimState::WalkingIn | SimState::WalkingOut) {
                continue;
            }
            let due = sim.last_footstep_ms == 0
                || now_ms.saturating_sub(sim.last_footstep_ms) >= limits.footstep_interval_ms;
            if !due {
                continue;
            }
            sim.last_footstep_ms = now_ms.max(1);
            self.footsteps.push(Footstep {
                agent_id: sim.agent_id.clone(),
                x: sim.x,
                y: sim.y + 10.0,
                color: palette::sim_colors(&sim.user).shirt,
                born_ms: now_ms,
            });
        }

        // Expire by TTL.
        self.footsteps
            .retain(|f| now_ms.saturating_sub(f.born_ms) <= limits.footstep_lifetime_ms);
        while let Some(front) = self.motes.front() {
            if now_ms.saturating_sub(front.born_ms) > limits.mote_lifetime_ms {
                self.motes.pop_front();
            } else {
                break;
            }
        }
        self.tethers
            .retain(|t| now_ms.saturating_sub(t.born_ms) <= limits.tether_lifetime_ms);
        self.halos
            .retain(|h| now_ms.saturating_sub(h.born_ms) <= limits.halo_lifetime_ms);
        while let Some(front) = self.file_ticks.front() {
            if now_ms.saturating_sub(front.born_ms) > limits.file_tick_ms {
                self.file_ticks.pop_front();
            } else {
                break;
            }
        }
        self.bench_flashes
            .retain(|b| now_ms.saturating_sub(b.born_ms) <= limits.bench_flash_ms);
    }
}

/// Pick the lab station (by JS-world x) closest to the given sim x. Sims that
/// emit bash-result from the desk or meeting rooms still land somewhere — we
/// want the nearest lab monitor to their body so the flash reads as "theirs".
pub fn nearest_station_idx(sim_x: f32) -> usize {
    let mut best = 0usize;
    let mut best_d = f32::INFINITY;
    for (i, &sx) in LAB_STATION_XS.iter().enumerate() {
        let d = (sx as f32 - sim_x).abs();
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::render::classify::Room;
    use crate::render::geometry::Point;
    use crate::render::sim_store::{SimAnim, SimState};
    use crate::state::Agent;

    // Test-local aliases so assertions stay readable after the live consts
    // moved into `Config`. Values match `Config::default()`.
    const MOTE_LIFETIME_MS: u64 = config::DEFAULT_MOTE_LIFETIME_MS;
    const MOTE_CAP: usize = config::DEFAULT_MOTE_CAP;
    const FILE_TICK_MS: u64 = config::DEFAULT_FILE_TICK_MS;
    const FILE_TICK_CAP: usize = config::DEFAULT_FILE_TICK_CAP;

    fn limits() -> FxLimits {
        FxLimits::default()
    }

    fn seed_sim(store: &mut SimStore, id: &str, state: SimState) {
        store.anim.insert(
            id.into(),
            SimAnim {
                agent_id: id.into(),
                session_id: None,
                user: id.into(),
                permission_mode: "default".into(),
                is_lab: false,
                x: 100.0,
                y: 100.0,
                path: vec![Point::new(200, 100)],
                seat: None,
                room: Room::Desk,
                state,
                bob_phase: 0.0,
                spawned_at_ms: 0,
                seated_at_ms: None,
                seated_since_ms: None,
                overflow_hash: 0,
                last_footstep_ms: 0,
            },
        );
    }

    fn make_channel() -> (broadcast::Sender<Event>, broadcast::Receiver<Event>) {
        broadcast::channel(8)
    }

    #[test]
    fn drain_tool_event_spawns_mote() {
        let mut store = SimStore::new();
        seed_sim(&mut store, "a1", SimState::Seated);
        let (tx, mut rx) = make_channel();
        let mut fx = FxStore::new();
        tx.send(Event::Tool {
            agent_id: "a1".into(),
            tool_name: "Read".into(),
            ts: 100,
        })
        .unwrap();
        fx.drain_events(&mut rx, &store, 100, &limits());
        assert_eq!(fx.motes.len(), 1);
        let m = &fx.motes[0];
        assert_eq!(m.color, palette::mote_color("Read"));
        assert_eq!(m.x, 100.0);
        assert_eq!(m.y, 100.0 - 18.0);
    }

    #[test]
    fn drain_tool_event_unknown_agent_drops() {
        let store = SimStore::new();
        let (tx, mut rx) = make_channel();
        let mut fx = FxStore::new();
        tx.send(Event::Tool {
            agent_id: "ghost".into(),
            tool_name: "Read".into(),
            ts: 100,
        })
        .unwrap();
        fx.drain_events(&mut rx, &store, 100, &limits());
        assert!(fx.motes.is_empty());
    }

    #[test]
    fn mote_cap_drops_oldest() {
        let mut store = SimStore::new();
        seed_sim(&mut store, "a1", SimState::Seated);
        let (tx, mut rx) = broadcast::channel(64);
        let mut fx = FxStore::new();
        for i in 0..MOTE_CAP + 1 {
            tx.send(Event::Tool {
                agent_id: "a1".into(),
                tool_name: format!("T{i}"),
                ts: i as u64,
            })
            .unwrap();
        }
        fx.drain_events(&mut rx, &store, 1_000, &limits());
        assert_eq!(fx.motes.len(), MOTE_CAP);
    }

    #[test]
    fn tick_expires_motes_past_ttl() {
        let mut store = SimStore::new();
        seed_sim(&mut store, "a1", SimState::Seated);
        let (tx, mut rx) = make_channel();
        let mut fx = FxStore::new();
        tx.send(Event::Tool {
            agent_id: "a1".into(),
            tool_name: "Read".into(),
            ts: 0,
        })
        .unwrap();
        fx.drain_events(&mut rx, &store, 0, &limits());
        assert_eq!(fx.motes.len(), 1);
        fx.tick(MOTE_LIFETIME_MS + 1, &mut store, &limits());
        assert!(fx.motes.is_empty());
    }

    #[test]
    fn tick_drops_footsteps_at_cadence() {
        let mut store = SimStore::new();
        seed_sim(&mut store, "a1", SimState::WalkingIn);
        let mut fx = FxStore::new();
        // t = 0, 120, 240, 360, 480 → five footsteps over 600 ms.
        for t in (0..600).step_by(60) {
            fx.tick(t, &mut store, &limits());
        }
        assert_eq!(fx.footsteps.len(), 5);
    }

    #[test]
    fn seated_sim_does_not_drop_footsteps() {
        let mut store = SimStore::new();
        seed_sim(&mut store, "a1", SimState::Seated);
        let mut fx = FxStore::new();
        for t in (0..1_000).step_by(60) {
            fx.tick(t, &mut store, &limits());
        }
        assert!(fx.footsteps.is_empty());
    }

    #[test]
    fn halo_deduplicates_by_agent_id() {
        let store = SimStore::new();
        let (tx, mut rx) = make_channel();
        let mut fx = FxStore::new();
        tx.send(Event::ToolError {
            agent_id: "a1".into(),
            tool_name: "Bash".into(),
            message: "oops".into(),
        })
        .unwrap();
        fx.drain_events(&mut rx, &store, 100, &limits());
        tx.send(Event::ToolError {
            agent_id: "a1".into(),
            tool_name: "Bash".into(),
            message: "oops again".into(),
        })
        .unwrap();
        fx.drain_events(&mut rx, &store, 800, &limits());
        assert_eq!(fx.halos.len(), 1);
        assert_eq!(fx.halos[0].born_ms, 800);
    }

    #[test]
    fn tether_only_when_parent_exists_in_sim_store() {
        let mut store = SimStore::new();
        seed_sim(&mut store, "parent-sid", SimState::Seated);
        let (tx, mut rx) = make_channel();
        let mut fx = FxStore::new();

        // Subagent whose session_id matches an existing sim.
        tx.send(Event::Start {
            agent: Box::new(Agent {
                agent_id: "child-1".into(),
                session_id: Some("parent-sid".into()),
                ..Default::default()
            }),
        })
        .unwrap();

        // Subagent whose session_id has no live parent.
        tx.send(Event::Start {
            agent: Box::new(Agent {
                agent_id: "child-2".into(),
                session_id: Some("missing-sid".into()),
                ..Default::default()
            }),
        })
        .unwrap();

        // Self-spawn (main session sim) — agent_id == session_id.
        tx.send(Event::Start {
            agent: Box::new(Agent {
                agent_id: "self".into(),
                session_id: Some("self".into()),
                ..Default::default()
            }),
        })
        .unwrap();

        fx.drain_events(&mut rx, &store, 100, &limits());
        assert_eq!(fx.tethers.len(), 1);
        assert_eq!(fx.tethers[0].parent, "parent-sid");
        assert_eq!(fx.tethers[0].child, "child-1");
    }

    #[test]
    fn file_tick_caps_at_three() {
        let store = SimStore::new();
        let (tx, mut rx) = broadcast::channel(16);
        let mut fx = FxStore::new();
        for i in 0..5 {
            tx.send(Event::FileTouch {
                agent_id: "a1".into(),
                path: format!("src/f{i}.rs"),
            })
            .unwrap();
        }
        fx.drain_events(&mut rx, &store, 0, &limits());
        assert_eq!(fx.file_ticks.len(), FILE_TICK_CAP);
        // Oldest dropped; newest retained.
        assert_eq!(fx.file_ticks.front().unwrap().path, "src/f2.rs");
        assert_eq!(fx.file_ticks.back().unwrap().path, "src/f4.rs");
    }

    #[test]
    fn file_tick_expires_at_ttl() {
        let mut store = SimStore::new();
        let (tx, mut rx) = broadcast::channel(4);
        let mut fx = FxStore::new();
        tx.send(Event::FileTouch {
            agent_id: "a1".into(),
            path: "src/x.rs".into(),
        })
        .unwrap();
        fx.drain_events(&mut rx, &store, 0, &limits());
        fx.tick(FILE_TICK_MS, &mut store, &limits());
        assert_eq!(fx.file_ticks.len(), 1);
        fx.tick(FILE_TICK_MS + 1, &mut store, &limits());
        assert!(fx.file_ticks.is_empty());
    }

    #[test]
    fn bench_flash_dedupes_by_station() {
        let mut store = SimStore::new();
        // Sim at lab station 1 (middle) — JS x = LAB_STATION_XS[1].
        let mid_x = LAB_STATION_XS[1] as f32;
        store.anim.insert(
            "a1".into(),
            SimAnim {
                agent_id: "a1".into(),
                session_id: None,
                user: "u".into(),
                permission_mode: "default".into(),
                is_lab: true,
                x: mid_x,
                y: 400.0,
                path: vec![],
                seat: None,
                room: Room::Lab,
                state: SimState::Seated,
                bob_phase: 0.0,
                spawned_at_ms: 0,
                seated_at_ms: Some(0),
                seated_since_ms: Some(0),
                overflow_hash: 0,
                last_footstep_ms: 0,
            },
        );
        let (tx, mut rx) = broadcast::channel(4);
        let mut fx = FxStore::new();
        tx.send(Event::BashResult {
            agent_id: "a1".into(),
            ok: true,
        })
        .unwrap();
        fx.drain_events(&mut rx, &store, 100, &limits());
        assert_eq!(fx.bench_flashes.len(), 1);
        // Same station, different outcome: should refresh, not stack.
        tx.send(Event::BashResult {
            agent_id: "a1".into(),
            ok: false,
        })
        .unwrap();
        fx.drain_events(&mut rx, &store, 400, &limits());
        assert_eq!(fx.bench_flashes.len(), 1);
        assert!(!fx.bench_flashes[0].ok);
        assert_eq!(fx.bench_flashes[0].born_ms, 400);
    }

    #[test]
    fn bench_flash_nearest_station() {
        // Left sim → station 0; right sim → station 2.
        assert_eq!(nearest_station_idx(LAB_STATION_XS[0] as f32 - 5.0), 0);
        assert_eq!(nearest_station_idx(LAB_STATION_XS[2] as f32 + 5.0), 2);
        assert_eq!(nearest_station_idx(LAB_STATION_XS[1] as f32), 1);
    }

    #[test]
    fn drain_handles_lagged_receiver() {
        let mut store = SimStore::new();
        seed_sim(&mut store, "a1", SimState::Seated);
        let (tx, mut rx) = broadcast::channel::<Event>(2);
        let mut fx = FxStore::new();
        // Saturate the channel beyond capacity so the next try_recv reports Lagged.
        for i in 0..6 {
            tx.send(Event::Tool {
                agent_id: "a1".into(),
                tool_name: format!("T{i}"),
                ts: i as u64,
            })
            .unwrap();
        }
        // Should not panic; the lag warning is logged and the surviving events
        // are drained.
        fx.drain_events(&mut rx, &store, 100, &limits());
        // Some events made it through; we don't pin the exact count since the
        // broadcast eviction policy is implementation-defined when overrun.
        assert!(fx.motes.len() <= MOTE_CAP);
    }
}
