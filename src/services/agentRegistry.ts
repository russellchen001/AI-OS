import type { AgentRecord } from "../types/agent";

const STORAGE_KEY = "ai-os.agent-registry.v1";

export const OPENCLAW_AGENT: AgentRecord = {
  id: "openclaw",
  name: "OpenClaw",
  description: "Default local execution agent",
  adapterKind: "openclaw-gateway",
  enabled: true,
  isDefault: true,
  builtIn: true,
  capabilities: ["shell", "browser", "files", "network", "skill-execution"],
  permissions: [
    "filesystem.read",
    "filesystem.write",
    "network.access",
    "browser.control",
    "shell.execute",
  ],
  connectionState: "ready",
};

export const HERMES_AGENT_TEMPLATE: AgentRecord = {
  id: "hermes",
  name: "Hermes Agent",
  description: "Optional external execution agent",
  adapterKind: "hermes-api",
  enabled: false,
  isDefault: false,
  builtIn: false,
  capabilities: ["shell", "browser", "files", "network", "skill-execution"],
  permissions: [
    "filesystem.read",
    "filesystem.write",
    "network.access",
    "browser.control",
    "shell.execute",
  ],
  connectionState: "not-configured",
};

function isAgentRecord(value: unknown): value is AgentRecord {
  return Boolean(
    value &&
      typeof value === "object" &&
      typeof (value as AgentRecord).id === "string" &&
      typeof (value as AgentRecord).name === "string" &&
      Array.isArray((value as AgentRecord).capabilities) &&
      Array.isArray((value as AgentRecord).permissions),
  );
}

export function loadAgentRegistry(): AgentRecord[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "[]");
    const customAgents = Array.isArray(parsed) ? parsed.filter(isAgentRecord) : [];
    return [
      OPENCLAW_AGENT,
      ...customAgents.filter((agent) => agent.id !== OPENCLAW_AGENT.id),
    ];
  } catch {
    return [OPENCLAW_AGENT];
  }
}

export function saveCustomAgent(agent: AgentRecord): AgentRecord[] {
  if (agent.builtIn || agent.id === OPENCLAW_AGENT.id) {
    throw new Error("Built-in agents cannot be replaced.");
  }

  const customAgents = loadAgentRegistry().filter(
    (current) => !current.builtIn && current.id !== agent.id,
  );
  const next = [...customAgents, { ...agent, isDefault: false }];
  localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  return [OPENCLAW_AGENT, ...next];
}

export function deleteCustomAgent(agentId: string): AgentRecord[] {
  const customAgents = loadAgentRegistry().filter(
    (agent) => !agent.builtIn && agent.id !== agentId,
  );
  localStorage.setItem(STORAGE_KEY, JSON.stringify(customAgents));
  return [OPENCLAW_AGENT, ...customAgents];
}
