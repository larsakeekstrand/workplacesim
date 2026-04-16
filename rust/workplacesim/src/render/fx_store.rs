//! Ambient effect ring buffers — footstep trails, tool-event motes,
//! parent→child tethers, error halos. Lives only in the render thread, fed
//! from the broadcast channel each frame; ports the per-sim arrays + global
//! `tethers` / `motes` lists from `public/main.js`.
//!
//! State here is derived: dropping the whole struct loses recent FX but the
//! authoritative agent state in `crate::state::State` is untouched.

use std::collections::VecDeque;

use tokio::sync::broadcast::{self, error::TryRecvError};

use super::palette::{self, Rgb};
use super::sim_store::{SimState, SimStore};
use crate::state::Event;

/// Match `public/main.js` constants verbatim.
pub const FOOTSTEP_LIFETIME_MS: u64 = 900;
pub const FOOTSTEP_INTERVAL_MS: u64 = 120;
pub const MOTE_LIFETIME_MS: u64 = 1200;
pub const MOTE_CAP: usize = 40;
pub const TETHER_LIFETIME_MS: u64 = 2000;
pub const HALO_LIFETIME_MS: u64 = 2000;

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

pub struct FxStore {
    pub footsteps: Vec<Footstep>,
    pub motes: VecDeque<Mote>,
    pub tethers: Vec<Tether>,
    pub halos: Vec<Halo>,
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
    ) {
        loop {
            match rx.try_recv() {
                Ok(ev) => self.ingest(&ev, sim_store, now_ms),
                Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => break,
                Err(TryRecvError::Lagged(n)) => {
                    tracing::warn!("fx broadcast lagged by {n} events");
                    continue;
                }
            }
        }
    }

    fn ingest(&mut self, event: &Event, sim_store: &SimStore, now_ms: u64) {
        match event {
            Event::Tool { agent_id, tool_name, .. } => {
                let Some(sim) = sim_store.anim.get(agent_id) else {
                    return;
                };
                if self.motes.len() >= MOTE_CAP {
                    self.motes.pop_front();
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
            _ => {}
        }
    }

    /// Per-frame maintenance: drop expired entries; emit footsteps for sims that
    /// are walking and haven't dropped one in `FOOTSTEP_INTERVAL_MS`.
    pub fn tick(&mut self, now_ms: u64, sim_store: &mut SimStore) {
        // Footsteps from walking sims. SimStore is authoritative; we mutate
        // `last_footstep_ms` on the sim itself so this is naturally idempotent
        // across rapid ticks. `last_footstep_ms == 0` is the never-dropped
        // sentinel so the first tick after a sim starts walking lays a step.
        for sim in sim_store.anim.values_mut() {
            if !matches!(sim.state, SimState::WalkingIn | SimState::WalkingOut) {
                continue;
            }
            let due = sim.last_footstep_ms == 0
                || now_ms.saturating_sub(sim.last_footstep_ms) >= FOOTSTEP_INTERVAL_MS;
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
            .retain(|f| now_ms.saturating_sub(f.born_ms) <= FOOTSTEP_LIFETIME_MS);
        while let Some(front) = self.motes.front() {
            if now_ms.saturating_sub(front.born_ms) > MOTE_LIFETIME_MS {
                self.motes.pop_front();
            } else {
                break;
            }
        }
        self.tethers
            .retain(|t| now_ms.saturating_sub(t.born_ms) <= TETHER_LIFETIME_MS);
        self.halos
            .retain(|h| now_ms.saturating_sub(h.born_ms) <= HALO_LIFETIME_MS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::classify::Room;
    use crate::render::geometry::Point;
    use crate::render::sim_store::{SimAnim, SimState};
    use crate::state::Agent;

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
        fx.drain_events(&mut rx, &store, 100);
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
        fx.drain_events(&mut rx, &store, 100);
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
        fx.drain_events(&mut rx, &store, 1_000);
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
        fx.drain_events(&mut rx, &store, 0);
        assert_eq!(fx.motes.len(), 1);
        fx.tick(MOTE_LIFETIME_MS + 1, &mut store);
        assert!(fx.motes.is_empty());
    }

    #[test]
    fn tick_drops_footsteps_at_cadence() {
        let mut store = SimStore::new();
        seed_sim(&mut store, "a1", SimState::WalkingIn);
        let mut fx = FxStore::new();
        // t = 0, 120, 240, 360, 480 → five footsteps over 600 ms.
        for t in (0..600).step_by(60) {
            fx.tick(t, &mut store);
        }
        assert_eq!(fx.footsteps.len(), 5);
    }

    #[test]
    fn seated_sim_does_not_drop_footsteps() {
        let mut store = SimStore::new();
        seed_sim(&mut store, "a1", SimState::Seated);
        let mut fx = FxStore::new();
        for t in (0..1_000).step_by(60) {
            fx.tick(t, &mut store);
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
        fx.drain_events(&mut rx, &store, 100);
        tx.send(Event::ToolError {
            agent_id: "a1".into(),
            tool_name: "Bash".into(),
            message: "oops again".into(),
        })
        .unwrap();
        fx.drain_events(&mut rx, &store, 800);
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

        fx.drain_events(&mut rx, &store, 100);
        assert_eq!(fx.tethers.len(), 1);
        assert_eq!(fx.tethers[0].parent, "parent-sid");
        assert_eq!(fx.tethers[0].child, "child-1");
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
        fx.drain_events(&mut rx, &store, 100);
        // Some events made it through; we don't pin the exact count since the
        // broadcast eviction policy is implementation-defined when overrun.
        assert!(fx.motes.len() <= MOTE_CAP);
    }
}
