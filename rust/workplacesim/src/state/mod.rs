//! In-memory state machine. Ports `server/state.js` event-for-event.
//!
//! The JS module mutated module-level globals; here we own everything in
//! `State` behind `Arc<parking_lot::RwLock<_>>`. The render thread runs in a
//! sync context (minifb's event loop is blocking), so a tokio RwLock would
//! force us to carry a runtime into the renderer — parking_lot keeps locking
//! cheap and sync on both the HTTP and render sides. Broadcast is still a
//! tokio `broadcast` channel: step 2's SSE route will subscribe and re-encode
//! each `Event` as `data: <json>\n\n`.
//!
//! Time is injected: every method that needs "now" takes `now_ms: u64`.
//! Tests pass deterministic values; production callers use `clock::now_ms()`.

use std::sync::Arc;
use std::time::Duration;

use indexmap::IndexMap;
use parking_lot::RwLock;
use tokio::sync::broadcast;

pub mod agent;
pub mod clock;
pub mod events;
pub mod payloads;
pub mod pending;

pub use agent::{Agent, CurrentError, Visit};
pub use events::Event;
pub use payloads::{
    BufferDescription, Lifecycle, Pretool, PretoolToolInput, StartAgent, StopAgent, ToolEvent,
    VisitRoom,
};

use pending::PendingDescription;

const PENDING_TTL: Duration = Duration::from_secs(60);
const STOP_GRACE: Duration = Duration::from_secs(10);
const VISIT_MIN: Duration = Duration::from_secs(1);
const VISIT_MAX: Duration = Duration::from_secs(120);
const VISIT_DEFAULT: Duration = Duration::from_secs(20);
const ERROR_PREVIEW_LEN: usize = 80;
const PROMPT_PREVIEW_LEN: usize = 80;

const VALID_ROOMS: &[&str] = &["test", "meeting", "desk"];

/// Capacity for the broadcast channel. Subscribers that lag by more than this
/// many messages see `RecvError::Lagged` and can resync via a snapshot event.
const BROADCAST_CAP: usize = 256;

pub struct State {
    // IndexMap preserves insertion order; `stop_agent`'s FIFO fallback needs it
    // to match the JS Map iteration order (oldest unfinished record wins).
    active_agents: IndexMap<String, Agent>,

    // Keyed by (session_id, agent_type). Mirror the JS `${sid ?? ""}::${type ?? ""}`
    // key shape with an Option tuple so None and "" stay distinct.
    pending_descriptions: IndexMap<(Option<String>, Option<String>), PendingDescription>,

    // agent_id → deadline (ms). `tick()` removes from `active_agents` once past.
    pending_stops: IndexMap<String, u64>,

    tx: broadcast::Sender<Event>,
}

/// Construct a fresh State wrapped in Arc<RwLock>, plus a subscribed receiver
/// so the caller doesn't race with the first emitted event.
pub fn new_state() -> (Arc<RwLock<State>>, broadcast::Receiver<Event>) {
    let (tx, rx) = broadcast::channel(BROADCAST_CAP);
    let state = State {
        active_agents: IndexMap::new(),
        pending_descriptions: IndexMap::new(),
        pending_stops: IndexMap::new(),
        tx,
    };
    (Arc::new(RwLock::new(state)), rx)
}

