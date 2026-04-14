#!/usr/bin/env node
// Fake hook traffic for workplacesim. Exercises every current feature:
// session start/end, subagent start/stop, pretool→subagent-start pairing,
// tool-event motes, lifecycle events (prompt / idle / turn-end / file-touch /
// bash-result / tool-error), lab visits, and permission_mode reclassify.
//
// Shares a single session_id across the whole run so every subagent tethers
// back to the main session sim, matching real Claude Code behaviour.

const args = Object.fromEntries(
  process.argv.slice(2).flatMap((a) => {
    const m = a.match(/^--([^=]+)(?:=(.*))?$/);
    return m ? [[m[1], m[2] ?? "true"]] : [];
  })
);

const URL = args.url || process.env.WORKPLACESIM_URL || "http://127.0.0.1:4317";

// Cadences (all in ms). Subagent spawn/rate kept for back-compat.
const SPAWN_RATE_MS = Number(args.rate ?? 1500);
const MIN_DURATION_MS = Number(args["min-duration"] ?? 4000);
const MAX_DURATION_MS = Number(args["max-duration"] ?? 18000);
const MAX_CONCURRENT = Number(args["max-concurrent"] ?? 6);
const TOTAL = args.total ? Number(args.total) : Infinity;
const PLAN_RATIO = Number(args["plan-ratio"] ?? 0.25);

const TOOL_RATE_MS = Number(args["tool-rate"] ?? 800);
const LIFECYCLE_RATE_MS = Number(args["lifecycle-rate"] ?? 2500);
const RECLASSIFY_RATE_MS = Number(args["reclassify-rate"] ?? 25_000);
const LAB_VISIT_RATE_MS = Number(args["lab-visit-rate"] ?? 18_000);

const NO_MAIN = args["no-main-session"] === "true";

const USERS = ["alice", "bob", "carol", "dave", "erin", "frank"];
const HOSTS = ["laptop", "devbox", "m3-max", "studio"];
const AGENT_TYPES = [
  "Explore",
  "Plan",
  "general-purpose",
  "code-reviewer",
  "code-navigator",
  "frontend-design",
];
const DESCRIPTIONS = [
  "Find all API endpoints",
  "Refactor auth middleware",
  "Investigate failing tests",
  "Run the integration test suite",
  "Bisect flaky test",
  "Add dark mode toggle",
  "Map build pipeline",
  "Locate telemetry config",
  "Draft migration plan",
  "Review recent changes",
  "Trace request path",
  "Audit dependency tree",
  "Write tests for cache layer",
  "Summarize recent PRs",
];

const TOOL_NAMES = [
  "Read", "Read", "Read",        // weighted — reads are common
  "Grep", "Grep", "Glob",
  "Edit", "Edit",
  "Write",
  "Bash", "Bash",
  "Agent",
  "WebFetch",
];

const FILE_PATHS = [
  "server/state.js",
  "server/index.js",
  "public/main.js",
  "scripts/simulate.js",
  "plugin/hooks/workplacesim-hook.sh",
  "server/__tests__/state.test.js",
  "tests/routing.test.js",
  "spec/user_spec.rb",
  "src/components/App.tsx",
  "src/utils/date.ts",
  "pkg/handler_test.go",
  "tests/fixtures/session.json",
  "docs/architecture.md",
];

const BASH_COMMANDS = [
  { cmd: "npm test", ok: true },
  { cmd: "npm run build", ok: true },
  { cmd: "pytest -q tests/", ok: true },
  { cmd: "pytest -q tests/", ok: false },
  { cmd: "cargo test --all", ok: false },
  { cmd: "go test ./...", ok: true },
  { cmd: "ls -la", ok: true },
  { cmd: "git status", ok: true },
];

const ERROR_MESSAGES = [
  "ENOENT: no such file or directory",
  "Permission denied",
  "Timeout after 30s",
  "Syntax error near unexpected token",
  "Unable to parse JSON response",
];

const PROMPTS = [
  "Refactor the lab-visit TTL logic to support mid-life extensions.",
  "Trace why the session sim doesn't move when I toggle plan mode.",
  "Review the corridor routing change; look for edge cases.",
  "Draft a migration plan for splitting server/state.js into modules.",
  "Add end-to-end tests for the file-touch ticker.",
  "Explore how hooks.json maps to the plugin runtime.",
];

