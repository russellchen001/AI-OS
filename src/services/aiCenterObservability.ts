import { recordAnalyticsEvent } from "./analytics";
import { calculateCost } from "./pricing";

export type AiCenterExecutionSource = "local" | "cloud";
export type AiCenterRouteMode = "auto" | "manual";
export type AiCenterAttemptOutcome = "success" | "failed" | "cancelled";

export type AiCenterAttempt = {
  providerId: string;
  providerInstanceId: string;
  modelId: string;
  source: AiCenterExecutionSource;
  startedAt: string;
  completedAt: string;
  latencyMs: number;
  outcome: AiCenterAttemptOutcome;
  errorCategory?: string;
};

export type AiCenterInvocationMetadata = {
  invocationId: string;
  routeMode: AiCenterRouteMode;
  providerId: string;
  providerInstanceId: string;
  modelId: string;
  source: AiCenterExecutionSource;
  startedAt: string;
  completedAt: string;
  latencyMs: number;
  inputTokens: number;
  outputTokens: number;
  tokenAccuracy: "estimated";
  estimatedCostUsd?: number;
  pricingMatched: boolean;
  fallbackOccurred: boolean;
  attempts: AiCenterAttempt[];
};

export type ObservableModelChoice = {
  providerId: string;
  providerInstanceId: string;
  modelId: string;
};

export function executionSource(
  choice: ObservableModelChoice,
): AiCenterExecutionSource {
  return choice.providerId === "ollama" ||
    choice.providerInstanceId === "ollama-local"
    ? "local"
    : "cloud";
}

export function estimateTokens(text: string): number {
  return Math.max(0, Math.ceil(text.length / 4));
}

export function safeAttemptError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  if (/cancel/i.test(message)) return "cancelled";
  if (/401|403|reconnect|credential|auth/i.test(message)) return "authentication";
  if (/429|rate limit|busy/i.test(message)) return "rate-limited";
  if (/timeout/i.test(message)) return "timeout";
  if (/no text|empty/i.test(message)) return "empty-response";
  if (/connect|reach|network|stream.*interrupt/i.test(message)) return "unavailable";
  return "provider-error";
}

export function shouldContinueAfterAttemptFailure(input: {
  routeMode: AiCenterRouteMode;
  emittedOutput: boolean;
  cancelled: boolean;
}): boolean {
  return input.routeMode === "auto" && !input.emittedOutput && !input.cancelled;
}

export function completeAttempt(
  choice: ObservableModelChoice,
  startedAtMs: number,
  outcome: AiCenterAttemptOutcome,
  error?: unknown,
): AiCenterAttempt {
  const completedAtMs = Date.now();
  return {
    providerId: choice.providerId,
    providerInstanceId: choice.providerInstanceId,
    modelId: choice.modelId,
    source: executionSource(choice),
    startedAt: new Date(startedAtMs).toISOString(),
    completedAt: new Date(completedAtMs).toISOString(),
    latencyMs: Math.max(0, Math.round(completedAtMs - startedAtMs)),
    outcome,
    ...(error === undefined ? {} : { errorCategory: safeAttemptError(error) }),
  };
}

export function buildInvocationMetadata(input: {
  invocationId: string;
  routeMode: AiCenterRouteMode;
  choice: ObservableModelChoice;
  startedAtMs: number;
  promptText: string;
  outputText: string;
  attempts: AiCenterAttempt[];
}): AiCenterInvocationMetadata {
  const completedAtMs = Date.now();
  const inputTokens = estimateTokens(input.promptText);
  const outputTokens = estimateTokens(input.outputText);
  const pricing = calculateCost({
    provider: input.choice.providerId,
    model: input.choice.modelId,
    inputTokens,
    outputTokens,
  });
  return {
    invocationId: input.invocationId,
    routeMode: input.routeMode,
    providerId: input.choice.providerId,
    providerInstanceId: input.choice.providerInstanceId,
    modelId: input.choice.modelId,
    source: executionSource(input.choice),
    startedAt: new Date(input.startedAtMs).toISOString(),
    completedAt: new Date(completedAtMs).toISOString(),
    latencyMs: Math.max(0, Math.round(completedAtMs - input.startedAtMs)),
    inputTokens,
    outputTokens,
    tokenAccuracy: "estimated",
    ...(pricing.matched ? { estimatedCostUsd: pricing.cost } : {}),
    pricingMatched: pricing.matched,
    fallbackOccurred: input.attempts.length > 1,
    attempts: input.attempts,
  };
}

export function recordInvocationSuccess(
  metadata: AiCenterInvocationMetadata,
): void {
  recordAnalyticsEvent({
    module: "provider",
    type: "success",
    title: "AI Center response completed",
    provider: metadata.providerId,
    model: metadata.modelId,
    inputTokens: metadata.inputTokens,
    outputTokens: metadata.outputTokens,
    estimatedCost: metadata.estimatedCostUsd,
    latencyMs: metadata.latencyMs,
    metadata: {
      invocationId: metadata.invocationId,
      routeMode: metadata.routeMode,
      source: metadata.source,
      fallbackOccurred: metadata.fallbackOccurred,
      attemptCount: metadata.attempts.length,
      tokenAccuracy: metadata.tokenAccuracy,
      pricingMatched: metadata.pricingMatched,
    },
  });
}
