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
  endpointRef.current = {
    ollamaUrl,
    openWebUiUrl,
  };

  const inFlightRef = useRef<
    Promise<void> | null
  >(null);
  const trailingRef = useRef<Deferred | null>(
    null,
  );

  const executeStatusQuery = useCallback(
    async () => {
      const endpoints = endpointRef.current;
      const nextStatuses =
        await getRuntimeStatuses({
          ollamaUrl: endpoints.ollamaUrl,
          openWebUiUrl:
            endpoints.openWebUiUrl,
        });
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
    },
    [],
  );

  const startStatusQuery = useCallback(() => {
    setIsRefreshingStatuses(true);
    const query = executeStatusQuery()
      .catch(() => {
        setStatusError(
          "Runtime status is unavailable.",
        );
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
        if (trailing === null) {
          setIsRefreshingStatuses(false);
          return;
        }

        const trailingQuery = startStatusQuery();
        trailingQuery.then(
          trailing.resolve,
          trailing.reject,
        );
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
    void refresh();

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
    refresh,
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
