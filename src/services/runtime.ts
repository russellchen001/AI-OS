import {
  invoke,
} from "@tauri-apps/api/core";

import type {
  RuntimeDefinition,
  RuntimeStatus,
  RuntimeStatusRequest,
} from "../types/runtime";

export async function listRuntimes():
Promise<RuntimeDefinition[]> {
  return invoke<RuntimeDefinition[]>(
    "list_runtimes",
  );
}

export async function getRuntimeStatuses(
  request?: RuntimeStatusRequest,
): Promise<RuntimeStatus[]> {
  return invoke<RuntimeStatus[]>(
    "get_runtime_statuses",
    {
      request,
    },
  );
}
