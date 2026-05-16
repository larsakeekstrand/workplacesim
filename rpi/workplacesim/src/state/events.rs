use serde::Serialize;

use super::agent::Agent;

/// Every variant here corresponds 1:1 to a `broadcast({type: ...})` call in
/// `server/state.js`. Serde's external-ish tag/rename config keeps the wire
/// format identical so the existing browser client can consume it unchanged.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Event {
    Snapshot {
        agents: Vec<Agent>,
    },
    Start {
        // Boxed to keep `Event` small — the other variants are <= 72 bytes,
        // Agent is ~360.
        agent: Box<Agent>,
    },
    Stop {
        agent_id: String,
    },
    Visit {
        agent_id: String,
        room: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        until: Option<u64>,
    },
    Tool {
        agent_id: String,
        tool_name: String,
        ts: u64,
    },
    Reclassify {
        agent_id: String,
        permission_mode: String,
    },
    #[serde(rename = "file-touch")]
    FileTouch {
        agent_id: String,
        path: String,
    },
    Prompt {
        agent_id: String,
        text: String,
    },
    Idle {
        agent_id: String,
        idle: bool,
    },
    #[serde(rename = "turn-end")]
    TurnEnd {
        agent_id: String,
    },
    #[serde(rename = "bash-result")]
    BashResult {
        agent_id: String,
        ok: bool,
    },
    #[serde(rename = "tool-error")]
    ToolError {
        agent_id: String,
        tool_name: String,
        message: String,
    },
}
