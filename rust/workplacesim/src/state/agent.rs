use serde::Serialize;

/// One active (or recently-finished) sim.
///
/// Mirrors the record shape used by `server/state.js` so JSON produced by
/// `Event::Snapshot { agents }` matches the browser's expectations exactly.
/// Extra fields are skipped when `None` to avoid leaking internal state.
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
pub struct Agent {
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub agent_type: String,
    pub description: String,
    pub user: String,
    pub host: String,
    pub cwd: String,
    pub permission_mode: String,
    pub started_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visit: Option<Visit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_error: Option<CurrentError>,
}

#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
pub struct Visit {
    pub room: String,
    pub until: u64,
}

#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
pub struct CurrentError {
    pub tool_name: String,
    pub message: String,
    pub ts: u64,
}
