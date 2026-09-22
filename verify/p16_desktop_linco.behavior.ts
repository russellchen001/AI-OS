import { handleDesktopLincoInbound } from "../src/services/desktopLincoInbound";
import type { RemoteAiOsEvent } from "../src/types/remoteInteraction";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const received: unknown[] = [];
const result: RemoteAiOsEvent[] = [{
  type: "cancelled",
  sessionKey: "linco-live",
  conversationId: "conversation-1",
  occurredAt: 1,
  message: "Nothing to cancel.",
}];
const events = await handleDesktopLincoInbound(
  { requestId: "request-1", envelope: { type: "stop_turn", sessionKey: "linco-live" } },
  { handleInbound: async (value) => { received.push(value); return result; } },
);
assert(received.length === 1, "desktop inbound did not enter the existing Remote adapter");
assert(events === result, "desktop response did not preserve normalized Remote events");

let rejected = false;
try {
  await handleDesktopLincoInbound(
    { requestId: "", envelope: { type: "stop_turn", sessionKey: "linco-live" } },
    { handleInbound: async () => result },
  );
} catch {
  rejected = true;
}
assert(rejected, "malformed desktop request id was accepted");

console.log("PASS P16 Desktop Linco inbound behavior");
