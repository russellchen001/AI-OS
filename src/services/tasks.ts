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
): Promise<ExecuteWorkTaskResponse> {
  return invoke<ExecuteWorkTaskResponse>("execute_chat_work_task", {
    input: { taskId, agentId },
  });
}
