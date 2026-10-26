import { invoke } from "@tauri-apps/api/core";
import type { ProviderModelEntry } from "../types/provider";

export type CliProviderStatus = {
  installed: boolean;
  authenticated: boolean;
  version?: string;
  message: string;
  models: string[];
  defaultModel?: string;
};

export type CliProviderAdapter = {
  id: "anthropic" | "grok";
  displayName: string;
  officialPackage?: string;
  loginCommand: string;
  probe: () => Promise<CliProviderStatus>;
  models: (instanceId: string, status: CliProviderStatus) => ProviderModelEntry[];
};

function modelEntry(
  instanceId: string,
  remoteModelId: string,
  displayName: string,
  isDefault: boolean,
  capabilities: ProviderModelEntry["capabilities"],
): ProviderModelEntry {
  return {
    id: `${instanceId}:${remoteModelId}`,
    providerInstanceId: instanceId,
    remoteModelId,
    displayName,
    capabilities,
    enabled: true,
    isDefault,
  };
}

const adapters: CliProviderAdapter[] = [
  {
    id: "anthropic",
    displayName: "Claude Code",
    loginCommand: "claude auth login",
    probe: async () => {
      const status = await invoke<Omit<CliProviderStatus, "models">>("get_claude_code_status");
      return { ...status, models: status.authenticated ? ["claude-code-default"] : [] };
    },
    models: (instanceId, status) => status.models.map((model) =>
      modelEntry(instanceId, model, "Claude Code account default", true, ["chat", "reasoning", "vision", "tool-use"])),
  },
  {
    id: "grok",
    displayName: "Grok CLI",
    officialPackage: "@xai-official/grok",
    loginCommand: "grok login",
    probe: () => invoke<CliProviderStatus>("get_grok_cli_status"),
    models: (instanceId, status) => status.models.map((model, index) =>
      modelEntry(
        instanceId,
        model,
        `${model} (Grok CLI)`,
        model === (status.defaultModel ?? status.models[0]) || (index === 0 && !status.defaultModel),
        ["chat", "reasoning"],
      )),
  },
];

export function listCliProviderAdapters(): CliProviderAdapter[] {
  return [...adapters];
}

export function getCliProviderAdapter(providerId: string): CliProviderAdapter | undefined {
  return adapters.find((adapter) => adapter.id === providerId);
}
