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
   OpenClaw remote servers
=========================== */

export type OpenClawConnectionState =
  | "unknown"
  | "testing"
  | "connected"
  | "unauthorized"
  | "unreachable"
  | "error";

export type OpenClawServer = {
  id: string;
  name: string;
  serverUrl: string;

  /*
   * 后端返回时只能是掩码值或空字符串，
   * 不要把真实 Token 发送回前端列表。
   */
  gatewayToken: string;
  hasGatewayToken: boolean;

  enabled: boolean;
  active: boolean;
  autoConnect: boolean;

  connectionState: OpenClawConnectionState;
  connectionMessage: string;
  lastCheckedAt?: string;
};

export type OpenClawServerInput = {
  name: string;
  serverUrl: string;

  /*
   * 新建时填写真实 Token。
   * 编辑时留空表示保留后端已有 Token。
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
  server?: OpenClawServer;
};

export type OpenClawRemoteStatus = {
  connected: boolean;
  serverId: string;
  serverName: string;
  serverUrl: string;
  gatewayStatus?: string;
  version?: string;
  rawResponse?: string;
};

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