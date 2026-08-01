import {
  buildInvocationMetadata,
  completeAttempt,
  estimateTokens,
  executionSource,
  recordInvocationSuccess,
  safeAttemptError,
  shouldContinueAfterAttemptFailure,
  type AiCenterAttempt,
} from "../../src/services/aiCenterObservability";

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(message);
}

function equal(actual: unknown, expected: unknown, message: string): void {
  assert(Object.is(actual, expected), `${message}: expected ${String(expected)}, got ${String(actual)}`);
}

const local = {
  providerId: "ollama",
  providerInstanceId: "ollama-local",
  modelId: "qwen2.5:7b",
};
const cloud = {
  providerId: "openrouter",
  providerInstanceId: "openrouter-account",
  modelId: "provider/model",
};

equal(executionSource(local), "local", "Ollama source");
equal(executionSource(cloud), "cloud", "cloud source");
equal(estimateTokens(""), 0, "empty token estimate");
equal(estimateTokens("12345"), 2, "rounded token estimate");

equal(safeAttemptError(new Error("token=secret-value")), "provider-error", "secret error normalization");
equal(safeAttemptError(new Error("Provider returned 401")), "authentication", "authentication normalization");
equal(safeAttemptError(new Error("rate limited")), "rate-limited", "rate-limit normalization");

equal(
  shouldContinueAfterAttemptFailure({
    routeMode: "auto",
    emittedOutput: false,
    cancelled: false,
  }),
  true,
  "Auto continues before output",
);
equal(
  shouldContinueAfterAttemptFailure({
    routeMode: "auto",
    emittedOutput: true,
    cancelled: false,
  }),
  false,
  "Auto stops after output",
);
equal(
  shouldContinueAfterAttemptFailure({
    routeMode: "manual",
    emittedOutput: false,
    cancelled: false,
  }),
  false,
  "Manual never falls back",
);
equal(
  shouldContinueAfterAttemptFailure({
    routeMode: "auto",
    emittedOutput: false,
    cancelled: true,
  }),
  false,
  "Cancellation never falls back",
);

const failedAttempt = completeAttempt(
  local,
  Date.now() - 10,
  "failed",
  new Error("https://provider.example/callback?token=secret"),
);
const successfulAttempt = completeAttempt(cloud, Date.now() - 5, "success");
equal(failedAttempt.errorCategory, "provider-error", "safe attempt category");
equal(JSON.stringify(failedAttempt).includes("secret"), false, "attempt redaction");
assert(failedAttempt.latencyMs >= 0, "attempt latency must be non-negative");

const attempts: AiCenterAttempt[] = [failedAttempt, successfulAttempt];
const fallback = buildInvocationMetadata({
  invocationId: "test-fallback",
  routeMode: "auto",
  choice: cloud,
  startedAtMs: Date.now() - 20,
  promptText: "hello",
  outputText: "world",
  attempts,
});
equal(fallback.fallbackOccurred, true, "fallback flag");
equal(JSON.stringify(fallback.attempts), JSON.stringify(attempts), "attempt order");
equal(fallback.pricingMatched, false, "unknown pricing match");
equal(fallback.estimatedCostUsd, undefined, "unknown cost omission");
equal(fallback.tokenAccuracy, "estimated", "token accuracy");
assert(fallback.latencyMs >= 0, "invocation latency must be non-negative");

const localSuccess = buildInvocationMetadata({
  invocationId: "test-local",
  routeMode: "manual",
  choice: local,
  startedAtMs: Date.now() - 5,
  promptText: "local prompt",
  outputText: "local response",
  attempts: [completeAttempt(local, Date.now() - 5, "success")],
});
equal(localSuccess.source, "local", "local success source");
equal(localSuccess.fallbackOccurred, false, "local first-attempt fallback flag");
equal(localSuccess.pricingMatched, true, "local pricing match");
equal(localSuccess.estimatedCostUsd, 0, "local estimated cost");
equal(localSuccess.attempts.length, 1, "manual attempt count");

const storage = new Map<string, string>();
Object.defineProperty(globalThis, "localStorage", {
  value: {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
    removeItem: (key: string) => storage.delete(key),
  },
});
Object.defineProperty(globalThis, "window", {
  value: { dispatchEvent: () => true },
});
if (typeof CustomEvent === "undefined") {
  Object.defineProperty(globalThis, "CustomEvent", {
    value: class<T> {
      detail: T | undefined;
      constructor(_name: string, options?: { detail?: T }) {
        this.detail = options?.detail;
      }
    },
  });
}
recordInvocationSuccess(localSuccess);
const events = JSON.parse(storage.get("ai-os.analytics.events.v1") ?? "[]") as Array<{
  provider: string;
  model: string;
  latencyMs: number;
  metadata: Record<string, string | number | boolean>;
}>;
equal(events.length, 1, "one Analytics success event");
equal(events[0]?.provider, "ollama", "Analytics provider");
equal(events[0]?.model, "qwen2.5:7b", "Analytics model");
equal(events[0]?.latencyMs, localSuccess.latencyMs, "Analytics latency");
equal(events[0]?.metadata.routeMode, "manual", "Analytics route mode");
equal(events[0]?.metadata.source, "local", "Analytics source");
equal(events[0]?.metadata.pricingMatched, true, "Analytics pricing state");

console.log("P13-M4 observability tests passed.");
