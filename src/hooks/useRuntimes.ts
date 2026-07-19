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

type Deferred = {
  promise: Promise<void>;
  resolve: () => void;
  reject: () => void;
};

function createDeferred(): Deferred {
  let resolve: () => void = () => {};
  let reject: () => void = () => {};
  const promise = new Promise<void>(
    (nextResolve, nextReject) => {
      resolve = nextResolve;
      reject = () => nextReject();
    },
  );
  return {
    promise,
    resolve,
    reject,
  };
}

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

  const inFlightRef = useRef<
    Promise<void> | null
  >(null);
  const trailingRef = useRef<Deferred | null>(
    null,
  );

  const executeStatusQuery = useCallback(
    async (
      queryGeneration: number,
      endpoints: RuntimeStatusRequest,
    ) => {
      const nextStatuses =
        await getRuntimeStatuses({
          ollamaUrl: endpoints.ollamaUrl,
          openWebUiUrl:
            endpoints.openWebUiUrl,
        });
      if (
        queryGeneration ===
        endpointGenerationRef.current
      ) {
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
      }
    },
    [],
  );

  const startStatusQuery = useCallback(() => {
    setIsRefreshingStatuses(true);
    const queryGeneration =
      endpointGenerationRef.current;
    const queryEndpoints = {
      ...endpointRef.current,
    };
    const query = executeStatusQuery(
      queryGeneration,
      queryEndpoints,
    )
      .catch(() => {
        if (
          queryGeneration ===
          endpointGenerationRef.current
        ) {
          setStatusError(
            "Runtime status is unavailable.",
          );
        }
        throw new Error(
          "Runtime status refresh failed.",
        );
      })
      .finally(() => {
        if (inFlightRef.current !== query) {
          return;
        }

        inFlightRef.current = null;
        const trailing = trailingRef.current;
        trailingRef.current = null;
        const endpointChanged =
          queryGeneration !==
          endpointGenerationRef.current;
        if (
          trailing === null &&
          !endpointChanged
        ) {
          setIsRefreshingStatuses(false);
          return;
        }

        const trailingQuery = startStatusQuery();
        if (trailing !== null) {
          trailingQuery.then(
            trailing.resolve,
            trailing.reject,
          );
        } else {
          void trailingQuery.catch(
            () => undefined,
          );
        }
      });
    inFlightRef.current = query;
    return query;
  }, [executeStatusQuery]);

  const refreshStatuses = useCallback(() => {
    if (inFlightRef.current === null) {
      return startStatusQuery();
    }

    if (trailingRef.current === null) {
      trailingRef.current = createDeferred();
    }
    return trailingRef.current.promise;
  }, [startStatusQuery]);

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
