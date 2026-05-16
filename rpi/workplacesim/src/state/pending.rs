/// TTL-bounded buffer for Agent-tool descriptions.
///
/// `PreToolUse(Agent)` fires before the subagent actually runs; when the
/// subagent's `SubagentStart`-equivalent payload arrives it often lacks a
/// description, so we stash the description keyed by `(session_id, agent_type)`
/// for the brief window between the two events.
#[derive(Clone, Debug)]
pub struct PendingDescription {
    pub description: String,
    pub tool_use_id: Option<String>,
    pub ts: u64,
}
