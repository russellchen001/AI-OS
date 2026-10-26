import { invoke } from "@tauri-apps/api/core";

export type AgentVersionPlan = {
  agentId: string;
  installedVersion?: string;
  latestVersion: string;
  integrity: string;
  updateAvailable: boolean;
  requiresApproval: true;
};

export type AgentUpgradeResult = {
  success: boolean;
  message: string;
  version?: string;
};

export type AgentLifecycleAdapter = {
  id: string;
  versionManaged: boolean;
  checkVersion?: () => Promise<AgentVersionPlan>;
  upgrade?: (plan: AgentVersionPlan) => Promise<AgentUpgradeResult>;
};

const adapters: AgentLifecycleAdapter[] = [
  {
    id: "openclaw",
    versionManaged: true,
    checkVersion: () => invoke("check_agent_version", { agentId: "openclaw" }),
    upgrade: (plan) => invoke("upgrade_agent", {
      request: {
        agentId: plan.agentId,
        version: plan.latestVersion,
        integrity: plan.integrity,
        userApproved: true,
      },
    }),
  },
  { id: "hermes", versionManaged: false },
];

export function getAgentLifecycleAdapter(agentId: string): AgentLifecycleAdapter | undefined {
  return adapters.find((adapter) => adapter.id === agentId);
}

export function listAgentLifecycleAdapters(): AgentLifecycleAdapter[] {
  return [...adapters];
}
