import type {
  Metrics,
  Service,
  Settings,
} from "../types/index";

export const INITIAL_SERVICES: Service[] = [
  {
    name: "OpenClaw",
    icon: "🤖",
    description: "Local AI gateway",
    status: "Unknown",
  },
  {
    name: "Ollama",
    icon: "🦙",
    description: "Local model runtime",
    status: "Unknown",
  },
  {
    name: "Docker",
    icon: "🐳",
    description: "Container runtime",
    status: "Unknown",
  },
  {
    name: "Open WebUI",
    icon: "🌐",
    description: "Browser AI workspace",
    status: "Unknown",
  },
  {
    name: "Cherry Studio",
    icon: "🍒",
    description: "Desktop AI client",
    status: "Unknown",
  },
];

export const DEFAULT_SETTINGS: Settings = {
  refreshInterval: 5,
  openClawUrl: "http://localhost:18789",
  ollamaUrl: "http://localhost:11434",
  openWebUiUrl: "http://localhost:3000",
  theme: "dark",
};

export const EMPTY_METRICS: Metrics = {
  cpu: 0,
  memoryUsed: 0,
  memoryTotal: 0,
  diskUsed: 0,
  diskTotal: 0,
};