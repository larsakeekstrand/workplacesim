#!/usr/bin/env bash
# workplacesim hook: reads Claude Code hook JSON from stdin, enriches with
# $USER + hostname, POSTs to the workplacesim server. Silent on failure so
# the hook never blocks Claude.
#
# Subcommands:
#   pretool        — PreToolUse(Agent) payload. Transforms tool_use_id → agent_id,
#                    tool_input.subagent_type → agent_type, tool_input.description →
#                    description; POSTs to /hooks/subagent-start.
#   pretool-file   — PreToolUse(Write|Edit|MultiEdit) payload. If tool_input.file_path
#                    looks like a test/spec/fixture path, POSTs a short lab visit.
#   posttool-bash  — PostToolUse(Bash) payload. If tool_input.command looks like a
#                    test-runner invocation, POSTs a longer lab visit.
#   stop           — SubagentStop payload. POSTs as-is to /hooks/subagent-stop.
#                    Backend uses FIFO-by-(session_id, agent_type) to correlate when
#                    the agent_id doesn't match a recorded start.
#   session-start  — SessionStart payload. Registers the main Claude Code session
#                    itself as a sim (agent_id = session_id, agent_type = "claude")
#                    so the office is occupied whenever Claude is running, not just
#                    during subagent dispatches.
#   session-end    — SessionEnd payload. Stops the main session sim.

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
  session-start)
    route="/hooks/subagent-start"
    payload=$(printf '%s' "$input" | jq --arg user "$user_arg" --arg host "$host_arg" '
      {
        agent_id: .session_id,
        session_id: .session_id,
        agent_type: "claude",
        description: "main session",
        cwd: .cwd,
        permission_mode: (.permission_mode // "default"),
        user: $user,
        host: $host
      }')
    ;;
  session-end)
    route="/hooks/subagent-stop"
    payload=$(printf '%s' "$input" | jq --arg user "$user_arg" --arg host "$host_arg" '
      {
        agent_id: .session_id,
        session_id: .session_id,
        agent_type: "claude",
        user: $user,
        host: $host
      }')
    ;;
  posttool-bash)
    tool_name=$(printf '%s' "$input" | jq -r '.tool_name // ""')
    [ "$tool_name" = "Bash" ] || exit 0
    cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // ""')
    # Heuristic: common test-runner invocations. Case-insensitive, tolerant of
    # leading pipes/&&/;/spaces. Intentionally conservative to avoid false
    # positives like `testfile.txt` or `best-option`.
    if ! printf '%s' "$cmd" | grep -qiE '(^|[;&|[:space:]])(npm|yarn|pnpm|bun|deno)([[:space:]]+run)?([[:space:]]+-s)?[[:space:]]+test([[:space:]:@]|$)|(^|[;&|[:space:]])(pytest|vitest|jest|mocha|ava|rspec|phpunit|tox|ward|nox)([[:space:]]|$)|(^|[;&|[:space:]])(go|cargo|dotnet|mix|swift)[[:space:]]+test([[:space:]]|$)|(^|[;&|[:space:]])(make|just|task)[[:space:]]+test([[:space:]]|$)'; then
      exit 0
    fi
    route="/hooks/lab-visit"
    payload=$(printf '%s' "$input" | jq --arg user "$user_arg" --arg host "$host_arg" '
      {
        session_id: .session_id,
        agent_id: .session_id,
        room: "test",
        source: "bash",
        ttl_ms: 20000,
        user: $user,
        host: $host
      }')
    ;;
  pretool-file)
    tool_name=$(printf '%s' "$input" | jq -r '.tool_name // ""')
    case "$tool_name" in
      Write|Edit|MultiEdit) ;;
      *) exit 0 ;;
    esac
    file_path=$(printf '%s' "$input" | jq -r '.tool_input.file_path // ""')
    [ -n "$file_path" ] || exit 0
    if ! printf '%s' "$file_path" | grep -qiE '(^|/)[^/]*\.(test|spec)\.[cmt]?[jt]sx?$|(^|/)[^/]*_test\.(go|py|rs)$|(^|/)[^/]*_spec\.rb$|(^|/)test_[^/]*\.py$|(^|/)(tests?|__tests__|spec|specs|fixtures)(/|$)'; then
      exit 0
    fi
    route="/hooks/lab-visit"
    payload=$(printf '%s' "$input" | jq --arg user "$user_arg" --arg host "$host_arg" '
      {
        session_id: .session_id,
        agent_id: .session_id,
        room: "test",
        source: "edit",
        ttl_ms: 8000,
        user: $user,
        host: $host
      }')
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
