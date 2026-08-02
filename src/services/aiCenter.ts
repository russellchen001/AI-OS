import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { listProviderInstances } from "./providers";
import {
  buildInvocationMetadata,
  completeAttempt,
  recordInvocationSuccess,
  safeAttemptError,
  shouldContinueAfterAttemptFailure,
  type AiCenterAttempt,
  type AiCenterInvocationMetadata,
} from "./aiCenterObservability";

type AiCenterProviderResponse = {
  providerId: string;
  modelId: string;
  text: string;
};

export type AiCenterResponse = {
  providerId: string;
  modelId: string;
  text: string;
  metadata: AiCenterInvocationMetadata;
};

export type AiCenterModelChoice = {
  providerId: string;
  providerInstanceId: string;
  modelId: string;
  label: string;
};

let localOllamaModels: AiCenterModelChoice[] = [];

export function setLocalOllamaModels(
  models: Array<{ name: string; model: string }>,
): void {
  const next = models.map((model) => ({
    providerId: "ollama",
    providerInstanceId: "ollama-local",
    modelId: model.model,
    label: `Ollama · ${model.name}`,
  }));
  if (JSON.stringify(next) === JSON.stringify(localOllamaModels)) return;
  localOllamaModels = next;
  window.dispatchEvent(new Event("ai-os:providers-changed"));
}

function choiceForModel(
  instance: ReturnType<typeof listProviderInstances>[number],
  model: (typeof instance.models)[number],
): AiCenterModelChoice {
  return {
    providerId: instance.providerId,
    providerInstanceId: instance.id,
    modelId: model.remoteModelId,
    label: `${instance.displayName} · ${model.displayName}`,
  };
}

export function listAiCenterModels(): AiCenterModelChoice[] {
  const connectedModels = listProviderInstances()
    .filter((instance) => instance.connectionState === "connected")
    .flatMap((instance) =>
      instance.models
        .filter((model) => model.enabled)
        .map((model) => choiceForModel(instance, model)),
    );
  return [...connectedModels, ...localOllamaModels];
}

function listAutoCandidates(): AiCenterModelChoice[] {
  const connectedCandidates = listProviderInstances()
    .filter((instance) => instance.connectionState === "connected")
    .flatMap((instance) => {
      const enabled = instance.models.filter((model) => model.enabled);
      const defaultModel = enabled.find((model) => model.isDefault);
      const ordered = defaultModel
        ? [defaultModel, ...enabled.filter((model) => model !== defaultModel)]
        : enabled;
      return ordered.map((model) => choiceForModel(instance, model));
    });
  return [...localOllamaModels, ...connectedCandidates];
}

function resolveChoice(choice: AiCenterModelChoice) {
  if (
    choice.providerId === "ollama" &&
    choice.providerInstanceId === "ollama-local"
  ) {
    const local = localOllamaModels.find(
      (candidate) => candidate.modelId === choice.modelId,
    );
    if (!local) throw new Error("NO_DEFAULT_MODEL");
    return {
      instance: {
        id: "ollama-local",
        providerId: "ollama",
        credential: { kind: "local" as const },
      },
      model: { remoteModelId: local.modelId },
    };
  }

  const instance = listProviderInstances().find(
    (candidate) =>
      candidate.id === choice.providerInstanceId &&
      candidate.providerId === choice.providerId &&
      candidate.connectionState === "connected",
  );
  if (!instance) throw new Error("NO_CONNECTED_PROVIDER");

  const model = instance.models.find(
    (candidate) =>
      candidate.enabled && candidate.remoteModelId === choice.modelId,
  );
  if (!model) throw new Error("NO_DEFAULT_MODEL");
  return { instance, model };
}

