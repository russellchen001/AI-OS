import {
  invoke,
} from "@tauri-apps/api/core";
import {
  listen,
  type UnlistenFn,
} from "@tauri-apps/api/event";

import type {
  RuntimeDefinition,
  RuntimeOperationAdmission,
  RuntimeOperationEvent,
  RuntimeOperationSnapshot,
  RuntimeStatus,
  RuntimeStatusRequest,
  StartRuntimeOperationRequest,
} from "../types/runtime";

export const RUNTIME_OPERATION_EVENT =
  "runtime://operation" as const;

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

export async function startRuntimeOperation(
  request: StartRuntimeOperationRequest,
): Promise<RuntimeOperationAdmission> {
  return invoke<RuntimeOperationAdmission>(
    "start_runtime_operation",
    {
      request,
    },
  );
}

export async function getRuntimeOperation(
  operationId: string,
): Promise<RuntimeOperationSnapshot> {
  return invoke<RuntimeOperationSnapshot>(
    "get_runtime_operation",
    {
      operationId,
    },
  );
}

export async function cancelRuntimeOperation(
  operationId: string,
): Promise<RuntimeOperationSnapshot> {
  return invoke<RuntimeOperationSnapshot>(
    "cancel_runtime_operation",
    {
      operationId,
    },
  );
}

export async function listenRuntimeOperations(
  handler: (operation: RuntimeOperationSnapshot) => void,
): Promise<UnlistenFn> {
  return listen<RuntimeOperationEvent>(
    RUNTIME_OPERATION_EVENT,
    (event) => {
      const payload: unknown = event.payload;
      if (
        typeof payload !== "object" ||
        payload === null ||
        !("version" in payload) ||
        payload.version !== 1 ||
        !("operation" in payload) ||
        typeof payload.operation !== "object" ||
        payload.operation === null
      ) {
        return;
      }
      handler(
        payload.operation as RuntimeOperationSnapshot,
      );
    },
  );
}

export function reconcileRuntimeOperation(
  current: RuntimeOperationSnapshot | null,
  incoming: RuntimeOperationSnapshot,
): RuntimeOperationSnapshot {
  if (current === null) {
    return incoming;
  }
  if (current.operationId !== incoming.operationId) {
    throw new Error(
      "Runtime operation snapshots cannot be reconciled.",
    );
  }
  return incoming.revision > current.revision
    ? incoming
    : current;
}
