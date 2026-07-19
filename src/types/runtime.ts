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
  | "unsupported-platform"
  | "runtime-not-found"
  | "operation-not-found"
  | "unsupported-operation"
  | "operation-conflict"
  | "operation-capacity-exceeded"
  | "cancellation-unsupported"
  | "cancellation-too-late"
  | "operation-failed"
  | "operation-task-failed"
  | "dependency-unavailable"
  | "dependency-not-installed"
  | "invalid-runtime-location"
  | "container-not-found"
  | "container-ambiguous"
  | "readiness-timeout";

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
  observedAt: string | null;
  error: NormalizedRuntimeError | null;
};

export type RuntimeStatusRequest = {
  ollamaUrl?: string;
  openWebUiUrl?: string;
};

export type RuntimeOperationAction =
  | "start"
  | "stop"
  | "restart"
  | "open";

export type StartRuntimeOperationRequest = {
  runtimeId: string;
  action: RuntimeOperationAction;
  endpointUrl?: string;
};

export type RuntimeOperationState =
  | "queued"
  | "running"
  | "cancelling"
  | "succeeded"
  | "failed"
  | "cancelled";

export type RuntimeOperationProgress = {
  phase: string;
  completedUnits: number | null;
  totalUnits: number | null;
  message: string;
};

export type RuntimeOperationResult = {
  message: string;
};

export type RuntimeOperationSnapshot = {
  operationId: string;
  runtimeId: string;
  action: RuntimeOperationAction;
  state: RuntimeOperationState;
  revision: number;
  acceptedAt: string;
  startedAt: string | null;
  updatedAt: string;
  completedAt: string | null;
  progress: RuntimeOperationProgress | null;
  cancellable: boolean;
  result: RuntimeOperationResult | null;
  error: NormalizedRuntimeError | null;
};

export type RuntimeOperationAdmission =
  | {
      status: "accepted";
      operation: RuntimeOperationSnapshot;
    }
  | {
      status: "conflict";
      existingOperation: RuntimeOperationSnapshot;
    }
  | {
      status: "rejected";
      error: NormalizedRuntimeError;
    };

export type RuntimeOperationEvent = {
  version: 1;
  operation: RuntimeOperationSnapshot;
};
