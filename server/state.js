const PENDING_TTL_MS = 60_000;
const STOP_GRACE_MS = 10_000;

const activeAgents = new Map();
const pendingDescriptions = new Map();
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
  } = raw;
  if (!agent_id) return null;
  if (activeAgents.has(agent_id)) return activeAgents.get(agent_id);

  const description = consumeDescription(session_id, agent_type) || agent_type || "agent";
  const record = {
    agent_id,
    session_id: session_id ?? null,
    agent_type: agent_type ?? "agent",
    description,
    user: user || "unknown",
    host: host || "",
    cwd: cwd ?? "",
    started_at: Date.now(),
  };
  activeAgents.set(agent_id, record);
  broadcast({ type: "start", agent: record });
  return record;
}

export function stopAgent(raw) {
  const { agent_id, last_assistant_message } = raw;
  if (!agent_id) return null;
  const record = activeAgents.get(agent_id);
  if (!record) return null;
  record.finished_at = Date.now();
  record.last_message = last_assistant_message ?? null;
  broadcast({ type: "stop", agent_id });
  setTimeout(() => activeAgents.delete(agent_id), STOP_GRACE_MS);
  return record;
}

setInterval(() => {
  const cutoff = Date.now() - PENDING_TTL_MS;
  for (const [key, entry] of pendingDescriptions) {
    if (entry.ts < cutoff) pendingDescriptions.delete(key);
  }
}, 30_000).unref();
