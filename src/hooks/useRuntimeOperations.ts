import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import {
  getRuntimeOperation,
  listenRuntimeOperations,
  reconcileRuntimeOperation,
  startRuntimeOperation,
} from "../services/runtime";
import type {
  RuntimeOperationAction,
  RuntimeOperationSnapshot,
  StartRuntimeOperationRequest,
} from "../types/runtime";

export type RuntimeNotificationSeverity =
  | "success"
  | "info"
  | "warning"
  | "error";

type Notify = (
  message: string,
  severity: RuntimeNotificationSeverity,
) => void;

type ListenerState =
  | "connecting"
  | "ready"
  | "failed";

type UseRuntimeOperationsOptions = {
  refreshStatuses: () => Promise<void>;
  notify: Notify;
};

const ACTIVE_STATES = new Set([
  "queued",
  "running",
  "cancelling",
]);

const LIFECYCLE_ACTIONS = new Set<
RuntimeOperationAction
>([
  "start",
  "stop",
  "restart",
]);

function isTerminal(
  operation: RuntimeOperationSnapshot,
): boolean {
  return !ACTIVE_STATES.has(
    operation.state,
  );
}

function channelKey(
  runtimeId: string,
  action: RuntimeOperationAction,
): string {
  return `${runtimeId}:${
    action === "open"
      ? "open"
      : "lifecycle"
  }`;
}

function terminalSuccessMessage(
  action: RuntimeOperationAction,
): string {
  switch (action) {
    case "start":
      return "Runtime started.";
    case "stop":
      return "Runtime stopped.";
    case "restart":
      return "Runtime restarted.";
    case "open":
      return "Runtime opened.";
  }
}

function pruneTerminalSnapshots(
  operations: Record<
    string,
    RuntimeOperationSnapshot
  >,
): Record<
  string,
  RuntimeOperationSnapshot
> {
  const terminals = Object.values(
    operations,
  )
    .filter(isTerminal)
    .sort((left, right) =>
      right.updatedAt.localeCompare(
        left.updatedAt,
      ),
    );

  if (terminals.length <= 50) {
    return operations;
  }

  const retained = new Set(
    terminals
      .slice(0, 50)
      .map(
        (operation) =>
          operation.operationId,
      ),
  );

  return Object.fromEntries(
    Object.entries(operations).filter(
      ([, operation]) =>
        !isTerminal(operation) ||
        retained.has(
          operation.operationId,
        ),
    ),
  );
}

