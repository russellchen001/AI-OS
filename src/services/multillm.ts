import {
  invoke,
} from "@tauri-apps/api/core";

export type MultiLlmMessage = {
  role: "system" | "user" | "assistant";
  content: string;
};

export type StartMultiLlmRequest = {
  operationId: string;
  providerId: string;
  baseUrl: string;
  apiKey: string;
  model: string;
  messages: MultiLlmMessage[];
  maxTokens?: number;
};

export async function startMultiLlmStream(
  request: StartMultiLlmRequest,
): Promise<void> {
  await invoke<void>(
    "start_multillm_stream",
    {
      request,
    },
  );
}

export async function cancelMultiLlmStream(
  operationId: string,
): Promise<void> {
  await invoke<void>(
    "cancel_multillm_stream",
    {
      operationId,
    },
  );
}
