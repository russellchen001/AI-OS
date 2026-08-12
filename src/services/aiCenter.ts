import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { listProviderInstances } from "./providers";
import {
  enrichInvocationPricing,
  recordInvocationSuccess,
  type AiCenterCanonicalInvocationMetadata,
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

type AiCenterRouteSelection = Pick<
  AiCenterModelChoice,
  "providerId" | "providerInstanceId" | "modelId"
>;

function resolveChoice(choice: AiCenterRouteSelection) {
  if (
    choice.providerId === "ollama" &&
    choice.providerInstanceId === "ollama-local"
  ) {
    return {
      instance: {
        id: "ollama-local",
        providerId: "ollama",
        credential: { kind: "local" as const },
      },
      model: { remoteModelId: choice.modelId },
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

export async function answerThroughAiCenter(
  prompt: string,
  selectedModel?: AiCenterModelChoice,
): Promise<AiCenterResponse> {
  const response = await invoke<
    AiCenterProviderResponse & { metadata: AiCenterCanonicalInvocationMetadata }
  >("execute_ai_center", {
    input: {
      routeMode: selectedModel ? "manual" : "auto",
      manualCandidate: selectedModel,
      prompt,
    },
  });
  const metadata = enrichInvocationPricing(response.metadata);
  recordInvocationSuccess(metadata);
  return { ...response, metadata };
}

type StreamEvent = { operationId: string; text: string };
type DoneEvent = {
  operationId: string;
  cancelled: boolean;
  providerId?: string;
  modelId?: string;
  metadata?: AiCenterCanonicalInvocationMetadata;
};
type ErrorEvent = { operationId: string; message: string };

export type AiCenterStream = {
  operationId: string;
  result: Promise<{ response: AiCenterResponse; cancelled: boolean }>;
  cancel: () => Promise<void>;
};

export type AiCenterConversationMessage = {
  role: "system" | "user" | "assistant";
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
        const errorCategory = /cancel/i.test(
          error instanceof Error ? error.message : String(error),
        )
          ? "cancelled"
          : "provider-error";
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
  const operationId = `chat-${crypto.randomUUID()}`;
  let output = "";
  let settled = false;
  let unlistenChunk: UnlistenFn | undefined;
  let unlistenDone: UnlistenFn | undefined;
  let unlistenError: UnlistenFn | undefined;

  const result = new Promise<{
    response: AiCenterResponse;
    cancelled: boolean;
  }>((resolve, reject) => {
    const cleanup = () => {
      unlistenChunk?.();
      unlistenDone?.();
      unlistenError?.();
    };
    void (async () => {
      unlistenChunk = await listen<StreamEvent>("ai-center://chunk", (event) => {
        if (event.payload.operationId !== operationId || settled) return;
        output += event.payload.text;
        onChunk(event.payload.text);
      });
      unlistenDone = await listen<DoneEvent>("ai-center://done", (event) => {
        if (event.payload.operationId !== operationId || settled) return;
        settled = true;
        cleanup();
        if (!event.payload.metadata) {
          reject(new Error("AI Center returned no invocation metadata."));
          return;
        }
        const metadata = enrichInvocationPricing(event.payload.metadata);
        if (!event.payload.cancelled) recordInvocationSuccess(metadata);
        resolve({
          response: {
            providerId: event.payload.providerId ?? metadata.providerId,
            modelId: event.payload.modelId ?? metadata.modelId,
            text: output,
            metadata,
          },
          cancelled: event.payload.cancelled,
        });
      });
      unlistenError = await listen<ErrorEvent>("ai-center://error", (event) => {
        if (event.payload.operationId !== operationId || settled) return;
        settled = true;
        cleanup();
        reject(new Error(event.payload.message));
      });
      try {
        await invoke("execute_ai_center_stream", {
          input: {
            operationId,
            routeMode: selectedModel ? "manual" : "auto",
            manualCandidate: selectedModel,
            messages,
          },
        });
      } catch (error) {
        if (settled) return;
        settled = true;
        cleanup();
        reject(
          new Error(
            typeof error === "string"
              ? error
              : "AI Center could not start the response stream.",
          ),
        );
      }
    })();
  });

  return {
    operationId,
    result,
    cancel: async () => {
      await invoke("cancel_provider_response_stream", { operationId });
    },
  };
}
