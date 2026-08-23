import { invoke } from "@tauri-apps/api/core";

/**
 * Rust prefixes every OpenClaw execution error with its machine-readable kind,
 * e.g. "[ConnectionUnavailable] ...". Classify on that, never on wording:
 * matching prose made "destination is unavailable" render as
 * "OpenClaw is unavailable" and cost days of misdirected debugging.
 */
function extractErrorKind(detail: string): string | null {
  const match = detail.match(/^\[([A-Za-z]+)\]/);
  return match ? match[1] : null;
}

function describeByKind(kind: string, operation: string): string | null {
  switch (kind) {
    case "InvalidRequest":
      return `The ${operation} request was rejected: check the source and destination.`;
    case "PermissionRequired":
      return `This ${operation} needs your confirmation before it can run.`;
    case "PermissionDenied":
      return `OpenClaw permission was denied for this ${operation}.`;
    case "AuthenticationRequired":
      return `Authentication is required. Reconnect and try the ${operation} again.`;
    case "PairingRequired":
      return `OpenClaw pairing is required. Pair OpenClaw and try the ${operation} again.`;
    case "ConnectionUnavailable":
      return `OpenClaw is unavailable. Start or connect OpenClaw and try the ${operation} again.`;
    case "ProtocolFailure":
      return `OpenClaw finished without producing the expected result for this ${operation}.`;
    case "ExecutionRejected":
      return `OpenClaw refused to run this ${operation}.`;
    case "ExecutionFailed":
      return `OpenClaw could not complete this ${operation}.`;
    default:
      return null;
  }
}


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

export type ParsedDownloadRequest = {
  source: string;
  extractionCode?: string;
  selectionHint: string;
};

export function parseDownloadRequestText(value: string): ParsedDownloadRequest {
  const text = value.trim();
  const link = text
    .match(/(?:https?|ftp|thunder|ed2k|magnet):[^\s]+/iu)?.[0]
    ?.replace(/[，。；;、）)\]】>]+$/u, "");
  if (!text) throw new Error("No download resource was provided.");

  const explicitCode = text.match(
    /(?:提取码|访问码|密码|extraction\s*code|code|pwd)\s*[:：=]?\s*([a-z0-9]{4,12})/iu,
  )?.[1];
  let queryCode: string | undefined;
  try {
    queryCode = link ? new URL(link).searchParams.get("pwd") ?? undefined : undefined;
  } catch {
    queryCode = undefined;
  }
  return {
    source: link ?? text,
    extractionCode: explicitCode ?? queryCode,
    selectionHint: text,
  };
}

export function describeDownloadResult(output: unknown): string {
  if (!output || typeof output !== "object") {
    throw new Error("OpenClaw returned an invalid download result.");
  }
  const result = output as {
    destination?: unknown;
    files?: unknown;
    kind?: unknown;
    source?: unknown;
    status?: unknown;
    tool?: unknown;
  };
  if (
    result.kind !== "download" ||
    typeof result.source !== "string" ||
    typeof result.destination !== "string" ||
    typeof result.tool !== "string" ||
    result.status !== "completed" ||
    !Array.isArray(result.files) ||
    !result.files.every((file) => typeof file === "string")
  ) {
    throw new Error("OpenClaw returned an invalid download result.");
  }
  const files = (result.files as string[])
    .map((file) => `${result.destination}/${file}`)
    .join("\n");
  return `Download completed with ${result.tool}.\n\n${files}`;
}

export function describeWorkTaskError(
  error: unknown,
  operation = "folder scan",
): string {
  const detail =
    typeof error === "string"
      ? error
      : error instanceof Error
        ? error.message
        : "";

  const detectedKind = extractErrorKind(detail);
  if (detectedKind) {
    const described = describeByKind(detectedKind, operation);
    if (described) return described;
  }

  if (operation === "download") {
    const downloadError = detail.toLowerCase();
    if (downloadError.includes("destination")) {
      return "The selected download destination is unavailable or does not permit writing.";
    }
    if (downloadError.includes("cloud-drive")) {
      return "No compatible cloud-drive download tool is registered for this source.";
    }
    if (downloadError.includes("unsupported")) {
      return "This download source is not supported.";
    }
    if (downloadError.includes("thunder")) {
      return "Thunder could not accept this download. Check that Thunder is installed and available.";
    }
    if (downloadError.includes("aria2")) {
      return "The aria2 download tool could not complete this download.";
    }
    if (downloadError.includes("browser") || downloadError.includes("web")) {
      return "The Browser/Web download workflow could not complete this download.";
    }
    if (downloadError.includes("authentication") || downloadError.includes("unauthorized")) {
      return "Authentication is required by the selected download service.";
    }
    if (downloadError.includes("permission") || downloadError.includes("not permitted")) {
      return "Permission was denied for this download.";
    }
    if (
      downloadError.includes("connection") ||
      downloadError.includes("unavailable") ||
      downloadError.includes("unreachable") ||
      downloadError.includes("no active")
    ) {
      return "OpenClaw is unavailable. Start or connect OpenClaw and try the download again.";
    }
    return detail
      ? `OpenClaw could not complete this download: ${detail}`
      : "OpenClaw could not complete this download.";
  }

  const normalized = detail.toLowerCase();

  if (normalized.includes("pairing")) {
    return `OpenClaw pairing is required. Pair OpenClaw and try the ${operation} again.`;
  }
  if (normalized.includes("permission") || normalized.includes("not permitted")) {
    return `OpenClaw permission was denied for this ${operation}.`;
  }
  if (normalized.includes("authentication") || normalized.includes("unauthorized")) {
    return `OpenClaw authentication is required. Reconnect OpenClaw and try the ${operation} again.`;
  }
  if (
    normalized.includes("connection") ||
    normalized.includes("unavailable") ||
    normalized.includes("unreachable") ||
    normalized.includes("no active") ||
    normalized.includes("runtime not found")
  ) {
    return `OpenClaw is unavailable. Start or connect OpenClaw and try the ${operation} again.`;
  }
  return `OpenClaw Runtime could not complete this ${operation}. Check OpenClaw and try again.`;
}

export function describeChatTaskError(
  error: unknown,
  isWorkRequest: boolean,
  workOperation?: string,
): string {
  if (isWorkRequest) return describeWorkTaskError(error, workOperation);
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
