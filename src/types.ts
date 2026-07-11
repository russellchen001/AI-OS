export type ServiceStatus =
  | "Running"
  | "Stopped"
  | "Unknown";

export type PageName =
  | "Dashboard"
  | "Services"
  | "Backup"
  | "Logs"
  | "Settings";

export type LogLevel =
  | "info"
  | "success"
  | "warning"
  | "error";

export type LogFilter = "all" | LogLevel;

export type ThemeMode = "dark" | "light";

export type Service = {
  name: string;
  icon: string;
  description: string;
  status: ServiceStatus;
  canOpen: boolean;
};

export type LogEntry = {
  id: string;
  timestamp: string;
  level: LogLevel;
  message: string;
};

export type Settings = {
  refreshInterval: number;
  theme: ThemeMode;
  openClawUrl: string;
  openWebUiUrl: string;
  ollamaUrl: string;
  autoOpenWebUi: boolean;
};

export type SystemMetrics = {
  cpuUsage: number;
  memoryUsedGb: number;
  memoryTotalGb: number;
  diskUsedGb: number;
  diskTotalGb: number;
};