async function answerWithChoice(
  prompt: string,
  choice: AiCenterModelChoice,
  operationId?: string,
): Promise<AiCenterProviderResponse> {
  const { instance, model } = resolveChoice(choice);
  if (instance.providerId === "anthropic" && instance.credential.kind === "local") {
    const response = await invoke<{ modelId: string; text: string }>(
      "generate_claude_code_response",
      { input: { operationId, modelId: model.remoteModelId, prompt } },
    );
    return { providerId: instance.providerId, ...response };
  }

  return invoke<AiCenterProviderResponse>("generate_provider_response", {
    input: {
      providerId: instance.providerId,
      providerInstanceId: instance.id,
      modelId: model.remoteModelId,
      prompt,
    },
  });
}

export async function answerThroughAiCenter(
  prompt: string,
  selectedModel?: AiCenterModelChoice,
): Promise<AiCenterResponse> {
  const candidates = selectedModel ? [selectedModel] : listAutoCandidates();
  if (candidates.length === 0) throw new Error("NO_CONNECTED_PROVIDER");

  const invocationId = `invoke-${crypto.randomUUID()}`;
  const invocationStartedAt = Date.now();
  const attempts: AiCenterAttempt[] = [];
  let lastError: unknown;
  for (const candidate of candidates) {
    const attemptStartedAt = Date.now();
    try {
      const response = await answerWithChoice(prompt, candidate);
      attempts.push(completeAttempt(candidate, attemptStartedAt, "success"));
      const metadata = buildInvocationMetadata({
        invocationId,
        routeMode: selectedModel ? "manual" : "auto",
        choice: candidate,
        startedAtMs: invocationStartedAt,
        promptText: prompt,
        outputText: response.text,
        attempts,
      });
      recordInvocationSuccess(metadata);
      return { ...response, metadata };
    } catch (error) {
      attempts.push(completeAttempt(candidate, attemptStartedAt, "failed", error));
      lastError = error;
      if (
        !shouldContinueAfterAttemptFailure({
          routeMode: selectedModel ? "manual" : "auto",
          emittedOutput: false,
          cancelled: false,
        })
      ) {
        throw error;
      }
    }
  }
  throw lastError ?? new Error("NO_CONNECTED_PROVIDER");
}

type StreamEvent = { operationId: string; text: string };
type DoneEvent = { operationId: string; cancelled: boolean };
type ErrorEvent = { operationId: string; message: string };

export type AiCenterStream = {
  operationId: string;
  result: Promise<{ response: AiCenterResponse; cancelled: boolean }>;
  cancel: () => Promise<void>;
};

export type AiCenterConversationMessage = {
  role: "user" | "assistant";
  content: string;
};

export type AiCenterMultiParticipant = {
  participantId: string;
  choice: AiCenterModelChoice;
};

export type AiCenterMultiParticipantResult =
  | {
      participantId: string;
      operationId: string;
      status: "success";
      response: AiCenterResponse;
    }
  | {
      participantId: string;
      operationId: string;
      status: "failed" | "cancelled";
      error: string;
    };

export type AiCenterMultiInvocationResult = {
  invocationId: string;
  startedAt: string;
  completedAt: string;
  latencyMs: number;
  participantCount: number;
  successCount: number;
  failedCount: number;
  cancelledCount: number;
  participants: AiCenterMultiParticipantResult[];
};

export type AiCenterMultiInvocation = {
  invocationId: string;
  result: Promise<AiCenterMultiInvocationResult>;
  cancel: () => Promise<void>;
};

function normalizeMultiParticipants(
  participants: AiCenterMultiParticipant[],
): AiCenterMultiParticipant[] {
  const seenParticipantIds = new Set<string>();
  const seenModels = new Set<string>();
  const normalized: AiCenterMultiParticipant[] = [];

  for (const participant of participants) {
    const participantId = participant.participantId.trim();
    const modelKey = [
      participant.choice.providerId,
      participant.choice.providerInstanceId,
      participant.choice.modelId,
    ].join(":");

    if (
      !participantId ||
      seenParticipantIds.has(participantId) ||
      seenModels.has(modelKey)
    ) {
      continue;
    }

    resolveChoice(participant.choice);
    seenParticipantIds.add(participantId);
    seenModels.add(modelKey);
    normalized.push({
      participantId,
      choice: participant.choice,
    });
  }

  return normalized;
}

