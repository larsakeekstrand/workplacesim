use serde::Deserialize;

/// Inputs for every state-mutating method. These will also be reused in step 2
/// as the axum route payload types, so they derive `Deserialize` now.
///
/// All fields mirror the union of keys observed in `server/index.js` route
/// handlers and `server/state.js` destructuring. Optional-everywhere because
/// hook payloads from Claude Code vary widely by event kind.

#[derive(Deserialize, Default, Debug, Clone)]
pub struct BufferDescription {
    pub session_id: Option<String>,
    pub subagent_type: Option<String>,
    pub description: Option<String>,
    pub tool_use_id: Option<String>,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct StartAgent {
    pub agent_id: String,
    pub session_id: Option<String>,
    pub agent_type: Option<String>,
    pub cwd: Option<String>,
    pub user: Option<String>,
    pub host: Option<String>,
    pub permission_mode: Option<String>,
    pub description: Option<String>,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct StopAgent {
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub agent_type: Option<String>,
    pub last_assistant_message: Option<String>,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct ToolEvent {
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub tool_name: Option<String>,
    pub permission_mode: Option<String>,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct VisitRoom {
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub room: Option<String>,
    pub ttl_ms: Option<u64>,
    pub permission_mode: Option<String>,
}

/// Shape of `PreToolUse` payloads. Only the `Agent` case is acted on; other
/// tool names reach the endpoint but become no-ops.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Pretool {
    pub session_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_use_id: Option<String>,
    #[serde(default)]
    pub tool_input: PretoolToolInput,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct PretoolToolInput {
    pub subagent_type: Option<String>,
    pub description: Option<String>,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Lifecycle {
    pub kind: Option<String>,
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub permission_mode: Option<String>,
    pub text: Option<String>,
    pub message: Option<String>,
    pub path: Option<String>,
    pub ok: Option<bool>,
    pub tool_name: Option<String>,
}
