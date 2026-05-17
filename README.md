# workplacesim

> *The idea isn't original — variations on "AI agents as little characters in a office setting" turn up in various corners of the internet. This is my own take on it, wired specifically to Claude Code's hook system.*

A top-down, game-style visualization of Claude Code subagent activity. The
visualizer ingests hook events over HTTP and renders little pixel-art sims
walking into rooms, sitting at desks, running tests, toggling plan mode,
and reacting to every tool call. Procedural pixel-art — no sprite assets.

## Two ways to run the visualizer

The visualizer ships in two implementations. Both speak the same HTTP + SSE
protocol, so the [Claude Code plugin](#install-the-claude-code-plugin) works
against either — pick based on whether you want a dedicated screen or a
debug window.

| | **Path A — Raspberry Pi** _(plugin default)_ | **Path B — Browser (Node)** |
|---|---|---|
| Stack | Rust → `/dev/fb0` over HDMI | Node + Phaser 3 in a browser |
| Where it runs | A Pi 1+ on your LAN | Wherever you run `npm start` |
| Setup time | ~10 min (rpi-imager + install.sh) | ~30 s (`npm install && npm start`) |
| Best for | A monitor on your desk that lights up when Claude works | Developing the visualizer; demos when no Pi handy |
| Plugin `WORKPLACESIM_URL` | unset (defaults to `http://workplacesim.local:4317`) | `http://127.0.0.1:4317` |

Both paths render the same scene from the same protocol. A few ambient
details are currently Node-only — window-light breathing modulated by
event rate, and south-of-sim identity labels — so the browser view is the
more complete reference; see [What you'll see](#what-youll-see).

## Install the Claude Code plugin

The hook side ships as a Claude Code plugin. Install it straight from GitHub:

```
/plugin marketplace add larsakeekstrand/workplacesim
/plugin install workplacesim
```

Pin a branch or tag with `larsakeekstrand/workplacesim@main`. Refresh with
`/plugin marketplace update`.

For a team setup, drop this in `.claude/settings.json` to pre-register the
marketplace so teammates only need `/plugin install workplacesim@workplacesim`:

```json
{
  "extraKnownMarketplaces": {
    "workplacesim": {
      "source": { "source": "github", "repo": "larsakeekstrand/workplacesim" }
    }
  }
}
```

When working on the plugin itself, install from your local checkout instead:

```
/plugin marketplace add /absolute/path/to/workplacesim
/plugin install workplacesim
```

The plugin's hook script needs `jq` and `curl` on `$PATH`. If `jq` is
missing the script exits 0 silently — your Claude session isn't blocked,
you just won't see sims appear.

**Restart Claude Code after installing.** The plugin registers eleven hook
subscriptions across eight Claude Code events; `/reload-plugins` is not
enough to activate new event subscriptions. After restart, `/workplacesim`
checks the visualizer's status.

By default the plugin POSTs to `http://workplacesim.local:4317` — the Pi
running the Rust visualizer, reachable via the in-binary mDNS responder.
For Path B (Node) or any other target, set `WORKPLACESIM_URL`:

```sh
export WORKPLACESIM_URL=http://127.0.0.1:4317   # local Node server
```

See `plugin/README.md` for plugin-specific details.

## Path A — Raspberry Pi

The Rust port at `rpi/workplacesim/` renders the scene directly to
`/dev/fb0` on a Raspberry Pi 1 (ARMv6) — no browser, no X, no Node. Plug
the Pi into a TV via HDMI, deploy the binary as a systemd service, and the
plugin reaches it via mDNS with no further config.

### Setup

Flash the SD card with **Raspberry Pi OS Lite** via `rpi-imager`
(https://www.raspberrypi.com/software/). Its Advanced Options dialog
preseeds hostname, authorized SSH key, wifi SSID/PSK, and locale. Then
from this repo on macOS:

```sh
# needs: Docker running, cargo install cross
cd rpi/workplacesim
./deploy/install.sh pi@workplacesim.local
```

Once the deploy finishes the plugin's default `WORKPLACESIM_URL` already
points at `workplacesim.local:4317`, so nothing further needs configuring —
every Claude session reports in immediately.

### How it talks to the plugin

The Rust binary is a drop-in for the Node server: same HTTP+SSE protocol,
same payloads, same `/events` and `/` routes (so a browser at
`http://<pi>:4317/` still shows the Phaser frontend for debug). It also
runs an in-process mDNS responder that announces `_workplacesim._tcp`, so
the Pi is reachable at `<hostname>.local:4317` without `avahi-daemon`.

### Desktop dev mode (macOS, no Pi required)

Same Rust crate runs in a minifb window on your Mac for iteration without
flashing to a Pi:

```sh
cd rpi/workplacesim
cargo run --features desktop --no-default-features --bin workplacesim
# or with seeded demo sims:
cargo run --features desktop --no-default-features --bin workplacesim -- --demo 3
```

### Live tuning

Open `http://<pi>:4317/config` on any device on the same LAN. Motion,
effect density, lifecycle TTLs, and display settings are editable through
the page and persist to disk.

See `rpi/workplacesim/deploy/README.md` for the full rpi-imager walkthrough,
the one-time Pi setup (passwordless sudo, `framebuffer_depth=32`, getty
disable), deploy flags, troubleshooting, and uninstall.

## Path B — Browser (Node + Phaser)

The Node server + Phaser 3 frontend renders the scene in a browser. Quicker
to spin up than the Pi path; the right choice for developing the visualizer
itself or running a demo from a laptop without dedicated hardware.

### Prerequisites

- Node.js 20+

### Run

```sh
npm install
npm start
# open http://localhost:4317
```

Env vars: `PORT` (default `4317`), `HOST` (default `127.0.0.1`; set to
`0.0.0.0` to accept hook posts from other machines on the LAN).

### Point the plugin at it

The plugin defaults to the Pi address, so override it for Path B and put
it in your shell rc:

```sh
export WORKPLACESIM_URL=http://127.0.0.1:4317
```

## Hook simulator (no real Claude needed)

Drives synthetic events at either visualizer — useful for iterating on the
frontend without a live Claude session.

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

By default the simulator targets `http://127.0.0.1:4317`. Point it
elsewhere with `WORKPLACESIM_URL=http://<host>:4317 npm run simulate` —
e.g. against the Pi to exercise the Rust render path.

The simulator runs one persistent session sim plus a rolling set of
subagents, all sharing a session_id so tether lines render correctly.
Every feature surface is exercised. Ctrl+C to stop — a
`SessionEnd`-equivalent clears the session sim cleanly.

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
- **Window-light breathing** _(Node only)_ — the exterior windows brighten
  with recent event rate; the office visibly inhales on activity bursts
  and settles on idle.
- **Tool-call motes** — tiny pixel dots drift up from a sim's head on each
  tool call, color-coded by tool family (Read=blue, Write=amber, Bash=green,
  Agent=magenta, Web=purple).
- **Chest labels** — a single character on each sim's torso tying it to a
  Claude Code session. The main session sim and all its subagents share
  the same char; two Claude instances running concurrently get different
  chars, so you can tell which session owns which sims at a glance.

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

## Smoke test (single agent, no plugin)

Works against either visualizer. Substitute `workplacesim.local` for
`localhost` to hit the Pi.

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

The Rust port (Path A) adds `/config`, `/api/config`, `/api/config/bounds`,
`/api/config/reset`, `/api/restart`, and `/api/status` for the live-tuning
page; the Node server (Path B) doesn't expose these.

## SSE event types

`snapshot`, `start`, `stop`, `tool`, `visit`, `reclassify`, `prompt`, `idle`,
`turn-end`, `file-touch`, `bash-result`, `tool-error`. Each payload is flat
JSON keyed by `type` plus `agent_id` and a handful of event-specific fields.

## Notes / limitations

- State is in-memory; restarting either server clears active sims.
- Real Claude Code's `SubagentStop` gives a different `agent_id` than
  dispatch. The backend falls back to FIFO matching by
  `(session_id, agent_type)`; for parallel subagents of the same type this is
  best-effort.
- Capacity: 12 desks, 4 meeting seats, 3 lab stations; overflow queues stand
  against the wall.
- Lab visits assume a single-room "test" target. The primitive is generalized
  in the server (`VISIT_ROOMS = {test, meeting, desk}`) but only `test` is
  currently driven by hooks.