const pick = (arr) => arr[Math.floor(Math.random() * arr.length)];
const rand = (a, b) => a + Math.random() * (b - a);
const id = (prefix) => `${prefix}_${Math.random().toString(36).slice(2, 10)}`;

async function post(path, body) {
  try {
    const res = await fetch(`${URL}${path}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!res.ok) console.warn(`POST ${path} ${res.status}`);
  } catch (err) {
    console.warn(`POST ${path} failed: ${err.message}`);
  }
}

// Shared session — one per simulator run, same as real Claude Code.
const sessionId = id("s");
const sessionUser = pick(USERS);
const sessionHost = pick(HOSTS);
let sessionMode = "default";

// activeAgents maps agent_id → metadata for still-running subagents. Used
// by the tool-event ticker to pick a random target.
// activeAgents[id] = { agentType, mode, isSession }. The session sim is the
// only entry whose `mode` mutates mid-run (via reclassifyTick); subagents keep
// the mode they were spawned with, mirroring real Claude Code.
const activeAgents = new Map();
activeAgents.set(sessionId, { agentType: "claude", mode: () => sessionMode, isSession: true });

let launched = 0;

async function startSession() {
  if (NO_MAIN) return;
  await post("/hooks/subagent-start", {
    agent_id: sessionId,
    session_id: sessionId,
    agent_type: "claude",
    description: "main session",
    cwd: `/Users/${sessionUser}/src/demo`,
    user: sessionUser,
    host: sessionHost,
    permission_mode: sessionMode,
  });
  console.log(`◆ session ${sessionId.slice(0, 8)} · ${sessionUser}@${sessionHost}`);
}

async function endSession() {
  if (NO_MAIN) return;
  await post("/hooks/subagent-stop", {
    agent_id: sessionId,
    session_id: sessionId,
    agent_type: "claude",
  });
}

async function runAgent() {
  const agentId = id("a");
  const agentType = pick(AGENT_TYPES);
  const description = pick(DESCRIPTIONS);
  const user = pick(USERS);
  const host = pick(HOSTS);
  const duration = rand(MIN_DURATION_MS, MAX_DURATION_MS);
  const planMode = Math.random() < PLAN_RATIO;
  const mode = planMode ? "plan" : "default";
  const tag = planMode ? "▶ [PLAN]" : "▶";

  console.log(`${tag} ${user}@${host} ${agentType} — ${description} (${Math.round(duration)}ms)`);

  activeAgents.set(agentId, { agentType, mode: () => mode, isSession: false });

  await post("/hooks/pretool", {
    tool_name: "Agent",
    session_id: sessionId,
    tool_use_id: id("tu"),
    tool_input: { subagent_type: agentType, description },
    user,
    host,
    permission_mode: mode,
  });
  await post("/hooks/subagent-start", {
    agent_id: agentId,
    session_id: sessionId,
    agent_type: agentType,
    description,
    cwd: `/Users/${user}/src/demo`,
    user,
    host,
    permission_mode: mode,
  });

  await new Promise((r) => setTimeout(r, duration));

  activeAgents.delete(agentId);
  await post("/hooks/subagent-stop", {
    agent_id: agentId,
    session_id: sessionId,
    agent_type: agentType,
    last_assistant_message: "done",
  });
  console.log(`■ ${user}@${host} ${agentType} stopped`);
}

function randomAgentId() {
  const ids = [...activeAgents.keys()];
  return ids[Math.floor(Math.random() * ids.length)];
}

// ---- periodic tickers ----

async function toolTick() {
  const agentId = randomAgentId();
  if (!agentId) return;
  const meta = activeAgents.get(agentId);
  await post("/hooks/tool-event", {
    session_id: sessionId,
    agent_id: agentId,
    tool_name: pick(TOOL_NAMES),
    permission_mode: meta ? meta.mode() : sessionMode,
  });
}

async function lifecycleTick() {
  // Pick a random lifecycle kind, weighted toward the more visible ones.
  const roll = Math.random();
  if (roll < 0.30) {
    // file-touch — ticker on the open-plan wall
    const path = pick(FILE_PATHS);
    await post("/hooks/lifecycle", {
      kind: "file-touch",
      session_id: sessionId,
      agent_id: sessionId,
      path,
      permission_mode: sessionMode,
    });
  } else if (roll < 0.55) {
    // bash-result — lab monitor flash
    const entry = pick(BASH_COMMANDS);
    await post("/hooks/lifecycle", {
      kind: "bash-result",
      session_id: sessionId,
      agent_id: sessionId,
      ok: entry.ok,
      permission_mode: sessionMode,
    });
  } else if (roll < 0.70) {
    // prompt — whiteboard update + clears idle
    await post("/hooks/lifecycle", {
      kind: "prompt",
      session_id: sessionId,
      agent_id: sessionId,
      text: pick(PROMPTS),
      permission_mode: sessionMode,
    });
  } else if (roll < 0.82) {
    // turn-end — head-wag wave
    await post("/hooks/lifecycle", {
      kind: "turn-end",
      session_id: sessionId,
      agent_id: sessionId,
      permission_mode: sessionMode,
    });
  } else if (roll < 0.92) {
    // idle — 💤 glyph, cleared by the next prompt
    await post("/hooks/lifecycle", {
      kind: "idle",
      session_id: sessionId,
      agent_id: sessionId,
      permission_mode: sessionMode,
    });
  } else {
    // tool-error — red halo + `!`
    await post("/hooks/lifecycle", {
      kind: "tool-error",
      session_id: sessionId,
      agent_id: sessionId,
      tool_name: pick(["Read", "Bash", "Edit", "WebFetch"]),
      message: pick(ERROR_MESSAGES),
      permission_mode: sessionMode,
    });
  }
}

async function reclassifyTick() {
  // Toggle plan mode on the session sim.
  sessionMode = sessionMode === "plan" ? "default" : "plan";
  // Send any hook carrying permission_mode; the server detects the change and
  // broadcasts `reclassify`. A cheap prompt is visible and useful.
  await post("/hooks/lifecycle", {
    kind: "prompt",
    session_id: sessionId,
    agent_id: sessionId,
    text:
      sessionMode === "plan"
        ? "Switching to plan mode to think through the approach…"
        : "Plan looks good — executing.",
    permission_mode: sessionMode,
  });
  console.log(`↔ session permission_mode → ${sessionMode}`);
}

async function labVisitTick() {
  // Random 5–20 s lab visit for the session sim.
  const ttl = Math.round(rand(5_000, 20_000));
  await post("/hooks/lab-visit", {
    session_id: sessionId,
    agent_id: sessionId,
    room: "test",
    source: Math.random() < 0.5 ? "bash" : "edit",
    ttl_ms: ttl,
    permission_mode: sessionMode,
  });
  console.log(`🧪 session visits lab for ${ttl}ms`);
}

async function spawnTick() {
  if (launched >= TOTAL) {
    const sub = activeAgents.size - (NO_MAIN ? 0 : 1);
    if (sub === 0) {
      console.log("simulator: all subagents finished");
      await shutdown(0);
    }
    return;
  }
  const subagentCount = activeAgents.size - (NO_MAIN ? 0 : 1);
  if (subagentCount < MAX_CONCURRENT) {
    launched++;
    runAgent().catch((e) => console.warn(e));
  }
}

// ---- shutdown ----

let shuttingDown = false;
async function shutdown(code = 0) {
  if (shuttingDown) return;
  shuttingDown = true;
  await endSession();
  process.exit(code);
}

process.on("SIGINT", () => shutdown(0));
process.on("SIGTERM", () => shutdown(0));

// ---- main ----

console.log(
  `simulator → ${URL}\n` +
    `  spawn ${SPAWN_RATE_MS}ms · concurrency ${MAX_CONCURRENT} · duration ${MIN_DURATION_MS}-${MAX_DURATION_MS}ms\n` +
    `  tools ${TOOL_RATE_MS}ms · lifecycle ${LIFECYCLE_RATE_MS}ms · reclassify ${RECLASSIFY_RATE_MS}ms · lab ${LAB_VISIT_RATE_MS}ms\n` +
    `  plan-ratio ${PLAN_RATIO}${TOTAL === Infinity ? "" : ` · total ${TOTAL}`}${NO_MAIN ? " · no main session" : ""}`
);

(async () => {
  await startSession();
  setInterval(spawnTick, SPAWN_RATE_MS);
  setInterval(toolTick, TOOL_RATE_MS);
  setInterval(lifecycleTick, LIFECYCLE_RATE_MS);
  setInterval(reclassifyTick, RECLASSIFY_RATE_MS);
  setInterval(labVisitTick, LAB_VISIT_RATE_MS);
  spawnTick();
})();
