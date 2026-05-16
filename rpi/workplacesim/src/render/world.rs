//! Per-frame lightweight snapshot of the authoritative `state::State`. The
//! render thread clones this under a read lock and drops the lock before
//! touching any animation state, so hook POSTs never contend with drawing.
//!
//! Deliberately a `Vec` not a `HashMap`: seat allocation in `SimStore` needs a
//! stable iteration order so two sims spawned in the same frame get the same
//! seats in reconcile regardless of the underlying map's bucket ordering. We
//! sort by `(started_at, agent_id)` below.

use crate::state::{Agent, State};

/// One agent, cloned out of `State::active_agents` with just the fields the
/// renderer reads. Keeps the snapshot small (no `session_prompt`, `visit`,
/// etc. for now — those belong to later sub-steps).
#[derive(Clone, Debug)]
pub struct AgentView {
    pub agent_id: String,
    pub session_id: Option<String>,
    pub user: String,
    pub agent_type: String,
    pub description: String,
    pub permission_mode: String,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub session_label: Option<String>,
}

impl From<&Agent> for AgentView {
    fn from(a: &Agent) -> Self {
        Self {
            agent_id: a.agent_id.clone(),
            session_id: a.session_id.clone(),
            user: a.user.clone(),
            agent_type: a.agent_type.clone(),
            description: a.description.clone(),
            permission_mode: a.permission_mode.clone(),
            started_at: a.started_at,
            finished_at: a.finished_at,
            session_label: a.session_label.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RenderWorld {
    pub agents: Vec<AgentView>,
    pub now_ms: u64,
}

impl RenderWorld {
    /// Build a world snapshot from locked `State`. The caller holds the lock
    /// briefly; this function only reads. Agents include finished-but-still-
    /// present records so the renderer can animate them walking out during
    /// their `STOP_GRACE` window.
    pub fn from_state(state: &State, now_ms: u64) -> Self {
        let mut agents: Vec<AgentView> = state
            .list_all_including_finished()
            .iter()
            .map(AgentView::from)
            .collect();
        agents.sort_by(|a, b| {
            a.started_at
                .cmp(&b.started_at)
                .then_with(|| a.agent_id.cmp(&b.agent_id))
        });
        Self { agents, now_ms }
    }
}
