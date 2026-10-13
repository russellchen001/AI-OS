import {
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";

import {
  getRuntimeStatuses,
  listRuntimes,
} from "../services/runtime";

import type {
  RuntimeDefinition,
  RuntimeStatus,
} from "../types/runtime";

type UseRuntimesOptions = {
  refreshInterval?: number;
};

function useRuntimes({
  refreshInterval,
}: UseRuntimesOptions = {}) {
  const [runtimes, setRuntimes] =
    useState<RuntimeDefinition[]>([]);
  const [statuses, setStatuses] =
    useState<RuntimeStatus[]>([]);
  const [isLoadingDefinitions, setIsLoadingDefinitions] =
    useState(false);
  const [isRefreshingStatuses, setIsRefreshingStatuses] =
    useState(false);
  const [definitionsError, setDefinitionsError] =
    useState("");
  const [statusError, setStatusError] =
    useState("");
  const [lastUpdated, setLastUpdated] =
    useState("Not checked");

  const coordinatorPromiseRef = useRef<
    Promise<void> | null
  >(null);
  const refreshRequestedRef = useRef(false);

  const refreshStatuses = useCallback(() => {
    refreshRequestedRef.current = true;
    if (coordinatorPromiseRef.current !== null) {
      return coordinatorPromiseRef.current;
    }

    setIsRefreshingStatuses(true);
    const coordinator = Promise.resolve().then(
      async () => {
        try {
          while (refreshRequestedRef.current) {
            refreshRequestedRef.current = false;
            try {
              const nextStatuses =
                await getRuntimeStatuses();

              setStatuses(nextStatuses);
              setStatusError("");
              setLastUpdated(
                new Date().toLocaleTimeString(
                  [],
                  {
                    hour: "2-digit",
                    minute: "2-digit",
                    second: "2-digit",
                  },
                ),
              );
            } catch {
              if (refreshRequestedRef.current) {
                continue;
              }

              setStatusError(
                "Runtime status is unavailable.",
              );
              throw new Error(
                "Runtime status refresh failed.",
              );
            }
          }
        } finally {
          coordinatorPromiseRef.current = null;
          setIsRefreshingStatuses(false);
        }
      },
    );
    coordinatorPromiseRef.current = coordinator;
    return coordinator;
  }, []);

  useEffect(() => {
    void refreshStatuses().catch(
      () => undefined,
    );
  }, [refreshStatuses]);

  const refreshDefinitions = useCallback(
    async () => {
      setIsLoadingDefinitions(true);
      try {
        const definitions =
          await listRuntimes();
        setRuntimes(definitions);
        setDefinitionsError("");
      } catch {
        setDefinitionsError(
          "Runtime definitions are unavailable.",
        );
      } finally {
        setIsLoadingDefinitions(false);
      }
    },
    [],
  );

  const refresh = useCallback(async () => {
    const definitions = refreshDefinitions();
    const nextStatuses =
      refreshStatuses().catch(
        () => undefined,
      );
    await Promise.all([
      definitions,
      nextStatuses,
    ]);
  }, [
    refreshDefinitions,
    refreshStatuses,
  ]);

  useEffect(() => {
    void refreshDefinitions();
  }, [refreshDefinitions]);

  useEffect(() => {
    if (!refreshInterval) {
      return;
    }

    const interval = window.setInterval(
      () => {
        void refreshStatuses().catch(
          () => undefined,
        );
      },
      Math.max(refreshInterval, 2) * 1000,
    );

    return () => {
      window.clearInterval(interval);
    };
  }, [
    refreshInterval,
    refreshStatuses,
  ]);

  return {
    runtimes,
    statuses,
    isLoadingDefinitions,
    isRefreshingStatuses,
    definitionsError,
    statusError,
    lastUpdated,
    refresh,
    refreshStatuses,
  };
}

export default useRuntimes;
