import { LincoBridgeAdapter } from "../src/services/integrations/lincoBridgeAdapter";
import type { RemoteAiOsEvent } from "../src/types/remoteInteraction";

declare const process: { env: Record<string, string | undefined> };

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const token = process.env.AI_OS_REMOTE_LINCO_TOKEN;
assert(token, "AI_OS_REMOTE_LINCO_TOKEN is required");
const desktopBase = process.env.AI_OS_REMOTE_LINCO_URL ?? "http://127.0.0.1:39817";
const linco = new LincoBridgeAdapter("http://127.0.0.1:3300");

async function desktopRequest(envelope: Record<string, unknown>): Promise<RemoteAiOsEvent[]> {
  const response = await fetch(`${desktopBase}/v1/linco/inbound`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(envelope),
  });
  const payload = await response.json() as { ok?: unknown; events?: unknown; error?: unknown };
  if (!response.ok || payload.ok !== true || !Array.isArray(payload.events)) {
    throw new Error(`Desktop inbound failed: ${String(payload.error ?? response.status)}`);
  }
  return payload.events as RemoteAiOsEvent[];
}

const probe = await linco.probe();
assert(probe.status === "available", "Linco service is unavailable");
const created = await linco.createConversation("openclaw", {
  tempSession: true,
  title: "AI-OS P16 Desktop Inbound E2E",
}) as { sessionId?: unknown };
assert(typeof created.sessionId === "string" && created.sessionId, "Linco session creation failed");

const unauthorized = await fetch(`${desktopBase}/health`);
assert(unauthorized.status === 401, "Desktop endpoint accepted an unauthenticated request");
const malformed = await fetch(`${desktopBase}/v1/linco/inbound`, {
  method: "POST",
  headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
  body: JSON.stringify({ type: "inbound_message", sessionKey: created.sessionId, command: "openclaw" }),
});
assert(malformed.status === 400, "Desktop endpoint accepted an executable or malformed envelope");

const messageId = `desktop-ask-${Date.now()}`;
const askPromise = desktopRequest({
  type: "inbound_message",
  sessionKey: created.sessionId,
  messageId,
  text: "Reply briefly: P16 desktop Linco ASK reached AI-OS.",
  mode: "auto",
});
await new Promise((resolve) => setTimeout(resolve, 12_000));
const stopped = await desktopRequest({ type: "stop_turn", sessionKey: created.sessionId });
const ask = await askPromise;
const ready = ask.find((event) => event.type === "session_ready");
assert(ready?.conversationId, "Desktop ASK did not resolve an AI-OS conversation");
assert(ask.some((event) => event.type === "turn_start"), "Desktop ASK did not enter Remote AI-OS orchestration");
assert(ask.some((event) => event.type === "cancelled" || event.type === "completed" || event.type === "failed"), "Desktop ASK returned no terminal state");
assert(ask.some((event) => event.type === "turn_end"), "Desktop ASK did not close the remote turn");
assert(stopped[0]?.conversationId === ready.conversationId, "stop_turn lost session continuity");
assert(stopped.some((event) => event.type === "cancelled"), "stop_turn did not reach AI-OS cancellation semantics");

const replay = await desktopRequest({
  type: "inbound_message",
  sessionKey: created.sessionId,
  messageId,
  text: "Reply briefly: P16 desktop Linco ASK reached AI-OS.",
  mode: "auto",
});
assert(replay[0]?.conversationId === ready.conversationId, "Reconnect created a duplicate AI-OS conversation");
assert(replay.some((event) => event.type === "error"), "Duplicate remote message was not rejected");

console.log(`PASS: Desktop Linco live ASK session=${created.sessionId} conversation=${ready.conversationId}`);
console.log("PASS: auth rejection, malformed rejection, response path, reconnect, replay rejection and stop_turn");
