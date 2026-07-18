export type RuntimeAdapterKind =
  | "openclaw"
  | "ollama"
  | "docker-desktop"
  | "open-webui"
  | "cherry-studio";

export type RuntimePlatform =
  "macos";

export type RuntimeLocation =
  | "local"
  | "remote"
  | "hybrid";

export type RuntimeCapability =
  | "discover"
  | "health"
  | "start"
  | "stop"
  | "restart"
  | "open"
  | "progress"
  | "cancel";

export type RuntimeAvailability =
  | "unknown"
  | "available"
  | "unavailable"
  | "not-installed"
  | "unsupported";

export type RuntimeLifecycle =
  | "unknown"
  | "stopped"
  | "starting"
  | "running"
  | "stopping"
  | "restarting"
  | "failed";

export type RuntimeHealth =
  | "unknown"
  | "checking"
  | "healthy"
  | "degraded"
  | "unhealthy";

export type RuntimeReadiness =
  | "unknown"
  | "ready"
  | "not-ready";

export type RuntimeErrorCode =
  | "authentication-required"
  | "pairing-required"
  | "connection-unavailable"
  | "configuration-unavailable"
  | "invalid-configuration"
  | "probe-failed"
  | "unsupported-platform";

export type NormalizedRuntimeError = {
  code: RuntimeErrorCode;
  message: string;
  retryable: boolean;
};

export type RuntimeDefinition = {
  id: string;
  adapterKind: RuntimeAdapterKind;
  displayKey: string;
  iconKey: string;
  supportedPlatforms: RuntimePlatform[];
  location: RuntimeLocation;
  dependencies: string[];
  capabilities: RuntimeCapability[];
};

export type RuntimeStatus = {
  id: string;
  adapterKind: RuntimeAdapterKind;
  supportedPlatform: RuntimePlatform;
  location: RuntimeLocation;
  dependencies: string[];
  capabilities: RuntimeCapability[];
  availability: RuntimeAvailability;
  lifecycle: RuntimeLifecycle;
  health: RuntimeHealth;
  readiness: RuntimeReadiness;
  observedAt: string;
  error: NormalizedRuntimeError | null;
};

export type RuntimeStatusRequest = {
  ollamaUrl?: string;
  openWebUiUrl?: string;
};
