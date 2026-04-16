# workplacesim — project context

A top-down, game-style visualizer of Claude Code subagent activity. A Node
server ingests hook events from Claude Code sessions and broadcasts them over
Server-Sent Events to a Phaser 3 browser frontend. Each active agent is a sim
that walks into one of three rooms, bobs at a seat, and walks out. Around that
core, ambient telemetry (motes, footsteps, tethers, glyphs, window-breathing)
and dispatch surfaces (file-touch ticker, lab bench monitors, plan-mode
whiteboard) react to every tool call and lifecycle signal.

## Stack

- **Backend** `server/index.js` + `server/state.js` — Express 4, SSE, no
  database. Everything lives in in-memory `Map`s; server restart clears the
  scene.
- **Frontend** `public/index.html` + `public/main.js` — Phaser 3 loaded from
  CDN, procedural pixel-art (`pixelArt: true`, no sprite assets). Canvas
  `1280 × 640`, scale `Phaser.Scale.FIT`.
- **Claude plugin** `plugin/` — Claude Code plugin that registers the hooks
  and a `/workplacesim` slash command. Installable from the repo root via
  `.claude-plugin/marketplace.json`.
- **Simulator** `scripts/simulate.js` — zero-dep Node script that POSTs fake
  traffic exercising every feature; useful for working on the frontend
  without real Claude sessions.
