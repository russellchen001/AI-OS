import {
  useCallback,
  useEffect,
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
  const ollamaUrl =
    request?.ollamaUrl;
  const openWebUiUrl =
    request?.openWebUiUrl;

  const [runtimes, setRuntimes] =
    useState<RuntimeDefinition[]>([]);
  const [statuses, setStatuses] =
    useState<RuntimeStatus[]>([]);
  const [isLoading, setIsLoading] =
    useState(false);
  const [error, setError] =
    useState("");

  const refresh = useCallback(
    async () => {
      try {
        setIsLoading(true);
        setError("");

        const [definitions, nextStatuses] =
          await Promise.all([
            listRuntimes(),
            getRuntimeStatuses({
              ollamaUrl,
              openWebUiUrl,
            }),
          ]);

        setRuntimes(definitions);
        setStatuses(nextStatuses);
      } catch (nextError) {
        setError(String(nextError));
      } finally {
        setIsLoading(false);
      }
    }, [
      ollamaUrl,
      openWebUiUrl,
    ]);

  useEffect(() => {
    void refresh();

    if (!refreshInterval) {
      return;
    }

    const interval = window.setInterval(
      () => {
        void refresh();
      },
      Math.max(refreshInterval, 2) * 1000,
    );

    return () => {
      window.clearInterval(interval);
    };
  }, [refresh, refreshInterval]);

  return {
    runtimes,
    statuses,
    isLoading,
    error,
    refresh,
  };
}

export default useRuntimes;
