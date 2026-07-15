export type ProviderHealth =
  | "unknown"
  | "checking"
  | "ready"
  | "missing-key"
  | "quota"
  | "rate-limited"
  | "busy"
  | "invalid-key"
  | "invalid-model"
  | "offline"
  | "disabled"
  | "error";

export type ProviderRuntime = {
  health: ProviderHealth;
  message: string;
  latencyMs?: number;
  lastCheckedAt?: number;
};

export type RuntimeProvider = {
  id: string;
  enabled: boolean;
  apiKey: string;
};

export function createProviderRuntime(
  provider: RuntimeProvider,
): ProviderRuntime {
  if (!provider.enabled) {
    return {
      health: "disabled",
      message: "Disabled",
    };
  }

  if (
    provider.id !== "ollama" &&
    !provider.apiKey.trim()
  ) {
    return {
      health: "missing-key",
      message: "Missing API key",
    };
  }

  if (provider.id === "ollama") {
    return {
      health: "unknown",
      message: "Local provider not checked",
    };
  }

  return {
    health: "unknown",
    message: "Not checked yet",
  };
}

export function checkingRuntime():
  ProviderRuntime {
  return {
    health: "checking",
    message: "Checking provider",
    lastCheckedAt: Date.now(),
  };
}

export function readyRuntime(
  latencyMs?: number,
): ProviderRuntime {
  return {
    health: "ready",
    message: "Provider responded successfully",
    latencyMs,
    lastCheckedAt: Date.now(),
  };
}

export function classifyProviderError(
  error: unknown,
): ProviderRuntime {
  const message = String(error);
  const normalized =
    message.toLowerCase();

  if (
    normalized.includes("402") ||
    normalized.includes(
      "insufficient balance",
    ) ||
    normalized.includes(
      "insufficient_balance",
    ) ||
    normalized.includes(
      "insufficient quota",
    ) ||
    normalized.includes(
      "insufficient_quota",
    ) ||
    normalized.includes(
      "credit balance",
    )
  ) {
    return {
      health: "quota",
      message:
        "Insufficient quota or balance",
      lastCheckedAt: Date.now(),
    };
  }

  if (
    normalized.includes("401") ||
    normalized.includes("403") ||
    normalized.includes("unauthorized") ||
    normalized.includes("invalid api key") ||
    normalized.includes("invalid_api_key") ||
    normalized.includes("authentication") ||
    normalized.includes("permission denied")
  ) {
    return {
      health: "invalid-key",
      message: "Invalid API key",
      lastCheckedAt: Date.now(),
    };
  }

  if (
    normalized.includes("404") ||
    normalized.includes("model not found") ||
    normalized.includes("model_not_found") ||
    normalized.includes("unknown model") ||
    normalized.includes("invalid model") ||
    normalized.includes("does not exist")
  ) {
    return {
      health: "invalid-model",
      message: "Invalid or unavailable model",
      lastCheckedAt: Date.now(),
    };
  }

  if (
    normalized.includes("503") ||
    normalized.includes(
      "service unavailable",
    ) ||
    normalized.includes(
      "high demand",
    ) ||
    normalized.includes(
      "temporarily unavailable",
    ) ||
    normalized.includes(
      '"status":"unavailable"',
    ) ||
    normalized.includes(
      '"status": "unavailable"',
    )
  ) {
    return {
      health: "busy",
      message:
        "Provider is temporarily busy",
      lastCheckedAt: Date.now(),
    };
  }

  if (
    normalized.includes("429") ||
    normalized.includes("rate limit")
  ) {
    return {
      health: "rate-limited",
      message: "Rate limit exceeded",
      lastCheckedAt: Date.now(),
    };
  }

  if (
    normalized.includes(
      "connection refused",
    ) ||
    normalized.includes("network") ||
    normalized.includes("offline") ||
    normalized.includes("timeout") ||
    normalized.includes("timed out") ||
    normalized.includes(
      "failed to connect",
    )
  ) {
    return {
      health: "offline",
      message: "Provider unavailable",
      lastCheckedAt: Date.now(),
    };
  }

  return {
    health: "error",
    message:
      message.length > 240
        ? `${message.slice(0, 240)}…`
        : message,
    lastCheckedAt: Date.now(),
  };
}

export function canRouteToProvider(
  runtime:
    | ProviderRuntime
    | undefined,
): boolean {
  if (!runtime) {
    return true;
  }

  return ![
    "missing-key",
    "quota",
    "busy",
    "invalid-key",
    "invalid-model",
    "offline",
    "disabled",
    "error",
  ].includes(runtime.health);
}

export function healthLabel(
  runtime:
    | ProviderRuntime
    | undefined,
): string {
  switch (runtime?.health) {
    case "checking":
      return "Checking";
    case "ready":
      return "Ready";
    case "missing-key":
      return "Missing key";
    case "quota":
      return "No quota";
    case "rate-limited":
      return "Rate limited";
    case "busy":
      return "Busy";
    case "invalid-key":
      return "Invalid key";
    case "invalid-model":
      return "Invalid model";
    case "offline":
      return "Offline";
    case "disabled":
      return "Disabled";
    case "error":
      return "Error";
    default:
      return "Unknown";
  }
}
