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
  RuntimeStatusRequest,
} from "../types/runtime";

type UseRuntimesOptions = {
  request?: RuntimeStatusRequest;
  refreshInterval?: number;
};

function useRuntimes({
  request,
  refreshInterval,
}: UseRuntimesOptions = {}) {
  const ollamaUrl = request?.ollamaUrl;
  const openWebUiUrl =
    request?.openWebUiUrl;

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

  const endpointRef = useRef({
    ollamaUrl,
    openWebUiUrl,
  });
  const endpointGenerationRef = useRef(0);
  if (
    endpointRef.current.ollamaUrl !==
      ollamaUrl ||
    endpointRef.current.openWebUiUrl !==
      openWebUiUrl
  ) {
    endpointRef.current = {
      ollamaUrl,
      openWebUiUrl,
    };
    endpointGenerationRef.current += 1;
  }
  const scheduledEndpointGenerationRef =
    useRef(-1);

  const coordinatorPromiseRef = useRef<
    Promise<void> | null
  >(null);
  const refreshRequestedRef = useRef(false);

  const drainStatusRefreshes = useCallback(
    async () => {
      while (refreshRequestedRef.current) {
        refreshRequestedRef.current = false;
        const queryGeneration =
          endpointGenerationRef.current;
        const queryEndpoints = {
          ...endpointRef.current,
        };
        scheduledEndpointGenerationRef.current =
          queryGeneration;

        try {
          const nextStatuses =
            await getRuntimeStatuses({
              ollamaUrl:
                queryEndpoints.ollamaUrl,
              openWebUiUrl:
                queryEndpoints.openWebUiUrl,
            });

          if (
            queryGeneration !==
            endpointGenerationRef.current
          ) {
            refreshRequestedRef.current = true;
            continue;
          }

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
          if (
            queryGeneration !==
            endpointGenerationRef.current
          ) {
            refreshRequestedRef.current = true;
            continue;
          }

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
    },
    [],
  );

  const refreshStatuses = useCallback(() => {
    refreshRequestedRef.current = true;
    if (coordinatorPromiseRef.current !== null) {
      return coordinatorPromiseRef.current;
    }

    setIsRefreshingStatuses(true);
    const coordinator = drainStatusRefreshes()
      .finally(() => {
        if (
          coordinatorPromiseRef.current ===
          coordinator
        ) {
          coordinatorPromiseRef.current = null;
          setIsRefreshingStatuses(false);
        }
      });
    coordinatorPromiseRef.current = coordinator;
    return coordinator;
  }, [drainStatusRefreshes]);

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
    const generation =
      endpointGenerationRef.current;
    if (
      scheduledEndpointGenerationRef.current ===
      generation
    ) {
      return;
    }
    scheduledEndpointGenerationRef.current =
      generation;
    void refreshStatuses().catch(
      () => undefined,
    );
  }, [
    ollamaUrl,
    openWebUiUrl,
    refreshStatuses,
  ]);

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
