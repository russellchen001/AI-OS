export type AgentId = "openclaw" | "hermes" | (string & {});

export type AgentCapability =
  | "shell"
  | "browser"
  | "files"
  | "network"
  | "devices"
  | "scheduled-tasks"
  | "skill-execution";

export type AgentPermission =
  | "filesystem.read"
  | "filesystem.write"
  | "network.access"
  | "browser.control"
  | "shell.execute"
  | "device.read"
  | "device.control";

export type AgentAdapterKind =
  | "openclaw-gateway"
  | "hermes-api"
  | "custom";

export type AgentRecord = {
  id: AgentId;
  name: string;
  description: string;
  adapterKind: AgentAdapterKind;
  enabled: boolean;
  isDefault: boolean;
  builtIn: boolean;
  capabilities: AgentCapability[];
  permissions: AgentPermission[];
  connectionState: "ready" | "not-configured" | "unavailable";
};

