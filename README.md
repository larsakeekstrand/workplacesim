# workplacesim

A top-down, game-style visualization of Claude Code subagent activity.
Each running subagent is a sim that walks into a room, sits at a desk with a
label showing the Claude user and the agent task, and walks out when the
subagent finishes. Driven by Claude Code hooks that POST to a local REST
backend; the browser subscribes to a Server-Sent Events stream.

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

The hook side ships as a Claude Code plugin in `plugin/`. Install it from
this repo as a one-plugin marketplace:

```
/plugin marketplace add /absolute/path/to/workplacesim
/plugin install workplacesim
```

Restart Claude Code after installing so the hooks wire up for the session.
The plugin registers a `PreToolUse` hook (matcher `Agent`) and a
`SubagentStop` hook. From any Claude Code session you can run
`/workplacesim` for a quick status check on the visualizer server.

To point the plugin at a visualizer running on another host, set
`WORKPLACESIM_URL=http://<host>:4317` in the environment Claude Code runs
in (e.g. your shell profile).

See `plugin/README.md` for the plugin-specific details.

## Hook simulator (no real Claude needed)

```sh
npm run simulate
# flags: --rate=1500 --max-concurrent=8 --min-duration=4000 --max-duration=18000 --total=20 --plan-ratio=0.25 --url=http://...
```

Generates fake start + stop traffic with random users, hosts, agent types,
and descriptions so you can watch the rooms fill up. `--plan-ratio` (0..1)
sets the fraction of fake agents that run in plan mode. Ctrl+C to stop.

## Routing rules

Each agent is sorted into one of three rooms when it starts. Priority:

1. **Test lab** — if `agent_type` or `description` contains `"test"`
   (case-insensitive). Sim gets a `🧪` badge, walks through the lab door,
   and sits at a workbench station with a scope and test rig.
2. **Meeting room** — `permission_mode === "plan"`. Sim gets a `📋` badge,
   walks through the meeting door, and takes a chair at the conference
   table with the whiteboard.
3. **Open plan** — everyone else; assigned the first free desk.

The classification is captured once at agent start. Mid-run changes to mode
or description are not reflected.

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

Watch a sim walk in, sit, then walk out.

## Notes / limitations

- State is in-memory; restarting the server clears active sims.
- Real Claude Code hooks give different IDs at dispatch vs stop. The
  backend falls back to FIFO matching by `(session_id, agent_type)` on
  `SubagentStop` when the `agent_id` doesn't match. For parallel subagents
  of the same type inside a single session, this is best-effort.
- 12 desks in the open-plan room + 4 meeting seats + 3 lab stations;
  additional concurrent agents queue against the wall.
