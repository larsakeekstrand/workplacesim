#!/usr/bin/env bash
# workplacesim hook: reads Claude Code hook JSON from stdin, enriches with
# $USER + hostname, POSTs to the workplacesim server. Silent on failure so
# the hook never blocks Claude.
#
# Subcommands:
#   pretool  — PreToolUse(Agent) payload. Transforms tool_use_id → agent_id,
#              tool_input.subagent_type → agent_type, tool_input.description →
#              description; POSTs to /hooks/subagent-start.
#   stop     — SubagentStop payload. POSTs as-is to /hooks/subagent-stop.
#              Backend uses FIFO-by-(session_id, agent_type) to correlate when
#              the agent_id doesn't match a recorded start.

set -u

URL="${WORKPLACESIM_URL:-http://127.0.0.1:4317}"
sub="${1:-}"

if ! command -v jq >/dev/null 2>&1; then
  exit 0
fi

input=$(cat)
user_arg="${USER:-unknown}"
host_arg="$(hostname -s 2>/dev/null || hostname)"

case "$sub" in
  pretool)
    route="/hooks/subagent-start"
    payload=$(printf '%s' "$input" | jq --arg user "$user_arg" --arg host "$host_arg" '
      {
        agent_id: .tool_use_id,
        session_id: .session_id,
        agent_type: (.tool_input.subagent_type // "agent"),
        description: (.tool_input.description // ""),
        cwd: .cwd,
        permission_mode: (.permission_mode // "default"),
        user: $user,
        host: $host
      }')
    ;;
  stop)
    route="/hooks/subagent-stop"
    payload=$(printf '%s' "$input" | jq --arg user "$user_arg" --arg host "$host_arg" \
      '. + {user: $user, host: $host}')
    ;;
  *)
    exit 0
    ;;
esac

curl -fsS --max-time 2 \
  -H "content-type: application/json" \
  -d "$payload" \
  "${URL}${route}" >/dev/null 2>&1 || true

exit 0