- **Rust port** `rust/workplacesim/` — additive; not required for the
  browser path. Speaks the exact HTTP+SSE protocol and renders the same
  scene directly to `/dev/fb0` on a Raspberry Pi 1, with a minifb
  desktop backend for Mac dev. Default Cargo features are `[]`; pick
  one of `--features desktop` (Mac window) or `--features fb` (Linux
  framebuffer, Pi). Deploy via `rust/workplacesim/deploy/install.sh
  pi@host`. Hooks on the Mac point at the Pi via
  `WORKPLACESIM_URL=http://<pi>:4317`.

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
  rust/workplacesim/              # Rust port for Raspberry Pi 1 / framebuffer
    src/{state,server,render}/…   # state.js + index.js + public/main.js ports
    deploy/{install.sh,workplacesim.service,README.md}
    tests/golden/*.raw            # byte-identical rendering regression tests
```

## Hook wiring (non-obvious lessons the hard way)

- **Two independent lifecycles produce sims**:
  1. **Main session sim** — `SessionStart` spawns a sim with
     `agent_id = session_id`, `agent_type = "claude"`; `SessionEnd` removes
     it. The office is occupied whenever Claude Code is running. Because
     `agent_id = session_id` is stable, the `SubagentStop` FIFO fallback is
     *only* exercised by subagent stops, not session stops.
  2. **Subagent sims** — one per `Agent` tool invocation (see below).
- **`SubagentStart` is not a real Claude Code hook event.** The runtime
  doesn't fire it. The real subagent lifecycle is:
  `PreToolUse(Agent)` → the subagent runs → `SubagentStop`.
- `PreToolUse(Agent)` carries `tool_use_id`, `session_id`, `cwd`,
  `permission_mode`, `tool_input.{subagent_type,description}`. The hook
  script reshapes this into `/hooks/subagent-start`: `tool_use_id → agent_id`,
  subagent_type/description lifted to the top.
- `SubagentStop` returns a *different* `agent_id` than dispatch. There is
  **no direct correlation**. `server/state.js` `stopAgent` falls back to
  FIFO by `(session_id, agent_type)` when direct `agent_id` lookup misses.
- `plugin/hooks/hooks.json` uses the plugin-specific **wrapped** form
  `{"description": "...", "hooks": { "PreToolUse": [...] }}` — the unwrapped
  form is for `~/.claude/settings.json` only.
- Hook scripts are read fresh each invocation, so script edits take effect
  immediately. **`hooks.json` changes (new events, new matchers) require a
  full Claude Code restart**; `/reload-plugins` is not enough. The installed
  copy lives at `~/.claude/plugins/cache/workplacesim/workplacesim/<ver>/`
  and is resynced from source on session start.
- Don't block Claude: hooks are `type: command`, the script pipes through
  `curl -fsS --max-time 2 … || true` and exits 0 so a down server or missing
  `jq` never stalls a session.

## Hook subscriptions and their surfaces

| Hook | Matcher | Script branch | Endpoint | What it drives |
|---|---|---|---|---|
| `SessionStart` | — | `session-start` | `/hooks/subagent-start` | Spawns the main session sim. |
| `SessionEnd` | — | `session-end` | `/hooks/subagent-stop` | Dismisses the main session sim. |
| `PreToolUse` | `Agent` | `pretool` | `/hooks/subagent-start` | Spawns each subagent sim; parent→child tether. |
| `PreToolUse` | `Write\|Edit\|MultiEdit` | `pretool-file` | `/hooks/lab-visit` (if test path) + `/hooks/lifecycle` (always) | Lab visit on test-file edits; file-touch ticker on the open-plan wall. |
| `PreToolUse` | *(broad tool list)* | `pretool-any` | `/hooks/tool-event` | Tool-call motes + `reclassify` on mode change. |
| `PostToolUse` | `Bash` | `posttool-bash` | `/hooks/lab-visit` (if test runner) + `/hooks/lifecycle` (always) | Lab visit on `npm test`/`pytest`/…; lab bench monitor flash. |
| `PostToolUse` | *(broad tool list)* | `posttool-any` | `/hooks/lifecycle` | Red error halo + transient `!` glyph when `tool_response.error` is present. |
| `UserPromptSubmit` | — | `user-prompt` | `/hooks/lifecycle` (prompt) | Whiteboard update; clears `idle`. |
| `Notification` | — | `notification` | `/hooks/lifecycle` (idle) | `💤` glyph on the session sim. |
| `Stop` | — | `stop-turn` | `/hooks/lifecycle` (turn-end) | 1 s head-wag tween on the session sim. |
| `SubagentStop` | — | `stop` | `/hooks/subagent-stop` | Dismisses the subagent sim. |

All hook payloads carry `permission_mode`. The server compares it against the
stored value for the targeted record on every incoming call; a difference
mutates the record and broadcasts `{type:"reclassify", agent_id, permission_mode}`.
The frontend walks the sim to the room implied by the new mode. This is what
makes mid-session plan-mode toggles visible.

## Server state primitives

- `activeAgents: Map<agent_id, record>` — the authoritative sim list. Records
  get optional fields added as features need them: `visit`, `session_prompt`,
  `idle`, `current_error`.
- `visitRoom(payload)` — TTL-bounded room move; broadcasts `visit` on entry
  and `{room:null}` on expiry. `VISIT_ROOMS = {test, meeting, desk}` but
  only `test` is currently used.
- `checkPermissionMode(record, incoming)` — called from every endpoint that
  receives a payload with `permission_mode`. Emits `reclassify`.
- `handleLifecycle(payload)` — dispatcher keyed by `kind`; covers `prompt`
  (stores `session_prompt`), `idle`, `turn-end`, `file-touch`, `bash-result`,
  `tool-error`. Most kinds are broadcast-only.
- `broadcastToolEvent(payload)` — one-shot mote broadcast; no record mutation.

## Visualization conventions

- **Room layout** (`main.js` top-of-file constants): open-plan on the left,
  meeting + lab stacked in the right column, exterior walls on the perimeter.
  Doors "punch" gaps in wall rectangles. Exterior walls have procedural
  windows with a faint light-spill trapezoid modulated by recent event rate.

- **Corridor routing** (`computeRoute`, `pathFromDoorTo`): sims hug an
  L-shaped corridor grid, never cutting diagonals across desks. Vertical
  halls at `HALLWAY_LEFT_X=130` and `HALLWAY_RIGHT_X=776`; horizontal
  corridors at `CORRIDOR_YS=[125, 256, 416, 576]` (the three southern values
  are `DESK_ROWS[i] + APPROACH_OFFSET_Y`, reusing existing geometry).
  `pathBetweenHallNodes` snaps cross-hall transit to a corridor y so the
  horizontal leg is always desk-safe.

- **Routing priority** (`classify(agent)`):
  1. `agent_type` or `description` matches `LAB_KEYWORDS` (test, spec, review,
     verify, verifier, lint, bench, analyzer, hunter, qa) → lab
  2. `permission_mode === "plan"` → meeting
  3. else → desk

- **Sim sprites** are composed containers (`shadow`, `legs`, `body`, `head`,
  `hair`, optional `badge`, child `glyph` Text), scale `1.8`. Shirt / pants /
  skin / hair colors are hashed from `user` so the same person reads as the
  same character across sessions.

- **Effects `Graphics` layer** — one shared `this.effects` Graphics, cleared
  and redrawn each `update()`. Holds footstep trails and parent→child
  tethers. Error halos are separate per-sim Graphics so they can tween alpha
  independently.

- **Glyph ladder** (highest priority first, in `simGlyph(sim)`):
  `!` (error, 2 s) → `🧪` (visiting lab) → `💤` (idle) → `…` (walking) →
  `📋` (seated in plan) → `Z` (seated >60 s) → empty.

- **Tool-mote palette** (`MOTE_COLORS`): Read/Grep/Glob blue `#7fc7ff`,
  Write/Edit/MultiEdit amber `#ffb86c`, Bash green `#8be98b`, Agent/Task
  magenta `#ff8fd4`, Web* purple `#c28fff`, default `#cccccc`. Capped at
  `MOTE_CAP = 40` live motes, drop-oldest.

- **Labels** sit south of the sim, fade to 0.35 alpha once seated, restore
  to 1.0 on hover. The file-touch ticker is a single `Text` along the
  open-plan north wall.

- **No new assets required.** Procedural pixel-art is the shipping path. The
  Kenney 1-Bit pack in `public/assets/kenney-1bit/` and `public/picker.html`
  are staged for a future tile-based swap but aren't used.

## Running

```sh
npm install
npm start            # server on http://127.0.0.1:4317
# separate shell, optional:
npm run simulate -- --plan-ratio=0.3 --max-concurrent=6
```

## When working on this code

- Keep the frontend rendering procedural unless the user explicitly asks for
  tile-based art — the picker page and Kenney pack are scaffolding, not the
  current shipping path.
- Any change to hook events / matchers must be mirrored to both
  `plugin/hooks/hooks.json` in the repo AND tested after a full Claude Code
  restart, not just `/reload-plugins`.
- Every hook POST must carry `permission_mode` if the sim it targets should
  reclassify on plan-mode toggles. The session sim's mode is authoritative;
  subagents capture mode at spawn and don't change.
- Backend is fire-and-forget by design: silent failure on a down server is
  a feature, not a bug. Hook script must never block or fail noisily.
- One file per room-drawing routine (`drawMeetingRoom`, `drawLabRoom`); new
  rooms follow the same pattern (walls → furniture → seats → local decor).
  Register any interactive sub-graphic (e.g. lab-monitor flash overlay,
  meeting whiteboard text) on the scene so the SSE handlers can address it.
- `/hooks/lifecycle` is the catch-all endpoint for new event types — add a
  `kind` case in `handleLifecycle` plus a matching SSE handler in `onEvent`
  rather than minting a new route.
- When adding any new SSE type, update `README.md`'s type list.
- Changes to shared behaviour (routing constants, classify rules, state
  semantics, SSE event shape) need to land in **both** the Node backend
  and the Rust port. The Rust crate has unit + golden-frame + HTTP
  integration tests that will catch drift; run `cargo test --features
  desktop --no-default-features` from `rust/workplacesim/` after any
  such change. Parity is the contract.
