export type ServiceStatus =
  | "Running"
  | "Stopped"
  | "Unknown";

export type Service = {
  name: string;
  icon: string;
  description: string;
  status: ServiceStatus;
};

export type PageName =
  | "Dashboard"
  | "Services"
  | "Backup"
  | "Logs"
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
};

export type Metrics = {
  cpu: number;
  memoryUsed: number;
  memoryTotal: number;
  diskUsed: number;
  diskTotal: number;
};