export default function useRuntimeOperations({
  refreshStatuses,
  notify,
}: UseRuntimeOperationsOptions) {
  const [operationsById, setOperationsById] =
    useState<
      Record<
        string,
        RuntimeOperationSnapshot
      >
    >({});
  const [pendingChannels, setPendingChannels] =
    useState<Set<string>>(
      () => new Set(),
    );
  const [listenerState, setListenerState] =
    useState<ListenerState>(
      "connecting",
    );

  const operationsRef = useRef(
    operationsById,
  );
  const pendingRef = useRef(
    new Set<string>(),
  );
  const handledTerminalsRef = useRef(
    new Set<string>(),
  );
  const refreshInFlightRef = useRef(false);
  const refreshTrailingRef = useRef(false);

  const runCoalescedRefresh = useCallback(
    async () => {
      if (refreshInFlightRef.current) {
        refreshTrailingRef.current = true;
        return;
      }

      refreshInFlightRef.current = true;
      try {
        await refreshStatuses();
      } catch {
        notify(
          "The action finished, but runtime status could not be refreshed.",
          "warning",
        );
      } finally {
        refreshInFlightRef.current = false;
        if (refreshTrailingRef.current) {
          refreshTrailingRef.current = false;
          void runCoalescedRefresh();
        }
      }
    }, [
      notify,
      refreshStatuses,
    ],
  );

  const handleTerminal = useCallback(
    (
      operation: RuntimeOperationSnapshot,
    ) => {
      if (
        !isTerminal(operation) ||
        handledTerminalsRef.current.has(
          operation.operationId,
        )
      ) {
        return;
      }

      handledTerminalsRef.current.add(
        operation.operationId,
      );

      if (operation.state === "succeeded") {
        notify(
          terminalSuccessMessage(
            operation.action,
          ),
          "success",
        );
      } else {
        notify(
          "The runtime action failed. Check the runtime installation and configuration, then try again.",
          "error",
        );
      }

      if (
        LIFECYCLE_ACTIONS.has(
          operation.action,
        )
      ) {
        void runCoalescedRefresh();
      }
    }, [
      notify,
      runCoalescedRefresh,
    ],
  );

  const ingestSnapshot = useCallback(
    (
      incoming: RuntimeOperationSnapshot,
      allowUnknown: boolean,
    ) => {
      const current =
        operationsRef.current[
          incoming.operationId
        ] ?? null;

      if (
        current === null &&
        !allowUnknown &&
        isTerminal(incoming)
      ) {
        return;
      }

      let reconciled: RuntimeOperationSnapshot;
      try {
        reconciled =
          reconcileRuntimeOperation(
            current,
            incoming,
          );
      } catch {
        return;
      }

      if (reconciled === current) {
        return;
      }

      const next = pruneTerminalSnapshots({
        ...operationsRef.current,
        [reconciled.operationId]:
          reconciled,
      });
      operationsRef.current = next;
      setOperationsById(next);
      handleTerminal(reconciled);
    },
    [handleTerminal],
  );

  useEffect(() => {
    let disposed = false;
    let unlisten:
      | (() => void)
      | null = null;
    let unlistenCalled = false;

    const callUnlisten = () => {
      if (
        unlisten !== null &&
        !unlistenCalled
      ) {
        unlistenCalled = true;
        unlisten();
      }
    };

    void listenRuntimeOperations(
      (operation) => {
        ingestSnapshot(
          operation,
          false,
        );
      },
    )
      .then((nextUnlisten) => {
        unlisten = nextUnlisten;
        if (disposed) {
          callUnlisten();
          return;
        }
        setListenerState("ready");
      })
      .catch(() => {
        if (disposed) {
          return;
        }
        setListenerState("failed");
        notify(
          "Runtime action updates are unavailable. Individual runtime controls are disabled.",
          "warning",
        );
      });

    return () => {
      disposed = true;
      callUnlisten();
    };
  }, [
    ingestSnapshot,
    notify,
  ]);

  const lookupAndIngest = useCallback(
    async (operationId: string) => {
      try {
        const operation =
          await getRuntimeOperation(
            operationId,
          );
        ingestSnapshot(operation, true);
      } catch {
        // Lookup is best-effort; event and response snapshots remain canonical.
      }
    },
    [ingestSnapshot],
  );

  const runRuntimeAction = useCallback(
    async (
      request: StartRuntimeOperationRequest,
    ) => {
      if (listenerState !== "ready") {
        return;
      }

      const key = channelKey(
        request.runtimeId,
        request.action,
      );
      if (pendingRef.current.has(key)) {
        return;
      }

      pendingRef.current.add(key);
      setPendingChannels(
        new Set(pendingRef.current),
      );

      try {
        const admission =
          await startRuntimeOperation(
            request,
          );

        if (admission.status === "accepted") {
          ingestSnapshot(
            admission.operation,
            true,
          );
          void lookupAndIngest(
            admission.operation.operationId,
          );
          return;
        }

        if (admission.status === "conflict") {
          ingestSnapshot(
            admission.existingOperation,
            true,
          );
          notify(
            "Another lifecycle action is already in progress for this runtime.",
            "warning",
          );
          void lookupAndIngest(
            admission.existingOperation
              .operationId,
          );
          return;
        }

        notify(
          admission.error.code ===
            "operation-capacity-exceeded"
            ? "Runtime actions are temporarily busy. Try again shortly."
            : "This action is not available for the current runtime configuration.",
          admission.error.code ===
            "operation-capacity-exceeded"
            ? "warning"
            : "error",
        );
      } catch {
        notify(
          "This action is not available for the current runtime configuration.",
          "error",
        );
      } finally {
        pendingRef.current.delete(key);
        setPendingChannels(
          new Set(pendingRef.current),
        );
      }
    },
    [
      ingestSnapshot,
      listenerState,
      lookupAndIngest,
      notify,
    ],
  );

  const derived = useMemo(() => {
    const activeLifecycleByRuntime:
      Record<
        string,
        RuntimeOperationSnapshot
      > = {};
    const activeOpenByRuntime:
      Record<
        string,
        RuntimeOperationSnapshot
      > = {};
    const latestTerminalByRuntime:
      Record<
        string,
        RuntimeOperationSnapshot
      > = {};

    for (const operation of Object.values(
      operationsById,
    )) {
      if (isTerminal(operation)) {
        const current =
          latestTerminalByRuntime[
            operation.runtimeId
          ];
        if (
          current === undefined ||
          operation.updatedAt >
            current.updatedAt
        ) {
          latestTerminalByRuntime[
            operation.runtimeId
          ] = operation;
        }
      } else if (operation.action === "open") {
        activeOpenByRuntime[
          operation.runtimeId
        ] = operation;
      } else {
        activeLifecycleByRuntime[
          operation.runtimeId
        ] = operation;
      }
    }

    const lifecyclePendingByRuntime:
      Record<string, boolean> = {};
    const openPendingByRuntime:
      Record<string, boolean> = {};
    for (const key of pendingChannels) {
      const separator =
        key.lastIndexOf(":");
      const runtimeId = key.slice(
        0,
        separator,
      );
      if (key.endsWith(":open")) {
        openPendingByRuntime[runtimeId] =
          true;
      } else {
        lifecyclePendingByRuntime[
          runtimeId
        ] = true;
      }
    }

    return {
      activeLifecycleByRuntime,
      activeOpenByRuntime,
      latestTerminalByRuntime,
      lifecyclePendingByRuntime,
      openPendingByRuntime,
    };
  }, [
    operationsById,
    pendingChannels,
  ]);

  return {
    operationsById,
    listenerState,
    runRuntimeAction,
    ...derived,
  };
}
