const PENDING_TTL_MS = 60_000;
const STOP_GRACE_MS = 10_000;

const VISIT_ROOMS = new Set(["test", "meeting", "desk"]);
const VISIT_MIN_MS = 1_000;
const VISIT_MAX_MS = 120_000;

const activeAgents = new Map();
const pendingDescriptions = new Map();
const visitTimers = new Map();
const subscribers = new Set();

const pendingKey = (sessionId, subagentType) => `${sessionId ?? ""}::${subagentType ?? ""}`;

function broadcast(message) {
  const payload = `data: ${JSON.stringify(message)}\n\n`;
  for (const res of subscribers) {
    try {
      res.write(payload);
    } catch {
      subscribers.delete(res);
    }
  }
}

export function subscribe(res) {
  subscribers.add(res);
  const snapshot = {
    type: "snapshot",
    agents: [...activeAgents.values()].filter((a) => !a.finished_at),
  };
  res.write(`data: ${JSON.stringify(snapshot)}\n\n`);
  return () => subscribers.delete(res);
}

export function listActive() {
  return [...activeAgents.values()].filter((a) => !a.finished_at);
}

export function bufferDescription({ session_id, subagent_type, description, tool_use_id }) {
  if (!subagent_type) return;
  pendingDescriptions.set(pendingKey(session_id, subagent_type), {
    description: description ?? "",
    tool_use_id: tool_use_id ?? null,
    ts: Date.now(),
  });
}

function consumeDescription(session_id, agent_type) {
  const key = pendingKey(session_id, agent_type);
  const entry = pendingDescriptions.get(key);
  if (!entry) return "";
  pendingDescriptions.delete(key);
  if (Date.now() - entry.ts > PENDING_TTL_MS) return "";
  return entry.description || "";
}

export function startAgent(raw) {
  const {
    agent_id,
    session_id,
    agent_type,
    cwd,
    user,
    host,
    permission_mode,
    description: rawDescription,
  } = raw;
  if (!agent_id) return null;
  if (activeAgents.has(agent_id)) return activeAgents.get(agent_id);

  const description =
    rawDescription || consumeDescription(session_id, agent_type) || agent_type || "agent";
  const record = {
    agent_id,
    session_id: session_id ?? null,
    agent_type: agent_type ?? "agent",
    description,
    user: user || "unknown",
    host: host || "",
    cwd: cwd ?? "",
    permission_mode: permission_mode || "default",
    started_at: Date.now(),
  };
  activeAgents.set(agent_id, record);
  broadcast({ type: "start", agent: record });
  return record;
}

export function stopAgent(raw) {
  const { agent_id, session_id, agent_type, last_assistant_message } = raw;

  let record = agent_id ? activeAgents.get(agent_id) : null;
  if (!record && session_id && agent_type) {
    // Real Claude Code payloads: PreToolUse(Agent) gives us tool_use_id, but
    // SubagentStop gives a different agent_id. Fall back to FIFO match on
    // (session_id, agent_type) — oldest unfinished sim with that shape wins.
    for (const r of activeAgents.values()) {
      if (r.finished_at) continue;
      if (r.session_id === session_id && r.agent_type === agent_type) {
        record = r;
        break;
      }
    }
  }

  if (!record) return null;
  record.finished_at = Date.now();
  record.last_message = last_assistant_message ?? null;
  const priorVisit = visitTimers.get(record.agent_id);
  if (priorVisit) {
    clearTimeout(priorVisit);
    visitTimers.delete(record.agent_id);
  }
  broadcast({ type: "stop", agent_id: record.agent_id });
  const targetId = record.agent_id;
  setTimeout(() => activeAgents.delete(targetId), STOP_GRACE_MS);
  return record;
}

export function visitRoom(raw) {
  const { session_id, agent_id, room, ttl_ms } = raw || {};
  if (!VISIT_ROOMS.has(room)) return null;
  const ttl = Math.max(VISIT_MIN_MS, Math.min(VISIT_MAX_MS, Number(ttl_ms) || 20_000));

  let record = agent_id ? activeAgents.get(agent_id) : null;
  if (!record && session_id) record = activeAgents.get(session_id);
  if (!record && session_id) {
    for (const r of activeAgents.values()) {
      if (r.finished_at) continue;
      if (r.session_id === session_id) {
        record = r;
        break;
      }
    }
  }
  if (!record || record.finished_at) return null;

  const now = Date.now();
  const until = Math.max(record.visit?.until ?? 0, now + ttl);
  record.visit = { room, until };
  broadcast({ type: "visit", agent_id: record.agent_id, room, until });

  const prior = visitTimers.get(record.agent_id);
  if (prior) clearTimeout(prior);
  const targetId = record.agent_id;
  const timer = setTimeout(() => {
    visitTimers.delete(targetId);
    const current = activeAgents.get(targetId);
    if (!current || !current.visit) return;
    if (current.visit.until > Date.now()) return;
    current.visit = null;
    broadcast({ type: "visit", agent_id: targetId, room: null });
  }, until - now + 50);
  timer.unref?.();
  visitTimers.set(targetId, timer);
  return record;
}

setInterval(() => {
  const cutoff = Date.now() - PENDING_TTL_MS;
  for (const [key, entry] of pendingDescriptions) {
    if (entry.ts < cutoff) pendingDescriptions.delete(key);
  }
}, 30_000).unref();
