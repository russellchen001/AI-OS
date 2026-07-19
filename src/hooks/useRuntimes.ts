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
  const [lastUpdated, setLastUpdated] =
    useState("Not checked");

  const refreshStatuses = useCallback(
    async () => {
      const nextStatuses =
        await getRuntimeStatuses({
          ollamaUrl,
          openWebUiUrl,
        });
      setStatuses(nextStatuses);
      setError("");
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
    }, [
      ollamaUrl,
      openWebUiUrl,
    ],
  );

  const refresh = useCallback(
    async () => {
      try {
        setIsLoading(true);
        setError("");

        const [definitions] =
          await Promise.all([
            listRuntimes(),
            refreshStatuses(),
          ]);

        setRuntimes(definitions);
      } catch {
        setError(
          "Runtime status is unavailable.",
        );
      } finally {
        setIsLoading(false);
      }
    }, [
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
          () => {
            setError(
              "Runtime status is unavailable.",
            );
          },
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
    isLoading,
    error,
    lastUpdated,
    refresh,
    refreshStatuses,
  };
}

export default useRuntimes;
