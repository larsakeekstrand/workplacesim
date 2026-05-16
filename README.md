# workplacesim

> *The idea isn't original — variations on "AI agents as little characters in a office setting" turn up in various corners of the internet. This is my own take on it, wired specifically to Claude Code's hook system.*

A top-down, game-style visualization of Claude Code subagent activity. A Node
server ingests hook events over HTTP; the Phaser 3 browser frontend subscribes
to a Server-Sent Events stream and renders little pixel-art sims walking into
rooms, sitting at desks, running tests, toggling plan mode, and reacting to
every tool call. Procedural pixel-art — no sprite assets.

## Prerequisites

- Node.js 20+
- `jq` and `curl` on any machine whose Claude Code sessions should report in

## Run the server

```sh
npm install
npm start
# open http://localhost:4317
```

Env vars: `PORT` (default `4317`), `HOST` (default `127.0.0.1`; set to `0.0.0.0`
to accept hook posts from other machines on the LAN).

## Install as a Claude Code plugin

The hook side ships as a Claude Code plugin in `plugin/`. Install it from this
repo as a one-plugin marketplace:

```
/plugin marketplace add /absolute/path/to/workplacesim
/plugin install workplacesim
```

**Restart Claude Code after installing.** The plugin registers eight hook
subscriptions; `/reload-plugins` is not enough to activate new event
subscriptions. After restart, `/workplacesim` checks the visualizer's status.

To point the plugin at a visualizer running on another host, set
`WORKPLACESIM_URL=http://<host>:4317` in the environment Claude Code runs in.

See `plugin/README.md` for plugin-specific details.

## Hook simulator (no real Claude needed)

```sh
npm run simulate
# common flags:
#   --rate=1500            subagent spawn interval
#   --max-concurrent=6     simultaneous subagents
#   --tool-rate=800        tool-event cadence (motes)
#   --lifecycle-rate=2500  prompt/idle/turn-end/file-touch/bash-result/error
#   --reclassify-rate=25000  plan-mode toggle cadence
#   --lab-visit-rate=18000  lab-visit cadence
#   --plan-ratio=0.25      fraction of subagents spawned in plan mode
#   --no-main-session      skip the persistent session sim
```

The simulator runs one persistent session sim plus a rolling set of subagents,
all sharing a session_id so tether lines render correctly. Every feature
surface is exercised: tool-call motes, file-touch ticker, lab monitor flashes,
plan-mode whiteboard, idle glyph, turn-end wave, error halo, lab visits, and
permission-mode reclassify. Ctrl+C to stop — a `SessionEnd`-equivalent clears
the session sim cleanly.

## What you'll see

### Rooms

- **Open plan** (left) — default seating for any subagent; 12 desks across
  three rows of four. The session sim lives here when Claude is in default
  mode.
- **Meeting room** (top right) — plan mode. Whiteboard shows the current user
  prompt.
- **Test lab** (bottom right) — any subagent whose `agent_type` or
  `description` matches `test|spec|review|verify|verifier|lint|bench|analyzer|hunter|qa`.
  The session sim also makes short "lab visits" when it runs a test command
  (`npm test`, `pytest`, `go test`, …) or edits a test-file path.

### Ambient details

- **Status glyphs over each sim** (priority order): `!` on tool error (2 s),
  `🧪` visiting the lab, `💤` idle, `…` walking, `📋` seated in plan, `Z` seated
  >60 s.
- **Footstep trails** fade behind every walking sim in the sim's shirt color.
- **Parent→child tethers** — a brief dashed line from the main session sim to
  each new subagent it spawns.
- **Window-light breathing** — the exterior windows brighten with recent event
  rate; the office visibly inhales on activity bursts and settles on idle.
- **Tool-call motes** — tiny pixel dots drift up from a sim's head on each
  tool call, color-coded by tool family (Read=blue, Write=amber, Bash=green,
  Agent=magenta, Web=purple).

### Dispatch surfaces

- **File-touch ticker** on the open-plan north wall shows the last 8 edited
  paths.
- **Lab bench monitors** flash green/red round-robin on every `PostToolUse(Bash)`
  result, fed by the exit code.
- **Plan-mode whiteboard** in the meeting room shows the current user prompt.

### Live reclassify

Toggling plan mode mid-session walks the session sim between rooms:
permission_mode is compared on every incoming hook, and changes broadcast a
`reclassify` event. Lab visits take priority over reclassify; when the visit
ends, the sim returns to whatever its current classification says.

