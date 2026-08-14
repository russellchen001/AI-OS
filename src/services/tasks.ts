import { invoke } from "@tauri-apps/api/core";

export type ChatTaskType = "ASK" | "DO";
export type ChatTaskStatus =
  | "READY"
  | "PLANNING"
  | "EXECUTING"
  | "VERIFYING"
  | "COMPLETED"
  | "FAILED";

export type SubmitChatTaskResponse = {
  taskId: string;
  status: ChatTaskStatus;
  taskType: ChatTaskType;
};

export type ExecuteWorkTaskResponse = {
  taskId: string;
  planId: string;
  agentId: string;
  status: ChatTaskStatus;
  output?: unknown;
};

export type ExecuteWorkTaskOptions = {
  capability?: string;
  input?: Record<string, unknown>;
  userConfirmed?: boolean;
};

export function describeWorkTaskError(error: unknown): string {
  const detail =
    typeof error === "string"
      ? error
      : error instanceof Error
        ? error.message
        : "";
  const normalized = detail.toLowerCase();

  if (normalized.includes("pairing")) {
    return "OpenClaw pairing is required. Pair OpenClaw and try the folder scan again.";
  }
  if (normalized.includes("permission") || normalized.includes("not permitted")) {
    return "OpenClaw permission was denied for this folder scan.";
  }
  if (normalized.includes("authentication") || normalized.includes("unauthorized")) {
    return "OpenClaw authentication is required. Reconnect OpenClaw and try again.";
  }
  if (
    normalized.includes("connection") ||
    normalized.includes("unavailable") ||
    normalized.includes("unreachable") ||
    normalized.includes("no active") ||
    normalized.includes("runtime not found")
  ) {
    return "OpenClaw is unavailable. Start or connect OpenClaw and try the folder scan again.";
  }
  return "OpenClaw Runtime could not complete this folder scan. Check OpenClaw and try again.";
}

export function describeChatTaskError(
  error: unknown,
  isWorkRequest: boolean,
): string {
  if (isWorkRequest) return describeWorkTaskError(error);
  return error instanceof Error && error.message === "NO_CONNECTED_PROVIDER"
    ? "Connect and test an AI in My AI before starting a conversation."
    : "AI‑OS could not complete this request. Check the selected AI connection and try again.";
}

export async function submitChatTask(
  prompt: string,
  taskType: ChatTaskType = "ASK",
): Promise<SubmitChatTaskResponse> {
  return invoke<SubmitChatTaskResponse>("submit_chat_task", {
    request: { prompt, taskType },
  });
}

export async function startChatTaskExecution(
  taskId: string,
): Promise<SubmitChatTaskResponse> {
  return invoke<SubmitChatTaskResponse>("start_chat_task_execution", {
    input: { taskId },
  });
}

export async function completeChatTaskExecution(
  taskId: string,
  result: { providerId: string; modelId: string; text: string },
): Promise<SubmitChatTaskResponse> {
  return invoke<SubmitChatTaskResponse>("complete_chat_task_execution", {
    input: { taskId, ...result },
  });
}

export async function failChatTaskExecution(
  taskId: string,
  reason = "AI Center request failed",
): Promise<SubmitChatTaskResponse> {
  return invoke<SubmitChatTaskResponse>("fail_chat_task_execution", {
    input: { taskId, reason },
  });
}

export async function executeChatWorkTask(
  taskId: string,
  agentId = "openclaw",
  options: ExecuteWorkTaskOptions = {},
): Promise<ExecuteWorkTaskResponse> {
  return invoke<ExecuteWorkTaskResponse>("execute_chat_work_task", {
    input: { taskId, agentId, ...options },
  });
}