export function invokeMultipleThroughAiCenter(
  messages: AiCenterConversationMessage[],
  participants: AiCenterMultiParticipant[],
): AiCenterMultiInvocation {
  const normalized = normalizeMultiParticipants(participants);
  if (normalized.length === 0) {
    throw new Error("NO_CONNECTED_PROVIDER");
  }

  const invocationId = `multi-${crypto.randomUUID()}`;
  const invocationStartedAt = Date.now();
  const activeStreams = new Map<string, AiCenterStream>();
  let cancelled = false;

  const participantResults = Promise.all(
    normalized.map(async (participant) => {
      const stream = streamThroughAiCenter(
        messages,
        participant.choice,
        () => undefined,
      );
      const operationId = stream.operationId;
      activeStreams.set(operationId, stream);

      try {
        if (cancelled) {
          await stream.cancel();
        }

        const outcome = await stream.result;
        if (cancelled || outcome.cancelled) {
          return {
            participantId: participant.participantId,
            operationId,
            status: "cancelled" as const,
            error: "cancelled",
          };
        }

        return {
          participantId: participant.participantId,
          operationId,
          status: "success" as const,
          response: outcome.response,
        };
      } catch (error) {
        const errorCategory = safeAttemptError(error);
        return {
          participantId: participant.participantId,
          operationId,
          status:
            cancelled || errorCategory === "cancelled"
              ? ("cancelled" as const)
              : ("failed" as const),
          error: cancelled ? "cancelled" : errorCategory,
        };
      } finally {
        activeStreams.delete(operationId);
      }
    }),
  );

  const result = participantResults.then((participants) => {
    const completedAtMs = Date.now();
    return {
      invocationId,
      startedAt: new Date(invocationStartedAt).toISOString(),
      completedAt: new Date(completedAtMs).toISOString(),
      latencyMs: Math.max(
        0,
        Math.round(completedAtMs - invocationStartedAt),
      ),
      participantCount: participants.length,
      successCount: participants.filter(
        (participant) => participant.status === "success",
      ).length,
      failedCount: participants.filter(
        (participant) => participant.status === "failed",
      ).length,
      cancelledCount: participants.filter(
        (participant) => participant.status === "cancelled",
      ).length,
      participants,
    };
  });

  return {
    invocationId,
    result,
    cancel: async () => {
      cancelled = true;
      await Promise.allSettled(
        [...activeStreams.values()].map((stream) => stream.cancel()),
      );
    },
  };
}

function promptFromMessages(messages: AiCenterConversationMessage[]): string {
  return messages
    .map((message) => `${message.role}: ${message.content}`)
    .join("\n\n");
}

