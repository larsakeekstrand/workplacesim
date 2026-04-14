# workplacesim — project context

A top-down, game-style visualizer of Claude Code subagent activity. A Node
server ingests hook events from Claude Code sessions and broadcasts them
over Server-Sent Events to a Phaser 3 browser frontend. Each running
subagent is a sim that walks into one of three rooms (open-plan desks,
meeting room, or test lab) depending on its type.

## Stack

- **Backend** `server/index.js` + `server/state.js` — Express 4, SSE, no
  database. Everything lives in an in-memory `Map`; a server restart
  clears the scene.
- **Frontend** `public/index.html` + `public/main.js` — Phaser 3 loaded
  from CDN, procedural pixel-art (no sprite assets; `pixelArt: true`).
  Canvas `1280 × 640`, scale `Phaser.Scale.FIT`.
- **Claude plugin** `plugin/` — ships as a Claude Code plugin that
  registers the hooks and a `/workplacesim` slash command. Installable
  from the repo root via `.claude-plugin/marketplace.json`.
- **Simulator** `scripts/simulate.js` — zero-dep Node script that POSTs
  fake traffic to the server; useful for working on the frontend without
  real Claude sessions.

## Layout

```
workplacesim/
  .claude-plugin/marketplace.json   # one-plugin marketplace at repo root
  .claude/settings.json             # project-scope enable of the plugin
  plugin/
    .claude-plugin/plugin.json      # plugin manifest
    hooks/hooks.json                # plugin hook registrations
    hooks/workplacesim-hook.sh      # bash hook, reads stdin JSON, POSTs
    commands/workplacesim.md        # /workplacesim slash command
    README.md
  server/{index,state}.js
  public/{index.html,main.js,picker.html,assets/kenney-1bit/…}
  scripts/simulate.js
```

## Hook wiring (non-obvious lessons the hard way)

- **Two independent lifecycles produce sims**:
  1. **Main session sim** — `SessionStart` spawns a sim with
     `agent_id = session_id`, `agent_type = "claude"`; `SessionEnd`
     removes it. This is why the office is occupied whenever Claude
     Code is running at all, even with no subagents. Because
     `agent_id = session_id` is stable, the `SubagentStop` FIFO
     fallback is *only* exercised by subagent stops, not session
     stops.
  2. **Subagent sims** — one per `Agent` tool invocation (see below).
- **`SubagentStart` is not a real Claude Code hook event.** Earlier doc
  lookups suggested otherwise; the runtime does not fire it. The real
  subagent lifecycle is: `PreToolUse` (matcher `"Agent"`) → the
  subagent runs → `SubagentStop`.
- `PreToolUse(Agent)` gives us `tool_use_id`, `session_id`, `cwd`,
  `permission_mode`, and `tool_input.{subagent_type,description}`.
  The hook script reshapes this into a `/hooks/subagent-start` request:
  `tool_use_id → agent_id`, subagent_type/description lifted to the top.
- `SubagentStop` gives a *different* `agent_id` than anything we saw at
  dispatch time. There is **no direct correlation**. `server/state.js`
  `stopAgent` falls back to FIFO by `(session_id, agent_type)` when
  direct `agent_id` lookup misses.
- `plugin/hooks/hooks.json` uses the plugin-specific **wrapped** form
  `{"description": "...", "hooks": { "PreToolUse": [...] }}` — the
  unwrapped form is for `~/.claude/settings.json` only.
- Hook scripts are read fresh each invocation, so script edits take
  effect immediately. `hooks.json` changes (events, matchers) require a
  Claude Code restart; `/reload-plugins` alone isn't enough when
  enabling new event subscriptions. The installed copy lives at
  `~/.claude/plugins/cache/workplacesim/workplacesim/0.1.0/` and is
  resynced from source on session start, so edits should go in
  `plugin/hooks/...` first.
- Don't block Claude: hooks are `type: command` without `async` (async
  is fine if available); the script does `curl -fsS --max-time 2 … ||
  true` and exits 0 so a down server or missing `jq` never stalls a
  session.

## Visualization conventions

- **Room layout** `main.js` top-of-file constants. Canvas is split:
  open-plan on the left, meeting + lab stacked in the right column,
  exterior walls on the perimeter. Doors "punch" gaps in wall
  rectangles. Exterior walls get procedural windows with a faint
  light-spill trapezoid inside.
- **Routing priority** (computed in `classify(agent)`):
  1. `agent_type` or `description` contains `"test"` → lab
  2. `permission_mode === "plan"` → meeting
  3. else → desk
- **Sim sprites** are composed containers (`shadow`, `legs`, `body`,
  `head`, `hair`, optional `badge`), scale `1.8`. Shirt / pants / skin
  / hair colors are hashed from `user` so the same person reads as the
  same character across sessions.
- **Labels** sit south of the sim, fade to 0.35 alpha once seated, and
  restore to 1.0 on hover.
- **No new assets required.** Anything that reads well procedurally
  (rectangles + lines + circles + subtle alphas) is preferred. The
  Kenney 1-Bit pack in `public/assets/kenney-1bit/` is staged for a
  future tile-based swap with `public/picker.html`, but the shipped
  scene is 100% procedural.

## Running

```sh
npm install
npm start            # server on http://127.0.0.1:4317
# separate shell, optional:
npm run simulate -- --plan-ratio=0.3 --max-concurrent=6
```

## When working on this code

- Keep the frontend rendering procedural unless the user explicitly asks
  for tile-based art — the picker page and Kenney pack are scaffolding,
  not the current shipping path.
- Any change to hook events / matchers must be mirrored to both
  `plugin/hooks/hooks.json` in the repo AND tested after a Claude Code
  restart, not just `/reload-plugins`.
- Backend is fire-and-forget by design: silent failure on a down server
  is a feature, not a bug.
- One file per room-drawing routine (`drawMeetingRoom`, `drawLabRoom`);
  new rooms follow the same pattern (walls → furniture → seats → any
  local decor).
