# workplacesim (Claude Code plugin)

Streams Claude Code subagent activity to the local **workplacesim** visualizer
server. When you dispatch a subagent in any Claude Code session with this
plugin enabled, a sim walks into the visualizer's room and sits at a desk;
when the subagent finishes, the sim leaves.

## What it registers

Three async hooks (all background, never block agent execution):

- `PreToolUse` (matcher: `Agent`) — buffers the agent description from the Task
  call so the visualizer can show it as a label.
- `SubagentStart` — emits the sim "walks in" event.
- `SubagentStop` — emits the sim "walks out" event.

One slash command:

- `/workplacesim` — checks if the visualizer server is reachable and lists the
  current active agents and which room they're in.

## Configuration

Two environment variables, both optional:

- `WORKPLACESIM_URL` — base URL for the visualizer server. Defaults to
  `http://127.0.0.1:4317`. Point it at a LAN address to send events to a
  visualizer running on another machine.
- The hook script also reads `$USER` and `$(hostname -s)` to label your sim.

## Dependencies

- `jq` and `curl` on `$PATH`. If `jq` is missing the hook script exits 0 silently
  and no events are sent — your Claude session is unaffected, you just won't see
  sims appear.

## Running the visualizer

This plugin only emits events; the visualizer server lives in the same repo:

```sh
cd workplacesim
npm install
npm start
# open http://localhost:4317
```
