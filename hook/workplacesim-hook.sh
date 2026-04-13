#!/usr/bin/env bash
# workplacesim hook: reads Claude Code hook JSON from stdin, enriches with
# $USER + hostname, POSTs to the workplacesim server. Silent on failure so
# the hook never blocks Claude.

set -u

URL="${WORKPLACESIM_URL:-http://127.0.0.1:4317}"
sub="${1:-}"

case "$sub" in
  pretool) route="/hooks/pretool" ;;
  start)   route="/hooks/subagent-start" ;;
  stop)    route="/hooks/subagent-stop" ;;
  *)
    echo "usage: workplacesim-hook.sh {pretool|start|stop}" >&2
    exit 0
    ;;
esac

if ! command -v jq >/dev/null 2>&1; then
  exit 0
fi

payload=$(jq --arg user "${USER:-unknown}" --arg host "$(hostname -s 2>/dev/null || hostname)" \
  '. + {user: $user, host: $host}')

curl -fsS --max-time 2 \
  -H "content-type: application/json" \
  -d "$payload" \
  "${URL}${route}" >/dev/null 2>&1 || true

exit 0
