import { LincoBridgeAdapter } from "../src/services/integrations/lincoBridgeAdapter";
import { LincoRemoteBridge } from "../src/services/lincoRemoteBridge";
import type { RemoteInboundEvent } from "../src/types/remoteInteraction";

declare const process: { exitCode?: number };

const adapter = new LincoBridgeAdapter("http://127.0.0.1:3300");
const probe = await adapter.probe();

if (probe.status !== "available") {
  console.log("SKIP: local Linco service is not reachable at 127.0.0.1:3300.");
  process.exitCode = 2;
} else {
  console.log("PASS: Linco reachable and visitor/session auth works.");
  const created = await adapter.createConversation("openclaw", {
    tempSession: true,
    title: "AI-OS P16 Remote Transport Validation",
  }) as { sessionId?: unknown };
  if (typeof created.sessionId !== "string" || !created.sessionId) {
    throw new Error("Linco did not return a transport session id.");
  }
  const sessions = await adapter.listSessions();
  if (!sessions.some((session) => session.id === created.sessionId)) {
    throw new Error("Created Linco transport session was not visible to the same visitor.");
  }
  await adapter.resumeSession(created.sessionId);
  const messages = await adapter.listMessages(created.sessionId);
  if (messages.length !== 0) {
    throw new Error("Transport-only validation unexpectedly created an Agent message.");
  }

  let bound: RemoteInboundEvent | undefined;
  const bridge = new LincoRemoteBridge(
    undefined,
    { receive: async (event) => { bound = event; return []; } },
    adapter,
  );
  await bridge.handleInbound({
    type: "stop_turn",
    sessionKey: created.sessionId,
  });
  if (bound?.type !== "stop_turn" || bound.sessionKey !== created.sessionId) {
    throw new Error("AI-OS remote adapter did not bind Linco transport session metadata.");
  }
  console.log(
    "PASS: Linco session create/list/resume/messages and AI-OS transport binding work without an Agent message.",
  );
}