export function streamThroughAiCenter(
  messages: AiCenterConversationMessage[],
  selectedModel: AiCenterModelChoice | undefined,
  onChunk: (text: string) => void,
): AiCenterStream {
  const candidates = selectedModel ? [selectedModel] : listAutoCandidates();
  if (candidates.length === 0) throw new Error("NO_CONNECTED_PROVIDER");

  const operationId = `chat-${crypto.randomUUID()}`;
  let cancelled = false;
  let activeCancel: () => Promise<void> = async () => {};

  const streamChoice = async (
    choice: AiCenterModelChoice,
    attemptId: string,
    emit: (text: string) => void,
  ): Promise<{ response: AiCenterProviderResponse; cancelled: boolean }> => {
    const { instance } = resolveChoice(choice);
    if (instance.providerId === "anthropic" && instance.credential.kind === "local") {
      activeCancel = () =>
        invoke("cancel_claude_code_request", { operationId: attemptId });
      const response = await answerWithChoice(
        promptFromMessages(messages),
        choice,
        attemptId,
      );
      if (!cancelled) emit(response.text);
      return { response, cancelled };
    }

    activeCancel = () =>
      invoke("cancel_provider_response_stream", { operationId: attemptId });
    return new Promise((resolve, reject) => {
      let output = "";
      let settled = false;
      let unlistenChunk: UnlistenFn | undefined;
      let unlistenDone: UnlistenFn | undefined;
      let unlistenError: UnlistenFn | undefined;
      const cleanup = () => {
        unlistenChunk?.();
        unlistenDone?.();
        unlistenError?.();
      };
      const fail = (error: Error) => {
        if (settled) return;
        settled = true;
        cleanup();
        reject(error);
      };
      void (async () => {
        unlistenChunk = await listen<StreamEvent>("ai-center://chunk", (event) => {
          if (event.payload.operationId !== attemptId) return;
          output += event.payload.text;
          emit(event.payload.text);
        });
        unlistenDone = await listen<DoneEvent>("ai-center://done", (event) => {
          if (event.payload.operationId !== attemptId || settled) return;
          settled = true;
          cleanup();
          resolve({
            response: {
              providerId: choice.providerId,
              modelId: choice.modelId,
              text: output,
            },
            cancelled: event.payload.cancelled,
          });
        });
        unlistenError = await listen<ErrorEvent>("ai-center://error", (event) => {
          if (event.payload.operationId !== attemptId) return;
          fail(new Error(event.payload.message));
        });
        try {
          await invoke("start_provider_response_stream", {
            input: {
              operationId: attemptId,
              providerId: choice.providerId,
              providerInstanceId: choice.providerInstanceId,
              modelId: choice.modelId,
              messages,
            },
          });
        } catch (error) {
          fail(
            new Error(
              typeof error === "string"
                ? error
                : "AI Center could not start the response stream.",
            ),
          );
        }
      })();
    });
  };

  const result = (async () => {
    const invocationStartedAt = Date.now();
    const promptText = promptFromMessages(messages);
    const attempts: AiCenterAttempt[] = [];
    let lastError: unknown;
    for (const [index, choice] of candidates.entries()) {
      let emitted = false;
      const attemptStartedAt = Date.now();
      try {
        const attempt = await streamChoice(
          choice,
          `${operationId}-${index}`,
          (text) => {
            emitted = true;
            onChunk(text);
          },
        );
        const wasCancelled = cancelled || attempt.cancelled;
        attempts.push(
          completeAttempt(
            choice,
            attemptStartedAt,
            wasCancelled ? "cancelled" : "success",
          ),
        );
        const metadata = buildInvocationMetadata({
          invocationId: operationId,
          routeMode: selectedModel ? "manual" : "auto",
          choice,
          startedAtMs: invocationStartedAt,
          promptText,
          outputText: attempt.response.text,
          attempts,
        });
        if (!wasCancelled) recordInvocationSuccess(metadata);
        return {
          response: { ...attempt.response, metadata },
          cancelled: wasCancelled,
        };
      } catch (error) {
        attempts.push(
          completeAttempt(
            choice,
            attemptStartedAt,
            cancelled ? "cancelled" : "failed",
            cancelled ? undefined : error,
          ),
        );
        lastError = error;
        if (cancelled) {
          const metadata = buildInvocationMetadata({
            invocationId: operationId,
            routeMode: selectedModel ? "manual" : "auto",
            choice,
            startedAtMs: invocationStartedAt,
            promptText,
            outputText: "",
            attempts,
          });
          return {
            response: {
              providerId: choice.providerId,
              modelId: choice.modelId,
              text: "",
              metadata,
            },
            cancelled: true,
          };
        }
        if (
          !shouldContinueAfterAttemptFailure({
            routeMode: selectedModel ? "manual" : "auto",
            emittedOutput: emitted,
            cancelled,
          })
        ) {
          throw error;
        }
      }
    }
    throw lastError ?? new Error("NO_CONNECTED_PROVIDER");
  })();

  return {
    operationId,
    result,
    cancel: async () => {
      cancelled = true;
      await activeCancel();
    },
  };
}
