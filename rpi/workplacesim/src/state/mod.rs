//! In-memory state machine. Ports `server/state.js` event-for-event.
//!
//! The JS module mutated module-level globals; here we own everything in
//! `State` behind `Arc<parking_lot::RwLock<_>>`. The render thread runs in a
//! sync context (minifb's event loop is blocking), so a tokio RwLock would
//! force us to carry a runtime into the renderer — parking_lot keeps locking
//! cheap and sync on both the HTTP and render sides. Broadcast is still a
//! tokio `broadcast` channel: the `/events` SSE route subscribes via
//! `State::subscribe_events()` and re-encodes each `Event` as `data: <json>\n\n`.
//!
//! Time is injected: every method that needs "now" takes `now_ms: u64`.
//! Tests pass deterministic values; production callers use `clock::now_ms()`.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

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

use crate::config::SharedConfig;
use pending::PendingDescription;

const ERROR_PREVIEW_LEN: usize = 80;
const PROMPT_PREVIEW_LEN: usize = 80;

const VALID_ROOMS: &[&str] = &["test", "meeting", "desk"];

// I and O dropped so "1/l" and "0/O" don't collide on a low-res TV.
const SESSION_LABEL_POOL: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZ";

/// Capacity for the broadcast channel. Subscribers that lag by more than this
/// many messages see `RecvError::Lagged` and can resync via a snapshot event.
const BROADCAST_CAP: usize = 256;

/// Window over which `events_per_min()` averages. Keep a little more history
/// than the 60-second query window so trimming doesn't race with reads.
const EVENT_HISTORY_MS: u64 = 5 * 60_000;
/// Hard upper bound on the ring buffer so a runaway scene can't grow state
/// without bound. `events_per_min` caps at ~600 events/sec under this cap.
const EVENT_HISTORY_CAP: usize = 200_000;

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

    /// Shared runtime config; read each tick/visit/stop with a short-lived
    /// RwLock read. Cloning the Arc is cheap, so tests hand in their own.
    config: SharedConfig,

    /// Lifetime count of broadcast events emitted since `new_state`. Surfaced
    /// on `/api/status` alongside `events_per_min`.
    events_total: u64,

    /// Rolling timestamps (ms) of recent events, newest at the back. Used by
    /// `events_per_min()` to compute an average over the last 60 s. Older
    /// entries are trimmed on every push; see `EVENT_HISTORY_MS`.
    recent_event_ts: VecDeque<u64>,

    // session_id → assigned label char. Released when the last record from
    // that session leaves `active_agents`.
    session_labels: HashMap<String, char>,

    // Free pool drained in order; tail is the next char handed out. None when
    // exhausted, in which case fresh sessions render label-less.
    label_free_pool: VecDeque<char>,
}

/// Construct a fresh State wrapped in Arc<RwLock>, plus a subscribed receiver
/// so the caller doesn't race with the first emitted event.
pub fn new_state(config: SharedConfig) -> (Arc<RwLock<State>>, broadcast::Receiver<Event>) {
    let (tx, rx) = broadcast::channel(BROADCAST_CAP);
    let state = State {
        active_agents: IndexMap::new(),
        pending_descriptions: IndexMap::new(),
        pending_stops: IndexMap::new(),
        tx,
        config,
        events_total: 0,
        recent_event_ts: VecDeque::new(),
        session_labels: HashMap::new(),
        label_free_pool: SESSION_LABEL_POOL.chars().collect(),
    };
    (Arc::new(RwLock::new(state)), rx)
}

