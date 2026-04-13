#!/usr/bin/env node
// Fake hook traffic for workplacesim. Posts pretool + subagent-start + subagent-stop
// pairs at a configurable cadence so you can watch the UI without real Claude hooks.

const args = Object.fromEntries(
  process.argv.slice(2).flatMap((a) => {
    const m = a.match(/^--([^=]+)(?:=(.*))?$/);
    return m ? [[m[1], m[2] ?? "true"]] : [];
  })
);

const URL = args.url || process.env.WORKPLACESIM_URL || "http://127.0.0.1:4317";
const RATE_MS = Number(args.rate ?? 1500);
const MIN_DURATION_MS = Number(args["min-duration"] ?? 4000);
const MAX_DURATION_MS = Number(args["max-duration"] ?? 18000);
const MAX_CONCURRENT = Number(args["max-concurrent"] ?? 8);
const TOTAL = args.total ? Number(args.total) : Infinity;

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
  "Add dark mode toggle",
  "Map build pipeline",
  "Locate telemetry config",
  "Draft migration plan",
  "Review recent changes",
  "Trace request path",
  "Audit dependency tree",
  "Summarize recent PRs",
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

let active = 0;
let launched = 0;

async function runAgent() {
  active++;
  const sessionId = id("s");
  const agentId = id("a");
  const agentType = pick(AGENT_TYPES);
  const description = pick(DESCRIPTIONS);
  const user = pick(USERS);
  const host = pick(HOSTS);
  const duration = rand(MIN_DURATION_MS, MAX_DURATION_MS);

  console.log(`▶ ${user}@${host} ${agentType} — ${description} (${Math.round(duration)}ms)`);

  await post("/hooks/pretool", {
    tool_name: "Agent",
    session_id: sessionId,
    tool_use_id: id("tu"),
    tool_input: { subagent_type: agentType, description },
    user,
    host,
  });
  await post("/hooks/subagent-start", {
    agent_id: agentId,
    session_id: sessionId,
    agent_type: agentType,
    cwd: `/Users/${user}/src/demo`,
    user,
    host,
  });

  await new Promise((r) => setTimeout(r, duration));

  await post("/hooks/subagent-stop", {
    agent_id: agentId,
    last_assistant_message: "done",
  });
  console.log(`■ ${user}@${host} ${agentType} stopped`);
  active--;
}

async function tick() {
  if (launched >= TOTAL) {
    if (active === 0) {
      console.log("simulator: all done");
      process.exit(0);
    }
    return;
  }
  if (active < MAX_CONCURRENT) {
    launched++;
    runAgent().catch((e) => console.warn(e));
  }
}

console.log(
  `simulator → ${URL} · rate=${RATE_MS}ms · maxConcurrent=${MAX_CONCURRENT} · duration=${MIN_DURATION_MS}-${MAX_DURATION_MS}ms${TOTAL === Infinity ? "" : ` · total=${TOTAL}`}`
);
setInterval(tick, RATE_MS);
tick();
