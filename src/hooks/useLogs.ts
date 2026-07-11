import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import {
  clearLogs,
  getLogs,
} from "../services/logs";

import type {
  LogEntry,
  LogLevel,
  LogSource,
} from "../types/index";

type UseLogsOptions = {
  lineLimit: number;
  refreshInterval: number;
  onMessage: (
    message: string,
  ) => void;
};

function useLogs({
  lineLimit,
  refreshInterval,
  onMessage,
}: UseLogsOptions) {
  const [
    logs,
    setLogs,
  ] = useState<LogEntry[]>([]);

  const [
    selectedSource,
    setSelectedSource,
  ] = useState<
    LogSource | "All"
  >("All");

  const [
    selectedLevel,
    setSelectedLevel,
  ] = useState<
    LogLevel | "All"
  >("All");

  const [
    searchText,
    setSearchText,
  ] = useState("");

  const [
    isLoading,
    setIsLoading,
  ] = useState(false);

  const [
    isAutoRefresh,
    setIsAutoRefresh,
  ] = useState(true);

  const [
    error,
    setError,
  ] = useState("");

  const requestIdRef =
    useRef(0);

  const refreshLogs =
    useCallback(
      async (
        showLoading = true,
      ) => {
        const requestId =
          requestIdRef.current + 1;

        requestIdRef.current =
          requestId;

        try {
          if (showLoading) {
            setIsLoading(true);
          }

          setError("");

          const result =
            await getLogs({
              source:
                selectedSource,
              level:
                selectedLevel,
              limit:
                Math.max(
                  lineLimit,
                  1,
                ),
            });

          if (
            requestId ===
            requestIdRef.current
          ) {
            setLogs(result);
          }
        } catch (nextError) {
          if (
            requestId !==
            requestIdRef.current
          ) {
            return;
          }

          const message =
            `Unable to load logs: ${String(
              nextError,
            )}`;

          setError(message);
        } finally {
          if (
            showLoading &&
            requestId ===
              requestIdRef.current
          ) {
            setIsLoading(false);
          }
        }
      },
      [
        lineLimit,
        selectedLevel,
        selectedSource,
      ],
    );

  useEffect(() => {
    refreshLogs();
  }, [refreshLogs]);

  useEffect(() => {
    if (!isAutoRefresh) {
      return undefined;
    }

    const interval =
      window.setInterval(
        () => {
          refreshLogs(false);
        },
        Math.max(
          refreshInterval,
          2,
        ) * 1000,
      );

    return () => {
      window.clearInterval(
        interval,
      );
    };
  }, [
    isAutoRefresh,
    refreshInterval,
    refreshLogs,
  ]);

  const filteredLogs =
    useMemo(() => {
      const normalizedSearch =
        searchText
          .trim()
          .toLowerCase();

      if (!normalizedSearch) {
        return logs;
      }

      return logs.filter(
        (entry) =>
          entry.message
            .toLowerCase()
            .includes(
              normalizedSearch,
            ) ||
          entry.source
            .toLowerCase()
            .includes(
              normalizedSearch,
            ) ||
          entry.level
            .toLowerCase()
            .includes(
              normalizedSearch,
            ),
      );
    }, [
      logs,
      searchText,
    ]);

  const removeLogs =
    useCallback(async () => {
      try {
        setIsLoading(true);
        setError("");

        const result =
          await clearLogs(
            selectedSource,
          );

        setLogs([]);
        onMessage(
          `🧹 ${result}`,
        );
      } catch (nextError) {
        const message =
          `Unable to clear logs: ${String(
            nextError,
          )}`;

        setError(message);
        onMessage(
          `❌ ${message}`,
        );
      } finally {
        setIsLoading(false);
      }
    }, [
      onMessage,
      selectedSource,
    ]);

  return {
    logs,
    filteredLogs,
    selectedSource,
    selectedLevel,
    searchText,
    isLoading,
    isAutoRefresh,
    error,
    setSelectedSource,
    setSelectedLevel,
    setSearchText,
    setIsAutoRefresh,
    refreshLogs,
    removeLogs,
  };
}

export default useLogs;