impl State {
    /// Subscribe to the broadcast channel. New subscribers may want to pair
    /// this with `snapshot_event()` to get the current active set.
    pub fn subscribe_events(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    fn emit(&mut self, event: Event) {
        // broadcast::Sender::send errors only when there are zero active
        // receivers. That's fine — events are fire-and-forget.
        self.events_total = self.events_total.saturating_add(1);
        let now_ms = clock::now_ms();
        self.recent_event_ts.push_back(now_ms);
        self.trim_event_history(now_ms);
        let _ = self.tx.send(event);
    }

    fn trim_event_history(&mut self, now_ms: u64) {
        let cutoff = now_ms.saturating_sub(EVENT_HISTORY_MS);
        while let Some(front) = self.recent_event_ts.front() {
            if *front < cutoff {
                self.recent_event_ts.pop_front();
            } else {
                break;
            }
        }
        // Safety cap to prevent unbounded growth under pathological event rates.
        while self.recent_event_ts.len() > EVENT_HISTORY_CAP {
            self.recent_event_ts.pop_front();
        }
    }

    /// Total count of broadcast events emitted since this `State` was created.
    /// Monotonic, saturating. Surfaced on `/api/status`.
    pub fn events_total(&self) -> u64 {
        self.events_total
    }

    /// Average events per minute over the last 60 s. Computed from the
    /// `recent_event_ts` ring buffer; returns 0.0 when the window is empty.
    pub fn events_per_min(&self) -> f64 {
        let now_ms = clock::now_ms();
        let window_start = now_ms.saturating_sub(60_000);
        let n = self
            .recent_event_ts
            .iter()
            .rev()
            .take_while(|ts| **ts >= window_start)
            .count();
        // Exact per-minute rate over the last 60 s. Since we already bound the
        // window to 60 s, the count IS the events/min.
        n as f64
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
        let pending_ttl = self.config.read().pending_ttl_ms;
        if now_ms.saturating_sub(entry.ts) > pending_ttl {
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
            let from_pending = self.consume_description(&p.session_id, &p.agent_type, now_ms);
            if !from_pending.is_empty() {
                from_pending
            } else if let Some(t) = p.agent_type.as_deref().filter(|s| !s.is_empty()) {
                t.to_string()
            } else {
                "agent".to_string()
            }
        };

        let session_label = self.assign_session_label(p.session_id.as_deref());

        let record = Agent {
            agent_id: p.agent_id.clone(),
            session_id: p.session_id,
            agent_type: p
                .agent_type
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "agent".to_string()),
            description,
            user: p
                .user
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "unknown".to_string()),
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
            session_label,
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
        let stop_grace = self.config.read().stop_grace_ms;
        self.pending_stops.insert(id, now_ms + stop_grace);
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

        let (visit_default, visit_min, visit_max) = {
            let c = self.config.read();
            (c.visit_default_ms, c.visit_min_ms, c.visit_max_ms)
        };
        let ttl_ms = match p.ttl_ms {
            Some(v) if v > 0 => v,
            _ => visit_default,
        };
        let ttl_ms = ttl_ms.max(visit_min).min(visit_max);

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

    // Look up or hand out a label for `session_id`. Pool exhaustion is a soft
    // failure — callers store None and the renderer skips painting.
    fn assign_session_label(&mut self, session_id: Option<&str>) -> Option<String> {
        let sid = session_id?;
        if let Some(c) = self.session_labels.get(sid) {
            return Some(c.to_string());
        }
        let c = self.label_free_pool.pop_front()?;
        self.session_labels.insert(sid.to_string(), c);
        Some(c.to_string())
    }

    // Return `session_id`'s char to the pool iff no other active record still
    // references it. Called after a stop is finalised in `tick`.
    fn release_session_label_if_unused(&mut self, session_id: &str) {
        if self
            .active_agents
            .values()
            .any(|a| a.session_id.as_deref() == Some(session_id))
        {
            return;
        }
        if let Some(c) = self.session_labels.remove(session_id) {
            self.label_free_pool.push_back(c);
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
        let pending_ttl = self.config.read().pending_ttl_ms;
        let pending_cutoff = now_ms.saturating_sub(pending_ttl);
        self.pending_descriptions
            .retain(|_, entry| entry.ts >= pending_cutoff);

        // 3. Finalise stops past grace window.
        let finalised: Vec<String> = self
            .pending_stops
            .iter()
            .filter_map(|(id, deadline)| {
                if now_ms >= *deadline {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();
        for id in finalised {
            self.pending_stops.shift_remove(&id);
            let removed_sid = self
                .active_agents
                .shift_remove(&id)
                .and_then(|a| a.session_id);
            if let Some(sid) = removed_sid {
                self.release_session_label_if_unused(&sid);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{self, Config};
    use tokio::sync::broadcast::error::TryRecvError;

    // Test-side aliases so call sites stay readable after the live consts
    // moved into `Config`. Values match the defaults in `Config::default()`
    // so the old expected-TTL assertions still hold.
    const PENDING_TTL_MS: u64 = config::DEFAULT_PENDING_TTL_MS;
    const STOP_GRACE_MS: u64 = config::DEFAULT_STOP_GRACE_MS;
    const VISIT_MIN_MS: u64 = config::DEFAULT_VISIT_MIN_MS;
    const VISIT_MAX_MS: u64 = config::DEFAULT_VISIT_MAX_MS;
    const VISIT_DEFAULT_MS: u64 = config::DEFAULT_VISIT_DEFAULT_MS;

    fn setup() -> (State, broadcast::Receiver<Event>) {
        let (tx, rx) = broadcast::channel(BROADCAST_CAP);
        let state = State {
            active_agents: IndexMap::new(),
            pending_descriptions: IndexMap::new(),
            pending_stops: IndexMap::new(),
            tx,
            config: config::shared(Config::default()),
            events_total: 0,
            recent_event_ts: VecDeque::new(),
            session_labels: HashMap::new(),
            label_free_pool: SESSION_LABEL_POOL.chars().collect(),
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
        let start_ms = PENDING_TTL_MS + 1_000;
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
            Some(2_000 + STOP_GRACE_MS)
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
        let grace_ms = STOP_GRACE_MS;

        s.tick(grace_ms - 1);
        assert!(
            s.active_agents.contains_key("a1"),
            "still present before grace"
        );
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
            assert_eq!(visit.until, 1_000 + VISIT_MIN_MS);
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
            assert_eq!(visit.until, 2_000 + VISIT_MAX_MS);
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
            assert_eq!(visit.until, 3_000 + VISIT_DEFAULT_MS);
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
        assert!(
            matches!(&events[0], Event::Visit { room: None, until: None, agent_id } if agent_id == "a1")
        );
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
        assert!(
            matches!(&events[0], Event::Reclassify { permission_mode, .. } if permission_mode == "plan")
        );
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
            Event::Idle {
                idle: false,
                agent_id,
            } => assert_eq!(agent_id, "a1"),
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
        assert_eq!(
            s.active_agents["a1"].session_prompt.as_ref().unwrap().len(),
            80
        );
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
            Event::ToolError {
                message, tool_name, ..
            } => {
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
        s.tick(PENDING_TTL_MS - 1);
        assert_eq!(s.pending_descriptions.len(), 1);
        s.tick(PENDING_TTL_MS + 1);
        assert!(s.pending_descriptions.is_empty());
    }

    #[test]
    fn session_label_assigned_in_pool_order_and_reused_per_session() {
        let (mut s, _rx) = setup();
        let a = s
            .start_agent(start("a1", Some("sess-1"), Some("claude")), 0)
            .unwrap();
        let b = s
            .start_agent(start("a2", Some("sess-1"), Some("verifier")), 0)
            .unwrap();
        let c = s
            .start_agent(start("a3", Some("sess-2"), Some("claude")), 0)
            .unwrap();
        assert_eq!(a.session_label.as_deref(), Some("1"));
        assert_eq!(b.session_label.as_deref(), Some("1"));
        assert_eq!(c.session_label.as_deref(), Some("2"));
    }

    #[test]
    fn session_label_none_when_no_session_id() {
        let (mut s, _rx) = setup();
        let a = s.start_agent(start("a1", None, Some("coder")), 0).unwrap();
        assert!(a.session_label.is_none());
    }

    #[test]
    fn session_label_released_only_when_last_record_finalised() {
        let (mut s, mut rx) = setup();
        s.start_agent(start("a1", Some("sess"), Some("claude")), 0);
        s.start_agent(start("a2", Some("sess"), Some("verifier")), 0);
        let _ = drain(&mut rx);
        // First stop + finalise: still another record on the session, no release.
        s.stop_agent(
            StopAgent {
                agent_id: Some("a1".into()),
                ..Default::default()
            },
            0,
        );
        s.tick(STOP_GRACE_MS + 1);
        let next = s
            .start_agent(start("a3", Some("other"), Some("claude")), 100)
            .unwrap();
        assert_eq!(next.session_label.as_deref(), Some("2"));
        // Finalise the last record on "sess" — "1" returns to the pool tail.
        // Fresh sessions still drain the head ("3") first; only once the head
        // chars have been used does "1" come back round.
        s.stop_agent(
            StopAgent {
                agent_id: Some("a2".into()),
                ..Default::default()
            },
            100,
        );
        s.tick(100 + STOP_GRACE_MS + 1);
        assert!(!s.session_labels.contains_key("sess"));
        assert!(s.label_free_pool.contains(&'1'));
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
