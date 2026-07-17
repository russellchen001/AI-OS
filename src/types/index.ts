export type ServiceStatus =
  | "Running"
  | "Stopped"
  | "Unknown";

export type ServiceName =
  | "OpenClaw"
  | "Ollama"
  | "Docker"
  | "Open WebUI"
  | "Cherry Studio";

export type Service = {
  name: ServiceName;
  icon: string;
  description: string;
  status: ServiceStatus;
};

export type PageName =
  | "Dashboard"
  | "Services"
  | "OpenClaw"
  | "Backup"
  | "Logs"
  | "Models"
  | "MCP"
  | "MultiLLM"
  | "Prompt Library"
  | "Artifacts"
  | "Settings";

export type ThemeMode =
  | "dark"
  | "light";

export type Settings = {
  refreshInterval: number;
  openClawUrl: string;
  ollamaUrl: string;
  openWebUiUrl: string;
  theme: ThemeMode;
  backupDirectory: string;
  includeOpenClawConfig: boolean;
  includeAiOsSettings: boolean;
  logLineLimit: number;
};

export type Metrics = {
  cpu: number;
  memoryUsed: number;
  memoryTotal: number;
  diskUsed: number;
  diskTotal: number;
};

export type BackupStatus =
  | "idle"
  | "creating"
  | "cancelling"
  | "restoring"
  | "success"
  | "error";

export type BackupRecord = {
  id: string;
  fileName: string;
  path: string;
  createdAt: string;
  sizeBytes: number;
};

export type CreateBackupRequest = {
  operationId: string;
  destinationDirectory: string;
  includeOpenClawConfig: boolean;
  includeAiOsSettings: boolean;
  settingsJson: string;
};

export type RestoreBackupRequest = {
  archivePath: string;
  restoreOpenClawConfig: boolean;
  restoreAiOsSettings: boolean;
};

export type BackupResult = {
  success: boolean;
  message: string;
  archivePath?: string;
  restoredSettingsJson?: string;
};

export type LogLevel =
  | "debug"
  | "info"
  | "warning"
  | "error";

export type LogSource =
  | "AI OS"
  | ServiceName;

export type LogEntry = {
  id: string;
  timestamp: string;
  level: LogLevel;
  source: LogSource;
  message: string;
};

export type LogQuery = {
  source?: LogSource | "All";
  level?: LogLevel | "All";
  limit: number;
};

export type OllamaModel = {
  name: string;
  model: string;
  size: number;
  digest: string;
  modifiedAt: string;

  details?: {
    format?: string;
    family?: string;
    families?: string[];
    parameterSize?: string;
    quantizationLevel?: string;
  };
};

export type OllamaPullProgress = {
  status: string;
  digest?: string;
  total?: number;
  completed?: number;
};

export type ModelActionStatus =
  | "idle"
  | "loading"
  | "pulling"
  | "deleting"
  | "running"
  | "error";

export type McpTransport =
  | "stdio"
  | "http"
  | "sse";

export type McpServer = {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  transport: McpTransport;
  command?: string;
  args: string[];
  url?: string;
  environment: Record<string, string>;
};

export type McpServerInput = {
  name: string;
  description: string;
  enabled: boolean;
  transport: McpTransport;
  command?: string;
  args: string[];
  url?: string;
  environment: Record<string, string>;
};

export type McpActionResult = {
  success: boolean;
  message: string;
  server?: McpServer;
};

/* ===========================
   OpenClaw
=========================== */

export type OpenClawConnectionState =
  | "unknown"
  | "testing"
  | "connected"
  | "unauthorized"
  | "unreachable"
  | "pairing-required"
  | "error";

export type OpenClawMode =
  | "local"
  | "remote";

export type OpenClawServer = {
  id: string;
  name: string;
  serverUrl: string;

  /*
   * 后端只能返回掩码或空字符串，
   * 不能把真实 Token 返回到前端。
   */
  gatewayToken: string;
  hasGatewayToken: boolean;

  enabled: boolean;
  active: boolean;
  autoConnect: boolean;

  connectionState: OpenClawConnectionState;
  connectionMessage: string;

  version?: string;
  gatewayId?: string;
  latencyMs?: number;
  lastCheckedAt?: string;
  createdAt?: string;
  updatedAt?: string;
};

export type OpenClawServerInput = {
  name: string;
  serverUrl: string;

  /*
   * 新建时必须填写。
   * 编辑时留空表示保留后端现有 Token。
   */
  gatewayToken: string;

  enabled: boolean;
  autoConnect: boolean;
};

export type OpenClawActionResult = {
  success: boolean;
  message: string;
  server?: OpenClawServer;
};

export type OpenClawConnectionResult = {
  success: boolean;
  state: OpenClawConnectionState;
  message: string;
  checkedAt: string;

  version?: string;
  gatewayId?: string;
  latencyMs?: number;

  server?: OpenClawServer;
};

export type OpenClawRemoteStatus = {
  connected: boolean;

  serverId: string;
  serverName: string;
  serverUrl: string;

  gatewayStatus?: OpenClawConnectionState;
  version?: string;
  gatewayId?: string;
  latencyMs?: number;
  checkedAt?: string;

  rawResponse?: string;
};

export type OpenClawDashboardSummary = {
  configured: boolean;
  connected: boolean;

  serverId?: string;
  serverName?: string;
  serverUrl?: string;

  state: OpenClawConnectionState;
  message: string;

  version?: string;
  gatewayId?: string;
  latencyMs?: number;
  lastCheckedAt?: string;
};

export type OpenClawRuntimeConfig = {
  mode: OpenClawMode;
  activeServerId?: string;
  activeServer?: OpenClawServer;
};

export type OpenClawExportDocument = {
  schemaVersion: 1;
  exportedAt: string;

  /*
   * 默认导出不应包含 Token。
   * includeSecrets=true 时才允许带 Token。
   */
  includesSecrets: boolean;
  servers: OpenClawServerInput[];
};

export type OpenClawExportResult = {
  success: boolean;
  message: string;
  json?: string;
};

export type OpenClawImportResult = {
  success: boolean;
  message: string;
  importedCount: number;
  skippedCount: number;
};

export type OpenClawGatewayRequest = {
  method: string;
  params?: Record<string, unknown>;
};

export type OpenClawGatewayResponse<T = unknown> = {
  success: boolean;
  message: string;
  data?: T;
  rawResponse?: string;
};

/* ===========================
   Shared
=========================== */

export type AppInfo = {
  name: string;
  version: string;
  description: string;
  identifier: string;
};

export type AsyncStatus =
  | "idle"
  | "loading"
  | "success"
  | "error";