import { listAgentLifecycleAdapters } from "../src/services/agentLifecycleManager";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const adapters = listAgentLifecycleAdapters();
assert(adapters.map((adapter) => adapter.id).join(",") === "openclaw,hermes", "Agent adapters are not deterministic");
const openclaw = adapters[0];
assert(openclaw.versionManaged && openclaw.checkVersion && openclaw.upgrade, "OpenClaw lifecycle adapter is incomplete");
const hermes = adapters[1];
assert(!hermes.versionManaged, "Hermes must remain an explicit future adapter, not a fake updater");

console.log("PASS: Generic Agent Lifecycle Manager behavior");
