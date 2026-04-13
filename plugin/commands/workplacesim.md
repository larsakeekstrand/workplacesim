---
description: Check the workplacesim server status and report active agents.
---

Check whether the workplacesim visualizer server is reachable and report on it.

1. The server URL is `${WORKPLACESIM_URL:-http://127.0.0.1:4317}`. Use a 3-second curl timeout.
2. `GET /api/agents` — if it succeeds, parse the JSON and summarize:
   - How many agents are currently active.
   - For each, one line: `user@host  agent_type · description  → <room>` where `<room>` is determined by:
     - `lab` if `agent_type` or `description` contains "test" (case-insensitive)
     - `meeting` if `permission_mode === "plan"`
     - `desk` otherwise
3. If the request fails, say so plainly and suggest: `npm start` from the workplacesim repo, or set `WORKPLACESIM_URL` to point elsewhere.

Keep the output under 15 lines.
