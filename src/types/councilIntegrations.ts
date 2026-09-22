export type CouncilIntegrationId =
  "paperclip" | "agency-agents" | "linco-bridge";

export type CouncilIntegrationStatus = "available" | "unavailable" | "error";

export type CouncilIntegrationProbe = {
  id: CouncilIntegrationId;
  name: string;
  status: CouncilIntegrationStatus;
  version?: string;
  detail: string;
  checkedAt: string;
  metadata?: Record<string, unknown>;
};

export type PaperclipCompany = {
  id: string;
  name: string;
  raw: Record<string, unknown>;
};

export type AgencyAgentDefinition = {
  slug: string;
  division: string;
  path: string;
};

export type AgencyAgentsDivision = {
  id: string;
  name: string;
  description?: string;
  raw: Record<string, unknown>;
};

export type AgencyAgentsRunbook = {
  id: string;
  name: string;
  mode?: string;
  agents: string[];
  raw: Record<string, unknown>;
};

export type AgencyAgentsCatalog = {
  commit: string;
  divisions: AgencyAgentsDivision[];
  runbooks: AgencyAgentsRunbook[];
  agents: AgencyAgentDefinition[];
};

export type LincoBridgeSession = {
  id: string;
  raw: Record<string, unknown>;
};