impl State {
    /// Subscribe to the broadcast channel. New subscribers may want to pair
    /// this with `snapshot_event()` to get the current active set.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    fn emit(&self, event: Event) {
        // broadcast::Sender::send errors only when there are zero active
        // receivers. That's fine — events are fire-and-forget.
        let _ = self.tx.send(event);
    }

    pub fn list_active(&self) -> Vec<Agent> {
        self.active_agents
            .values()
            .filter(|a| a.finished_at.is_none())
            .cloned()
            .collect()
    }

    /// Every record in the map, including ones with `finished_at.is_some()`
    /// (still within the STOP_GRACE window). Used by the renderer so a sim can
    /// animate its walk-out after SubagentStop/SessionEnd.
    pub fn list_all_including_finished(&self) -> Vec<Agent> {
        self.active_agents.values().cloned().collect()
    }

    pub fn snapshot_event(&self) -> Event {
        Event::Snapshot {
            agents: self.list_active(),
        }
    }

    pub fn buffer_description(&mut self, p: BufferDescription, now_ms: u64) {
        if p.subagent_type.is_none() {
            return;
        }
        self.pending_descriptions.insert(
            (p.session_id.clone(), p.subagent_type.clone()),
            PendingDescription {
                description: p.description.unwrap_or_default(),
                tool_use_id: p.tool_use_id,
                ts: now_ms,
            },
        );
    }

    /// Pull the pending description for (session_id, agent_type) out of the
    /// buffer. Matches JS: the entry is deleted even if expired, but returns
    /// empty string in that case.
    fn consume_description(
        &mut self,
        session_id: &Option<String>,
        agent_type: &Option<String>,
        now_ms: u64,
    ) -> String {
        let key = (session_id.clone(), agent_type.clone());
        let Some(entry) = self.pending_descriptions.shift_remove(&key) else {
            return String::new();
        };
        if now_ms.saturating_sub(entry.ts) > PENDING_TTL.as_millis() as u64 {
            return String::new();
        }
        entry.description
    }

    pub fn start_agent(&mut self, p: StartAgent, now_ms: u64) -> Option<Agent> {
        if p.agent_id.is_empty() {
            return None;
        }
        if let Some(existing) = self.active_agents.get(&p.agent_id) {
            return Some(existing.clone());
        }

        let description = if let Some(d) = p.description.as_deref().filter(|s| !s.is_empty()) {
            d.to_string()
        } else {
            let from_pending =
                self.consume_description(&p.session_id, &p.agent_type, now_ms);
            if !from_pending.is_empty() {
                from_pending
            } else if let Some(t) = p.agent_type.as_deref().filter(|s| !s.is_empty()) {
                t.to_string()
            } else {
                "agent".to_string()
            }
        };

        let record = Agent {
            agent_id: p.agent_id.clone(),
            session_id: p.session_id,
            agent_type: p.agent_type.filter(|s| !s.is_empty()).unwrap_or_else(|| "agent".to_string()),
            description,
            user: p.user.filter(|s| !s.is_empty()).unwrap_or_else(|| "unknown".to_string()),
            host: p.host.unwrap_or_default(),
            cwd: p.cwd.unwrap_or_default(),
            permission_mode: p
                .permission_mode
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "default".to_string()),
            started_at: now_ms,
            finished_at: None,
            last_message: None,
            visit: None,
            session_prompt: None,
            idle: None,
            current_error: None,
        };
        self.active_agents.insert(p.agent_id, record.clone());
        self.emit(Event::Start {
            agent: Box::new(record.clone()),
        });
        Some(record)
    }

    pub fn stop_agent(&mut self, p: StopAgent, now_ms: u64) -> Option<Agent> {
        // Direct lookup first; falls back to FIFO on (session_id, agent_type)
        // because SubagentStop's agent_id differs from PreToolUse(Agent)'s
        // tool_use_id that seeded the record.
        let mut target_id: Option<String> = None;

        if let Some(aid) = p.agent_id.as_deref() {
            if self.active_agents.contains_key(aid) {
                target_id = Some(aid.to_string());
            }
        }

        if target_id.is_none() {
            if let (Some(sid), Some(atype)) = (p.session_id.as_deref(), p.agent_type.as_deref()) {
                for (id, rec) in self.active_agents.iter() {
                    if rec.finished_at.is_some() {
                        continue;
                    }
                    if rec.session_id.as_deref() == Some(sid) && rec.agent_type == atype {
                        target_id = Some(id.clone());
                        break;
                    }
                }
            }
        }

        let id = target_id?;
        let record = self.active_agents.get_mut(&id)?;
        record.finished_at = Some(now_ms);
        record.last_message = p.last_assistant_message;
        let snapshot = record.clone();

        self.emit(Event::Stop {
            agent_id: id.clone(),
        });
        self.pending_stops
            .insert(id, now_ms + STOP_GRACE.as_millis() as u64);
        Some(snapshot)
    }

    /// Resolve a record by the same rules as JS `findRecord`. Rejects finished
    /// records. Returns the resolved agent_id alongside so callers don't have
    /// to borrow twice.
    fn find_record_id(
        &self,
        agent_id: &Option<String>,
        session_id: &Option<String>,
    ) -> Option<String> {
        if let Some(aid) = agent_id.as_deref() {
            if let Some(r) = self.active_agents.get(aid) {
                if r.finished_at.is_none() {
                    return Some(aid.to_string());
                }
                return None;
            }
        }

        if let Some(sid) = session_id.as_deref() {
            // Main session sim has agent_id == session_id.
            if let Some(r) = self.active_agents.get(sid) {
                if r.finished_at.is_none() {
                    return Some(sid.to_string());
                }
            }

            // Fallback scan: first unfinished record whose session_id matches.
            for (id, rec) in self.active_agents.iter() {
                if rec.finished_at.is_some() {
                    continue;
                }
                if rec.session_id.as_deref() == Some(sid) {
                    return Some(id.clone());
                }
            }
        }

        None
    }

    fn check_permission_mode(&mut self, agent_id: &str, incoming: &Option<String>) {
        let Some(mode) = incoming.as_deref().filter(|s| !s.is_empty()) else {
            return;
        };
        let Some(record) = self.active_agents.get_mut(agent_id) else {
            return;
        };
        if record.permission_mode == mode {
            return;
        }
        record.permission_mode = mode.to_string();
        self.emit(Event::Reclassify {
            agent_id: agent_id.to_string(),
            permission_mode: mode.to_string(),
        });
    }

    pub fn tool_event(&mut self, p: ToolEvent, now_ms: u64) {
        let id = p.agent_id.clone().or_else(|| p.session_id.clone());
        let Some(id) = id.filter(|s| !s.is_empty()) else {
            return;
        };
        let Some(tool_name) = p.tool_name.filter(|s| !s.is_empty()) else {
            return;
        };
        if !self.active_agents.contains_key(&id) {
            eprintln!(
                "tool_event: no record for agent_id={:?} session_id={:?} tool_name={}",
                p.agent_id, p.session_id, tool_name
            );
            return;
        }
        self.check_permission_mode(&id, &p.permission_mode);
        self.emit(Event::Tool {
            agent_id: id,
            tool_name,
            ts: now_ms,
        });
    }

    pub fn visit_room(&mut self, p: VisitRoom, now_ms: u64) -> Option<()> {
        let room = p.room.as_deref()?;
        if !VALID_ROOMS.contains(&room) {
            return None;
        }
        let room = room.to_string();

        let ttl_ms = match p.ttl_ms {
            Some(v) if v > 0 => v,
            _ => VISIT_DEFAULT.as_millis() as u64,
        };
        let ttl_ms = ttl_ms
            .max(VISIT_MIN.as_millis() as u64)
            .min(VISIT_MAX.as_millis() as u64);

        let id = self.find_record_id(&p.agent_id, &p.session_id)?;
        self.check_permission_mode(&id, &p.permission_mode);

        let record = self.active_agents.get_mut(&id)?;
        let prior_until = record.visit.as_ref().map(|v| v.until).unwrap_or(0);
        let until = prior_until.max(now_ms + ttl_ms);
        record.visit = Some(Visit {
            room: room.clone(),
            until,
        });
        self.emit(Event::Visit {
            agent_id: id,
            room: Some(room),
            until: Some(until),
        });
        Some(())
    }

    pub fn handle_lifecycle(&mut self, p: Lifecycle, now_ms: u64) -> Option<()> {
        let kind = p.kind.as_deref()?;
        let id = self.find_record_id(&p.agent_id, &p.session_id)?;
        self.check_permission_mode(&id, &p.permission_mode);

        match kind {
            "prompt" => {
                let text: String = p
                    .text
                    .unwrap_or_default()
                    .chars()
                    .take(PROMPT_PREVIEW_LEN)
                    .collect();
                let was_idle = {
                    let record = self.active_agents.get_mut(&id)?;
                    record.session_prompt = Some(text.clone());
                    let was_idle = record.idle == Some(true);
                    if was_idle {
                        record.idle = Some(false);
                    }
                    was_idle
                };
                if was_idle {
                    self.emit(Event::Idle {
                        agent_id: id.clone(),
                        idle: false,
                    });
                }
                self.emit(Event::Prompt { agent_id: id, text });
                Some(())
            }
            "idle" => {
                let emit = {
                    let record = self.active_agents.get_mut(&id)?;
                    if record.idle == Some(true) {
                        false
                    } else {
                        record.idle = Some(true);
                        true
                    }
                };
                if emit {
                    self.emit(Event::Idle {
                        agent_id: id,
                        idle: true,
                    });
                }
                Some(())
            }
            "turn-end" => {
                self.emit(Event::TurnEnd { agent_id: id });
                Some(())
            }
            "file-touch" => {
                let path = p.path?;
                if path.is_empty() {
                    return Some(());
                }
                self.emit(Event::FileTouch { agent_id: id, path });
                Some(())
            }
            "bash-result" => {
                self.emit(Event::BashResult {
                    agent_id: id,
                    ok: p.ok.unwrap_or(false),
                });
                Some(())
            }
            "tool-error" => {
                let message: String = p
                    .message
                    .unwrap_or_default()
                    .chars()
                    .take(ERROR_PREVIEW_LEN)
                    .collect();
                let tool_name = p.tool_name.unwrap_or_default();
                {
                    let record = self.active_agents.get_mut(&id)?;
                    record.current_error = Some(CurrentError {
                        tool_name: tool_name.clone(),
                        message: message.clone(),
                        ts: now_ms,
                    });
                }
                self.emit(Event::ToolError {
                    agent_id: id,
                    tool_name,
                    message,
                });
                Some(())
            }
            _ => None,
        }
    }

    /// Drive time-based state transitions. The render loop will call this
    /// every frame; tests call it explicitly with a chosen `now_ms`.
    ///
    /// Three jobs:
    /// 1. expire visits (emit `{type:visit, room: null}` when `until` has passed),
    /// 2. sweep pending_descriptions older than PENDING_TTL,
    /// 3. finalize pending_stops past STOP_GRACE (remove from active_agents).
    pub fn tick(&mut self, now_ms: u64) {
        // 1. Visit expiry.
        let expired_visits: Vec<String> = self
            .active_agents
            .iter()
            .filter_map(|(id, rec)| match rec.visit.as_ref() {
                Some(v) if now_ms > v.until => Some(id.clone()),
                _ => None,
            })
            .collect();
        for id in expired_visits {
            if let Some(rec) = self.active_agents.get_mut(&id) {
                rec.visit = None;
            }
            self.emit(Event::Visit {
                agent_id: id,
                room: None,
                until: None,
            });
        }

        // 2. Pending-description sweep.
        let pending_cutoff = now_ms.saturating_sub(PENDING_TTL.as_millis() as u64);
        self.pending_descriptions
            .retain(|_, entry| entry.ts >= pending_cutoff);

        // 3. Finalise stops past grace window.
        let finalised: Vec<String> = self
            .pending_stops
            .iter()
            .filter_map(|(id, deadline)| if now_ms >= *deadline { Some(id.clone()) } else { None })
            .collect();
        for id in finalised {
            self.pending_stops.shift_remove(&id);
            self.active_agents.shift_remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast::error::TryRecvError;

    fn setup() -> (State, broadcast::Receiver<Event>) {
        let (tx, rx) = broadcast::channel(BROADCAST_CAP);
        let state = State {
            active_agents: IndexMap::new(),
            pending_descriptions: IndexMap::new(),
            pending_stops: IndexMap::new(),
            tx,
        };
        (state, rx)
    }

    fn drain(rx: &mut broadcast::Receiver<Event>) -> Vec<Event> {
        let mut out = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(ev) => out.push(ev),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Closed) => break,
                Err(TryRecvError::Lagged(_)) => continue,
            }
        }
        out
    }

    fn start(agent_id: &str, session_id: Option<&str>, agent_type: Option<&str>) -> StartAgent {
        StartAgent {
            agent_id: agent_id.to_string(),
            session_id: session_id.map(|s| s.to_string()),
            agent_type: agent_type.map(|s| s.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn start_agent_inserts_and_emits_start() {
        let (mut s, mut rx) = setup();
        let a = s.start_agent(start("a1", Some("sess"), Some("coder")), 1_000);
        assert!(a.is_some());
        assert_eq!(s.list_active().len(), 1);
        let events = drain(&mut rx);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::Start { agent } => {
                assert_eq!(agent.agent_id, "a1");
                assert_eq!(agent.agent_type, "coder");
                assert_eq!(agent.description, "coder");
                assert_eq!(agent.user, "unknown");
                assert_eq!(agent.permission_mode, "default");
                assert_eq!(agent.started_at, 1_000);
            }
            other => panic!("expected Start, got {other:?}"),
        }
    }

    #[test]
    fn start_agent_idempotent_by_agent_id() {
        let (mut s, mut rx) = setup();
        s.start_agent(start("a1", Some("sess"), Some("coder")), 1_000);
        let _ = drain(&mut rx);
        let again = s.start_agent(start("a1", Some("sess"), Some("other")), 2_000);
        assert_eq!(s.list_active().len(), 1);
        assert!(again.is_some());
        // Still the original, not overwritten.
        assert_eq!(again.unwrap().agent_type, "coder");
        assert!(drain(&mut rx).is_empty(), "no second event");
    }

    #[test]
    fn start_agent_description_fallback_uses_pending() {
        let (mut s, mut rx) = setup();
        s.buffer_description(
            BufferDescription {
                session_id: Some("sess".into()),
                subagent_type: Some("verifier".into()),
                description: Some("run golden-frame tests".into()),
                tool_use_id: Some("tu-1".into()),
            },
            1_000,
        );
        s.start_agent(
            StartAgent {
                agent_id: "tu-1".into(),
                session_id: Some("sess".into()),
                agent_type: Some("verifier".into()),
                description: None,
                ..Default::default()
            },
            1_500,
        );
        let events = drain(&mut rx);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::Start { agent } => {
                assert_eq!(agent.description, "run golden-frame tests");
            }
            other => panic!("expected Start, got {other:?}"),
        }
    }

    #[test]
    fn start_agent_description_fallback_expired_pending_consumed_but_empty() {
        let (mut s, mut rx) = setup();
        s.buffer_description(
            BufferDescription {
                session_id: Some("sess".into()),
                subagent_type: Some("verifier".into()),
                description: Some("stale description".into()),
                tool_use_id: None,
            },
            0,
        );
        // Start past PENDING_TTL; entry should be consumed but return empty.
        let start_ms = PENDING_TTL.as_millis() as u64 + 1_000;
        s.start_agent(
            StartAgent {
                agent_id: "a1".into(),
                session_id: Some("sess".into()),
                agent_type: Some("verifier".into()),
                ..Default::default()
            },
            start_ms,
        );
        let events = drain(&mut rx);
        match &events[0] {
            Event::Start { agent } => {
                // Falls through to agent_type.
                assert_eq!(agent.description, "verifier");
            }
            other => panic!("expected Start, got {other:?}"),
        }
        // And the pending entry is gone.
        assert!(s.pending_descriptions.is_empty());
    }

    #[test]
    fn stop_agent_direct_by_agent_id() {
        let (mut s, mut rx) = setup();
        s.start_agent(start("a1", Some("sess"), Some("coder")), 1_000);
        let _ = drain(&mut rx);
        let stopped = s.stop_agent(
            StopAgent {
                agent_id: Some("a1".into()),
                last_assistant_message: Some("done".into()),
                ..Default::default()
            },
            2_000,
        );
        assert!(stopped.is_some());
        let stopped = stopped.unwrap();
        assert_eq!(stopped.finished_at, Some(2_000));
        assert_eq!(stopped.last_message.as_deref(), Some("done"));
        // list_active excludes finished.
        assert_eq!(s.list_active().len(), 0);
        // Record still present (until tick).
        assert!(s.active_agents.contains_key("a1"));
        // Stop recorded with deadline.
        assert_eq!(
            s.pending_stops.get("a1").copied(),
            Some(2_000 + STOP_GRACE.as_millis() as u64)
        );
        let events = drain(&mut rx);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], Event::Stop { agent_id } if agent_id == "a1"));
    }

    #[test]
    fn stop_agent_fifo_fallback() {
        let (mut s, mut rx) = setup();
        // Two subagents sharing (session_id, agent_type). agent_id is the
        // tool_use_id from PreToolUse(Agent); SubagentStop will arrive with a
        // different agent_id, so FIFO on (session, type) picks the oldest.
        s.start_agent(start("tu-1", Some("sess"), Some("verifier")), 1_000);
        s.start_agent(start("tu-2", Some("sess"), Some("verifier")), 1_010);
        let _ = drain(&mut rx);
        let stopped = s.stop_agent(
            StopAgent {
                agent_id: Some("different-id-from-stop".into()),
                session_id: Some("sess".into()),
                agent_type: Some("verifier".into()),
                last_assistant_message: None,
            },
            2_000,
        );
        assert!(stopped.is_some());
        assert_eq!(stopped.as_ref().unwrap().agent_id, "tu-1");
        let events = drain(&mut rx);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], Event::Stop { agent_id } if agent_id == "tu-1"));
        // tu-2 still active.
        assert!(s
            .active_agents
            .get("tu-2")
            .map(|r| r.finished_at.is_none())
            .unwrap_or(false));
    }

    #[test]
    fn stop_agent_miss() {
        let (mut s, mut rx) = setup();
        let r = s.stop_agent(
            StopAgent {
                agent_id: Some("unknown".into()),
                ..Default::default()
            },
            1_000,
        );
        assert!(r.is_none());
        assert!(drain(&mut rx).is_empty());
    }

    #[test]
    fn tick_finalizes_pending_stops_after_grace() {
        let (mut s, mut rx) = setup();
        s.start_agent(start("a1", Some("sess"), Some("coder")), 0);
        s.stop_agent(
            StopAgent {
                agent_id: Some("a1".into()),
                ..Default::default()
            },
            0,
        );
        let _ = drain(&mut rx);
        let grace_ms = STOP_GRACE.as_millis() as u64;

        s.tick(grace_ms - 1);
        assert!(s.active_agents.contains_key("a1"), "still present before grace");
        s.tick(grace_ms + 1);
        assert!(!s.active_agents.contains_key("a1"), "removed after grace");
        assert!(!s.pending_stops.contains_key("a1"));
    }

    #[test]
    fn visit_room_accepts_valid_rooms() {
        let (mut s, mut rx) = setup();
        s.start_agent(start("a1", Some("sess"), Some("coder")), 0);
        let _ = drain(&mut rx);
        for room in ["test", "meeting", "desk"] {
            let r = s.visit_room(
                VisitRoom {
                    agent_id: Some("a1".into()),
                    room: Some(room.into()),
                    ttl_ms: Some(5_000),
                    ..Default::default()
                },
                1_000,
            );
            assert!(r.is_some(), "room {room} should be accepted");
        }
        let events = drain(&mut rx);
        assert_eq!(events.len(), 3);
        for ev in &events {
            assert!(matches!(ev, Event::Visit { room: Some(_), .. }));
        }
    }

    #[test]
    fn visit_room_rejects_unknown_room() {
        let (mut s, mut rx) = setup();
        s.start_agent(start("a1", Some("sess"), Some("coder")), 0);
        let _ = drain(&mut rx);
        let r = s.visit_room(
            VisitRoom {
                agent_id: Some("a1".into()),
                room: Some("kitchen".into()),
                ttl_ms: Some(5_000),
                ..Default::default()
            },
            1_000,
        );
        assert!(r.is_none());
        assert!(drain(&mut rx).is_empty());
    }

    #[test]
    fn visit_room_clamps_ttl() {
        let (mut s, mut rx) = setup();
        s.start_agent(start("a1", Some("sess"), Some("coder")), 0);
        let _ = drain(&mut rx);

        s.visit_room(
            VisitRoom {
                agent_id: Some("a1".into()),
                room: Some("test".into()),
                ttl_ms: Some(100),
                ..Default::default()
            },
            1_000,
        );
        {
            let visit = s.active_agents["a1"].visit.clone().unwrap();
            assert_eq!(visit.until, 1_000 + VISIT_MIN.as_millis() as u64);
        }

        // Clear then retry with huge ttl.
        s.active_agents.get_mut("a1").unwrap().visit = None;
        s.visit_room(
            VisitRoom {
                agent_id: Some("a1".into()),
                room: Some("test".into()),
                ttl_ms: Some(999_999_999),
                ..Default::default()
            },
            2_000,
        );
        {
            let visit = s.active_agents["a1"].visit.clone().unwrap();
            assert_eq!(visit.until, 2_000 + VISIT_MAX.as_millis() as u64);
        }

        // Missing ttl → default 20s.
        s.active_agents.get_mut("a1").unwrap().visit = None;
        s.visit_room(
            VisitRoom {
                agent_id: Some("a1".into()),
                room: Some("test".into()),
                ttl_ms: None,
                ..Default::default()
            },
            3_000,
        );
        {
            let visit = s.active_agents["a1"].visit.clone().unwrap();
            assert_eq!(visit.until, 3_000 + VISIT_DEFAULT.as_millis() as u64);
        }

        // Drain events; presence already asserted above.
        let _ = drain(&mut rx);
    }

    #[test]
    fn visit_room_extends_existing_until() {
        let (mut s, mut rx) = setup();
        s.start_agent(start("a1", Some("sess"), Some("coder")), 0);
        let _ = drain(&mut rx);

        // First visit → until = 1_000 + 60_000 = 61_000.
        s.visit_room(
            VisitRoom {
                agent_id: Some("a1".into()),
                room: Some("test".into()),
                ttl_ms: Some(60_000),
                ..Default::default()
            },
            1_000,
        );
        assert_eq!(s.active_agents["a1"].visit.as_ref().unwrap().until, 61_000);

        // Second visit with earlier until → does NOT shorten.
        s.visit_room(
            VisitRoom {
                agent_id: Some("a1".into()),
                room: Some("meeting".into()),
                ttl_ms: Some(5_000),
                ..Default::default()
            },
            2_000,
        );
        assert_eq!(s.active_agents["a1"].visit.as_ref().unwrap().until, 61_000);

        // Third visit extends.
        s.visit_room(
            VisitRoom {
                agent_id: Some("a1".into()),
                room: Some("desk".into()),
                ttl_ms: Some(120_000),
                ..Default::default()
            },
            5_000,
        );
        assert_eq!(s.active_agents["a1"].visit.as_ref().unwrap().until, 125_000);
        let _ = drain(&mut rx);
    }

    #[test]
    fn tick_expires_visit_and_emits_null() {
        let (mut s, mut rx) = setup();
        s.start_agent(start("a1", Some("sess"), Some("coder")), 0);
        // Set visit manually to control `until` precisely.
        s.active_agents.get_mut("a1").unwrap().visit = Some(Visit {
            room: "test".into(),
            until: 500,
        });
        let _ = drain(&mut rx);
        s.tick(400);
        assert!(s.active_agents["a1"].visit.is_some());
        assert!(drain(&mut rx).is_empty());
        s.tick(600);
        assert!(s.active_agents["a1"].visit.is_none());
        let events = drain(&mut rx);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], Event::Visit { room: None, until: None, agent_id } if agent_id == "a1"));
    }

    #[test]
    fn permission_mode_change_emits_reclassify() {
        let (mut s, mut rx) = setup();
        s.start_agent(start("a1", Some("sess"), Some("coder")), 0);
        let _ = drain(&mut rx);
        s.tool_event(
            ToolEvent {
                agent_id: Some("a1".into()),
                tool_name: Some("Read".into()),
                permission_mode: Some("plan".into()),
                ..Default::default()
            },
            1_000,
        );
        assert_eq!(s.active_agents["a1"].permission_mode, "plan");
        let events = drain(&mut rx);
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], Event::Reclassify { permission_mode, .. } if permission_mode == "plan"));
        assert!(matches!(&events[1], Event::Tool { .. }));
    }

    #[test]
    fn permission_mode_same_no_event() {
        let (mut s, mut rx) = setup();
        s.start_agent(
            StartAgent {
                agent_id: "a1".into(),
                session_id: Some("sess".into()),
                agent_type: Some("coder".into()),
                permission_mode: Some("plan".into()),
                ..Default::default()
            },
            0,
        );
        let _ = drain(&mut rx);
        s.tool_event(
            ToolEvent {
                agent_id: Some("a1".into()),
                tool_name: Some("Read".into()),
                permission_mode: Some("plan".into()),
                ..Default::default()
            },
            1_000,
        );
        let events = drain(&mut rx);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], Event::Tool { .. }));
    }

    #[test]
    fn lifecycle_prompt_truncates_and_clears_idle() {
        let (mut s, mut rx) = setup();
        s.start_agent(start("a1", Some("sess"), Some("coder")), 0);
        s.handle_lifecycle(
            Lifecycle {
                kind: Some("idle".into()),
                agent_id: Some("a1".into()),
                ..Default::default()
            },
            1_000,
        );
        let _ = drain(&mut rx);

        let long_text: String = "x".repeat(200);
        s.handle_lifecycle(
            Lifecycle {
                kind: Some("prompt".into()),
                agent_id: Some("a1".into()),
                text: Some(long_text),
                ..Default::default()
            },
            2_000,
        );
        let events = drain(&mut rx);
        assert_eq!(events.len(), 2);
        match &events[0] {
            Event::Idle { idle: false, agent_id } => assert_eq!(agent_id, "a1"),
            other => panic!("expected Idle(false), got {other:?}"),
        }
        match &events[1] {
            Event::Prompt { text, agent_id } => {
                assert_eq!(text.len(), 80);
                assert_eq!(agent_id, "a1");
            }
            other => panic!("expected Prompt, got {other:?}"),
        }
        assert_eq!(s.active_agents["a1"].idle, Some(false));
        assert_eq!(s.active_agents["a1"].session_prompt.as_ref().unwrap().len(), 80);
    }

    #[test]
    fn lifecycle_idle_idempotent() {
        let (mut s, mut rx) = setup();
        s.start_agent(start("a1", Some("sess"), Some("coder")), 0);
        let _ = drain(&mut rx);
        s.handle_lifecycle(
            Lifecycle {
                kind: Some("idle".into()),
                agent_id: Some("a1".into()),
                ..Default::default()
            },
            1_000,
        );
        s.handle_lifecycle(
            Lifecycle {
                kind: Some("idle".into()),
                agent_id: Some("a1".into()),
                ..Default::default()
            },
            2_000,
        );
        let events = drain(&mut rx);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], Event::Idle { idle: true, .. }));
    }

    #[test]
    fn lifecycle_turn_end_finds_via_session_id() {
        let (mut s, mut rx) = setup();
        // Main session sim: agent_id == session_id.
        s.start_agent(
            StartAgent {
                agent_id: "sess-1".into(),
                session_id: Some("sess-1".into()),
                agent_type: Some("claude".into()),
                ..Default::default()
            },
            0,
        );
        let _ = drain(&mut rx);
        s.handle_lifecycle(
            Lifecycle {
                kind: Some("turn-end".into()),
                agent_id: None,
                session_id: Some("sess-1".into()),
                ..Default::default()
            },
            1_000,
        );
        let events = drain(&mut rx);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], Event::TurnEnd { agent_id } if agent_id == "sess-1"));
    }

    #[test]
    fn lifecycle_tool_error_stores_current_error_and_emits() {
        let (mut s, mut rx) = setup();
        s.start_agent(start("a1", Some("sess"), Some("coder")), 0);
        let _ = drain(&mut rx);
        let long = "e".repeat(200);
        s.handle_lifecycle(
            Lifecycle {
                kind: Some("tool-error".into()),
                agent_id: Some("a1".into()),
                tool_name: Some("Bash".into()),
                message: Some(long),
                ..Default::default()
            },
            5_000,
        );
        let stored = s.active_agents["a1"].current_error.clone().unwrap();
        assert_eq!(stored.tool_name, "Bash");
        assert_eq!(stored.message.len(), 80);
        assert_eq!(stored.ts, 5_000);
        let events = drain(&mut rx);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::ToolError { message, tool_name, .. } => {
                assert_eq!(message.len(), 80);
                assert_eq!(tool_name, "Bash");
            }
            other => panic!("expected ToolError, got {other:?}"),
        }
    }

    #[test]
    fn list_active_excludes_finished() {
        let (mut s, mut rx) = setup();
        s.start_agent(start("a1", Some("sess"), Some("coder")), 0);
        assert_eq!(s.list_active().len(), 1);
        s.stop_agent(
            StopAgent {
                agent_id: Some("a1".into()),
                ..Default::default()
            },
            1_000,
        );
        assert_eq!(s.list_active().len(), 0);
        let _ = drain(&mut rx);
    }

    #[test]
    fn find_record_rejects_finished() {
        let (mut s, mut rx) = setup();
        s.start_agent(start("a1", Some("sess"), Some("coder")), 0);
        s.stop_agent(
            StopAgent {
                agent_id: Some("a1".into()),
                ..Default::default()
            },
            1_000,
        );
        let _ = drain(&mut rx);
        // Lifecycle on the finished record → no-op, no emit.
        let r = s.handle_lifecycle(
            Lifecycle {
                kind: Some("turn-end".into()),
                agent_id: Some("a1".into()),
                ..Default::default()
            },
            2_000,
        );
        assert!(r.is_none());
        assert!(drain(&mut rx).is_empty());
        // Visit too.
        let r = s.visit_room(
            VisitRoom {
                agent_id: Some("a1".into()),
                room: Some("test".into()),
                ..Default::default()
            },
            3_000,
        );
        assert!(r.is_none());
        assert!(drain(&mut rx).is_empty());
    }

    #[test]
    fn tool_event_unknown_agent_logs_no_emit() {
        let (mut s, mut rx) = setup();
        s.tool_event(
            ToolEvent {
                agent_id: Some("ghost".into()),
                tool_name: Some("Read".into()),
                ..Default::default()
            },
            1_000,
        );
        assert!(drain(&mut rx).is_empty());
    }

    #[test]
    fn tick_sweeps_expired_pending_descriptions() {
        let (mut s, _rx) = setup();
        s.buffer_description(
            BufferDescription {
                session_id: Some("sess".into()),
                subagent_type: Some("verifier".into()),
                description: Some("x".into()),
                ..Default::default()
            },
            0,
        );
        s.tick(PENDING_TTL.as_millis() as u64 - 1);
        assert_eq!(s.pending_descriptions.len(), 1);
        s.tick(PENDING_TTL.as_millis() as u64 + 1);
        assert!(s.pending_descriptions.is_empty());
    }

    #[test]
    fn snapshot_event_reflects_active_only() {
        let (mut s, mut rx) = setup();
        s.start_agent(start("a1", Some("sess"), Some("coder")), 0);
        s.start_agent(start("a2", Some("sess"), Some("verifier")), 0);
        s.stop_agent(
            StopAgent {
                agent_id: Some("a1".into()),
                ..Default::default()
            },
            100,
        );
        let _ = drain(&mut rx);
        match s.snapshot_event() {
            Event::Snapshot { agents } => {
                assert_eq!(agents.len(), 1);
                assert_eq!(agents[0].agent_id, "a2");
            }
            other => panic!("expected Snapshot, got {other:?}"),
        }
    }
}
