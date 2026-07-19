import {
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";

import {
  startAllServices,
  stopAllServices,
} from "../services/tauri";

type LegacyBulkAction =
  | "start"
  | "stop"
  | null;

type UseLegacyBulkRuntimeActionsOptions = {
  allRunning: boolean;
  refreshStatuses: () => Promise<void>;
  notify: (message: string) => void;
  isCanonicalActivityActive: () => boolean;
};

// Bulk behavior remains on the legacy boundary pending a separately approved design.
export default function useLegacyBulkRuntimeActions({
  allRunning,
  refreshStatuses,
  notify,
  isCanonicalActivityActive,
}: UseLegacyBulkRuntimeActionsOptions) {
  const [globalAction, setGlobalAction] =
    useState<LegacyBulkAction>(null);
  const [bulkIsolationActive, setBulkIsolationActive] =
    useState(false);
  const bulkIsolationRef = useRef(false);
  const mountedRef = useRef(true);
  const timeoutHandlesRef = useRef(
    new Set<number>(),
  );

  const setIsolation = useCallback(
    (active: boolean) => {
      bulkIsolationRef.current = active;
      if (mountedRef.current) {
        setBulkIsolationActive(active);
      }
    },
    [],
  );

  const scheduleRefresh = useCallback(
    (
      delay: number,
      releaseAfter: boolean,
    ) => {
      const handle = window.setTimeout(
        () => {
          timeoutHandlesRef.current.delete(
            handle,
          );
          void refreshStatuses()
            .catch(() => undefined)
            .finally(() => {
              if (releaseAfter) {
                setIsolation(false);
              }
            });
        },
        delay,
      );
      timeoutHandlesRef.current.add(handle);
    },
    [
      refreshStatuses,
      setIsolation,
    ],
  );

  const isBulkIsolationActive = useCallback(
    () => bulkIsolationRef.current,
    [],
  );

  const startAll = useCallback(async () => {
    if (
      bulkIsolationRef.current ||
      isCanonicalActivityActive()
    ) {
      return;
    }

    setIsolation(true);
    try {
      setGlobalAction("start");
      notify("🚀 Starting services...");
      const result = await startAllServices();
      if (!mountedRef.current) {
        return;
      }
      notify(result);
      scheduleRefresh(5000, false);
      scheduleRefresh(20000, false);
      scheduleRefresh(45000, true);
    } catch {
      setIsolation(false);
      if (mountedRef.current) {
        notify("Start All failed.");
      }
    } finally {
      if (mountedRef.current) {
        setGlobalAction(null);
      }
    }
  }, [
    isCanonicalActivityActive,
    notify,
    scheduleRefresh,
    setIsolation,
  ]);

  const stopAll = useCallback(async () => {
    if (
      bulkIsolationRef.current ||
      isCanonicalActivityActive()
    ) {
      return;
    }

    setIsolation(true);
    try {
      setGlobalAction("stop");
      notify("🛑 Stopping services...");
      const result = await stopAllServices();
      if (!mountedRef.current) {
        return;
      }
      notify(result);
      scheduleRefresh(8000, true);
    } catch {
      setIsolation(false);
      if (mountedRef.current) {
        notify("Stop All failed.");
      }
    } finally {
      if (mountedRef.current) {
        setGlobalAction(null);
      }
    }
  }, [
    isCanonicalActivityActive,
    notify,
    scheduleRefresh,
    setIsolation,
  ]);

  const handleGlobalToggle = useCallback(
    () => {
      if (
        bulkIsolationRef.current ||
        isCanonicalActivityActive()
      ) {
        return;
      }
      void (allRunning
        ? stopAll()
        : startAll());
    },
    [
      allRunning,
      isCanonicalActivityActive,
      startAll,
      stopAll,
    ],
  );

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      bulkIsolationRef.current = false;
      for (const handle of
        timeoutHandlesRef.current) {
        window.clearTimeout(handle);
      }
      timeoutHandlesRef.current.clear();
    };
  }, []);

  return {
    globalAction,
    bulkIsolationActive,
    isBulkIsolationActive,
    handleGlobalToggle,
  };
}
