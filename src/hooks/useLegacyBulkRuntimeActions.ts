import {
  useCallback,
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
  hasCanonicalActivity: boolean;
  refreshStatuses: () => Promise<void>;
  notify: (message: string) => void;
};

// Bulk behavior remains on the legacy boundary pending a separately approved design.
export default function useLegacyBulkRuntimeActions({
  allRunning,
  hasCanonicalActivity,
  refreshStatuses,
  notify,
}: UseLegacyBulkRuntimeActionsOptions) {
  const [globalAction, setGlobalAction] =
    useState<LegacyBulkAction>(null);

  const startAll = useCallback(async () => {
    try {
      setGlobalAction("start");
      notify("🚀 Starting services...");
      notify(await startAllServices());

      for (const delay of [
        5000,
        20000,
        45000,
      ]) {
        window.setTimeout(() => {
          void refreshStatuses().catch(
            () => undefined,
          );
        }, delay);
      }
    } catch {
      notify("Start All failed.");
    } finally {
      setGlobalAction(null);
    }
  }, [
    notify,
    refreshStatuses,
  ]);

  const stopAll = useCallback(async () => {
    try {
      setGlobalAction("stop");
      notify("🛑 Stopping services...");
      notify(await stopAllServices());
      window.setTimeout(() => {
        void refreshStatuses().catch(
          () => undefined,
        );
      }, 8000);
    } catch {
      notify("Stop All failed.");
    } finally {
      setGlobalAction(null);
    }
  }, [
    notify,
    refreshStatuses,
  ]);

  const handleGlobalToggle = useCallback(
    () => {
      if (
        globalAction !== null ||
        hasCanonicalActivity
      ) {
        return;
      }
      void (allRunning
        ? stopAll()
        : startAll());
    },
    [
      allRunning,
      globalAction,
      hasCanonicalActivity,
      startAll,
      stopAll,
    ],
  );

  return {
    globalAction,
    handleGlobalToggle,
  };
}
