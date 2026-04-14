import express from "express";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  broadcastToolEvent,
  bufferDescription,
  handleLifecycle,
  listActive,
  startAgent,
  stopAgent,
  subscribe,
  visitRoom,
} from "./state.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const publicDir = path.resolve(__dirname, "..", "public");

const PORT = Number(process.env.PORT) || 4317;
const HOST = process.env.HOST || "127.0.0.1";

const app = express();

app.use(express.json({ limit: "256kb" }));
app.use((req, res, next) => {
  res.setHeader("Access-Control-Allow-Origin", "*");
  res.setHeader("Access-Control-Allow-Headers", "content-type");
  res.setHeader("Access-Control-Allow-Methods", "GET,POST,OPTIONS");
  if (req.method === "OPTIONS") return res.sendStatus(204);
  next();
});

app.post("/hooks/pretool", (req, res) => {
  const body = req.body || {};
  if (body.tool_name === "Agent" && body.tool_input) {
    bufferDescription({
      session_id: body.session_id,
      subagent_type: body.tool_input.subagent_type,
      description: body.tool_input.description,
      tool_use_id: body.tool_use_id,
    });
  }
  res.sendStatus(204);
});

app.post("/hooks/subagent-start", (req, res) => {
  startAgent(req.body || {});
  res.sendStatus(204);
});

app.post("/hooks/subagent-stop", (req, res) => {
  stopAgent(req.body || {});
  res.sendStatus(204);
});

app.post("/hooks/lab-visit", (req, res) => {
  visitRoom(req.body || {});
  res.sendStatus(204);
});

app.post("/hooks/tool-event", (req, res) => {
  broadcastToolEvent(req.body || {});
  res.sendStatus(204);
});

app.post("/hooks/lifecycle", (req, res) => {
  handleLifecycle(req.body || {});
  res.sendStatus(204);
});

app.get("/api/agents", (_req, res) => {
  res.json({ agents: listActive() });
});

app.get("/events", (req, res) => {
  res.setHeader("Content-Type", "text/event-stream");
  res.setHeader("Cache-Control", "no-cache, no-transform");
  res.setHeader("Connection", "keep-alive");
  res.setHeader("X-Accel-Buffering", "no");
  res.flushHeaders?.();

  const keepAlive = setInterval(() => res.write(": ping\n\n"), 25_000);
  const unsubscribe = subscribe(res);

  req.on("close", () => {
    clearInterval(keepAlive);
    unsubscribe();
  });
});

app.use(express.static(publicDir));

app.listen(PORT, HOST, () => {
  console.log(`workplacesim listening on http://${HOST}:${PORT}`);
});
