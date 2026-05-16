# workplacesim (Claude Code plugin)

Streams Claude Code session + subagent activity to the local **workplacesim**
visualizer server. With the plugin enabled, the office fills up whenever
Claude is running: each session is a sim, subagents tether to their parent
session, and every tool call / plan-mode toggle / file edit / test run shows
up as ambient motion in the scene.

## What it registers

All hooks are background and fire-and-forget — the script curls to a local
HTTP endpoint with a 2 s timeout and exits 0 on anything going wrong, so a
down visualizer never blocks a Claude session.

| Event | Matcher | Purpose |
|---|---|---|
| `SessionStart` | — | Spawn the main session sim. |
| `SessionEnd` | — | Dismiss the main session sim. |
| `PreToolUse` | `Agent` | Spawn a subagent sim with a tether from the session. |
| `PreToolUse` | `Write\|Edit\|MultiEdit` | File-touch ticker; lab visit on test-file paths. |
| `PreToolUse` | *(broad tool list)* | Tool-call motes; permission-mode reclassify. |
| `PostToolUse` | `Bash` | Lab bench monitor flash; lab visit on test-runner commands. |
| `PostToolUse` | *(broad tool list)* | Red halo + `!` glyph on `tool_response.error`. |
| `UserPromptSubmit` | — | Update the meeting-room whiteboard; clear idle. |
| `Notification` | — | `💤` idle glyph on the session sim. |
| `Stop` | — | Head-wag wave on turn end. |
| `SubagentStop` | — | Dismiss the subagent sim. |

**Restart Claude Code after install.** `/reload-plugins` is not enough to
activate new event subscriptions.

One slash command:

- `/workplacesim` — checks if the visualizer server is reachable and lists
  the currently active agents and which room they're in.

## Configuration

Two environment variables, both optional:

- `WORKPLACESIM_URL` — base URL for the visualizer server. Defaults to
  `http://127.0.0.1:4317`. Point it at a LAN address to send events to a
  visualizer running on another machine. For the Raspberry Pi deploy the
  canonical URL is `http://workplacesim.local:4317` — `rpi/workplacesim/
  deploy/install.sh` sets up the hostname and mDNS advertisement so that
  just works; see `rpi/workplacesim/deploy/README.md` for the flow.
- The hook script also reads `$USER` and `$(hostname -s)` to label your sim.

## Dependencies

- `jq` and `curl` on `$PATH`. If `jq` is missing the hook script exits 0
  silently and no events are sent — your Claude session is unaffected, you
  just won't see sims appear.

## Running the visualizer

This plugin only emits events; the visualizer server lives in the same repo:

```sh
cd workplacesim
npm install
npm start
# open http://localhost:4317
```
