import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { listProviderInstances } from "./providers";

export type AiCenterResponse = {
  providerId: string;
  modelId: string;
  text: string;
};

export type AiCenterModelChoice = {
  providerId: string;
  providerInstanceId: string;
  modelId: string;
  label: string;
};

export function listAiCenterModels(): AiCenterModelChoice[] {
  return listProviderInstances()
    .filter((instance) => instance.connectionState === "connected")
    .flatMap((instance) =>
      instance.models
        .filter((model) => model.enabled)
        .map((model) => ({
          providerId: instance.providerId,
          providerInstanceId: instance.id,
          modelId: model.remoteModelId,
          label: `${instance.displayName} · ${model.displayName}`,
        })),
    );
}

export async function answerThroughAiCenter(
  prompt: string,
  selectedModel?: AiCenterModelChoice,
): Promise<AiCenterResponse> {
  const instances = listProviderInstances();
  const instance = selectedModel
    ? instances.find(
        (candidate) =>
          candidate.id === selectedModel.providerInstanceId &&
          candidate.providerId === selectedModel.providerId &&
          candidate.connectionState === "connected",
      )
    : instances.find(
        (candidate) =>
          candidate.connectionState === "connected" &&
          candidate.models.some((model) => model.enabled),
      );
  if (!instance) {
    throw new Error("NO_CONNECTED_PROVIDER");
  }
  const model = selectedModel
    ? instance.models.find(
        (candidate) =>
          candidate.enabled &&
          candidate.remoteModelId === selectedModel.modelId,
      )
    : instance.models.find(
        (candidate) => candidate.enabled && candidate.isDefault,
      ) ?? instance.models.find((candidate) => candidate.enabled);
  if (!model) {
    throw new Error("NO_DEFAULT_MODEL");
  }

  return invoke<AiCenterResponse>("generate_provider_response", {
    input: {
      providerId: instance.providerId,
      providerInstanceId: instance.id,
      modelId: model.remoteModelId,
      prompt,
    },
  });
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

export function streamThroughAiCenter(
  messages: AiCenterConversationMessage[],
  selectedModel: AiCenterModelChoice | undefined,
  onChunk: (text: string) => void,
): AiCenterStream {
  const models = listAiCenterModels();
  const choice = selectedModel ?? models[0];
  if (!choice) {
    throw new Error("NO_CONNECTED_PROVIDER");
  }
  const operationId = `chat-${crypto.randomUUID()}`;
  const result = new Promise<{ response: AiCenterResponse; cancelled: boolean }>(
    (resolve, reject) => {
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
          if (event.payload.operationId !== operationId) return;
          output += event.payload.text;
          onChunk(event.payload.text);
        });
        unlistenDone = await listen<DoneEvent>("ai-center://done", (event) => {
          if (event.payload.operationId !== operationId || settled) return;
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
          if (event.payload.operationId !== operationId) return;
          fail(new Error(event.payload.message));
        });
        try {
          await invoke("start_provider_response_stream", {
            input: {
              operationId,
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
    },
  );
  return {
    operationId,
    result,
    cancel: () =>
      invoke("cancel_provider_response_stream", { operationId }),
  };
}