### Corridor routing

Sims hug a corridor grid instead of cutting diagonals across desks — vertical
halls at x=130 and x=776, horizontal corridors at y=125/256/416/576.

## Running on a Raspberry Pi (no browser)

A Rust port in `rpi/workplacesim/` renders the scene directly to
`/dev/fb0` on a Raspberry Pi 1 (ARMv6) — no browser, no X, no Node.
Plug the Pi into a TV via HDMI, deploy the binary as a systemd
service, point Claude Code's hooks at it over the LAN:

```sh
# From this repo on macOS (needs: Docker, cargo install cross):
cd rpi/workplacesim
./deploy/install.sh pi@raspberrypi.local
# On the Mac where Claude Code runs:
export WORKPLACESIM_URL=http://raspberrypi.local:4317
```

The Rust binary is a drop-in for the Node server: same HTTP+SSE
protocol, same payloads, same `/events` and `/` routes (so a browser
at `http://<pi>:4317/` still shows the Phaser frontend for debug).
Dev on macOS runs windowed via:

```sh
cd rpi/workplacesim
cargo run --features desktop --no-default-features --bin workplacesim
# or with seeded demo sims:
cargo run --features desktop --no-default-features --bin workplacesim -- --demo 3
```

See `rpi/workplacesim/deploy/README.md` for Pi config knobs
(`framebuffer_depth=32`, getty disable, logs, uninstall, non-root
operation).

## Smoke test (single agent, no plugin)

```sh
curl -XPOST http://localhost:4317/hooks/subagent-start -H content-type:application/json -d '{
  "agent_id":"a1","session_id":"s1","agent_type":"Explore","cwd":"/tmp",
  "user":"alice","host":"laptop","description":"Find API endpoints"
}'
sleep 4
curl -XPOST http://localhost:4317/hooks/subagent-stop -H content-type:application/json -d '{
  "agent_id":"a1","last_assistant_message":"done"
}'
```

## HTTP endpoints

All `POST`, all fire-and-forget (204 on success, silent drop on unknown
session/agent):

| Endpoint | Source | Effect |
|---|---|---|
| `/hooks/pretool` | `PreToolUse(Agent)` buffer step | Stores subagent description by `(session_id, subagent_type)` for later `startAgent` lookup. |
| `/hooks/subagent-start` | `SessionStart` / `PreToolUse(Agent)` | Registers a new agent record; broadcasts `start`. |
| `/hooks/subagent-stop` | `SessionEnd` / `SubagentStop` | Retires an agent; broadcasts `stop` (10 s grace). |
| `/hooks/tool-event` | `PreToolUse(*)` | Broadcasts `tool` (drives motes); also compares `permission_mode` → `reclassify` on change. |
| `/hooks/lab-visit` | `PostToolUse(Bash)` test-runner regex, `PreToolUse(Edit)` test-path regex | Broadcasts `visit` with a TTL (1–120 s, clamped); frontend walks the sim to the lab temporarily. |
| `/hooks/lifecycle` | `UserPromptSubmit`, `Notification`, `Stop`, `PostToolUse(*)`, plus extra emits from `PostToolUse(Bash)` and `PreToolUse(Edit)` | Dispatches by `kind`: `prompt` / `idle` / `turn-end` / `file-touch` / `bash-result` / `tool-error`. |
| `/api/agents` (GET) | — | JSON dump of currently active agents. |
| `/events` (GET) | — | SSE stream of all broadcasts. |

## SSE event types

`snapshot`, `start`, `stop`, `tool`, `visit`, `reclassify`, `prompt`, `idle`,
`turn-end`, `file-touch`, `bash-result`, `tool-error`. Each payload is flat
JSON keyed by `type` plus `agent_id` and a handful of event-specific fields.

## Notes / limitations

- State is in-memory; restarting the server clears active sims.
- Real Claude Code's `SubagentStop` gives a different `agent_id` than
  dispatch. The backend falls back to FIFO matching by
  `(session_id, agent_type)`; for parallel subagents of the same type this is
  best-effort.
- Capacity: 12 desks, 4 meeting seats, 3 lab stations; overflow queues stand
  against the wall.
- Lab visits assume a single-room "test" target. The primitive is generalized
  in the server (`VISIT_ROOMS = {test, meeting, desk}`) but only `test` is
  currently driven by hooks.
