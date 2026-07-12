import {
  useCallback,
  useMemo,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";

import { INITIAL_SERVICES } from "../config/constants";
import type {
  Service,
  Settings,
} from "../types/index";

type UseServicesOptions = {
  settings: Settings;
  onMessage: (
    message: string,
  ) => void;
};

function useServices({
  settings,
  onMessage,
}: UseServicesOptions) {
  const [services, setServices] =
    useState<Service[]>(
      INITIAL_SERVICES,
    );

  const [
    lastUpdated,
    setLastUpdated,
  ] = useState("Not checked");

  const [
    isChecking,
    setIsChecking,
  ] = useState(false);

  const [
    globalAction,
    setGlobalAction,
  ] = useState<
    "start" | "stop" | null
  >(null);

  const [
    serviceAction,
    setServiceAction,
  ] = useState<string | null>(
    null,
  );

  const [
    openAction,
    setOpenAction,
  ] = useState<string | null>(
    null,
  );

  const isBusy =
    globalAction !== null ||
    serviceAction !== null;

  const healthCheck = useCallback(
    async (
      showMessage = true,
    ) => {
      try {
        setIsChecking(true);

        const result =
  await invoke<string>(
    "health_check",
    {
      openclawUrl:
        settings.openClawUrl,

      ollamaUrl:
        settings.ollamaUrl,

      openWebUiUrl:
        settings.openWebUiUrl,
    },
  );

        if (showMessage) {
          onMessage(result);
        }

        setServices((current) =>
          current.map(
            (service) => ({
              ...service,

              status:
                result.includes(
                  `${service.name}: 🟢`,
                )
                  ? "Running"
                  : "Stopped",
            }),
          ),
        );

        setLastUpdated(
          new Date()
            .toLocaleTimeString(
              [],
              {
                hour: "2-digit",
                minute:
                  "2-digit",
                second:
                  "2-digit",
              },
            ),
        );
      } catch (error) {
        onMessage(
          `Health Check failed: ${String(
            error,
          )}`,
        );
      } finally {
        setIsChecking(false);
      }
    },
    [
  onMessage,
  settings.openClawUrl,
  settings.ollamaUrl,
  settings.openWebUiUrl,
],
  );

  const startAll =
    useCallback(async () => {
      try {
        setGlobalAction(
          "start",
        );

        onMessage(
          "🚀 Starting services...",
        );

        const result =
          await invoke<string>(
            "start_all",
          );

        onMessage(result);

        window.setTimeout(
          () => {
            healthCheck(false);
          },
          5000,
        );

        window.setTimeout(
          () => {
            healthCheck(false);
          },
          20000,
        );

        window.setTimeout(
          () => {
            healthCheck(false);
          },
          45000,
        );
      } catch (error) {
        onMessage(
          `Start All failed: ${String(
            error,
          )}`,
        );
      } finally {
        setGlobalAction(null);
      }
    }, [
      healthCheck,
      onMessage,
    ]);

  const stopAll =
    useCallback(async () => {
      try {
        setGlobalAction(
          "stop",
        );

        onMessage(
          "🛑 Stopping services...",
        );

        const result =
          await invoke<string>(
            "stop_all",
          );

        onMessage(result);

        window.setTimeout(
          () => {
            healthCheck(false);
          },
          8000,
        );
      } catch (error) {
        onMessage(
          `Stop All failed: ${String(
            error,
          )}`,
        );
      } finally {
        setGlobalAction(null);
      }
    }, [
      healthCheck,
      onMessage,
    ]);

  const startService =
    useCallback(
      async (
        service: string,
      ) => {
        try {
          setServiceAction(
            `start:${service}`,
          );

          onMessage(
            `🚀 Starting ${service}...`,
          );

          const result =
            await invoke<string>(
              "start_service",
              {
                service,
              },
            );

          onMessage(result);

          const delay =
            service === "Docker"
              ? 20000
              : service ===
                  "Open WebUI"
                ? 8000
                : 3000;

          window.setTimeout(
            () => {
              healthCheck(false);
            },
            delay,
          );
        } catch (error) {
          onMessage(
            `Failed to start ${service}: ${String(
              error,
            )}`,
          );
        } finally {
          setServiceAction(
            null,
          );
        }
      },
      [
        healthCheck,
        onMessage,
      ],
    );

  const stopService =
    useCallback(
      async (
        service: string,
      ) => {
        try {
          setServiceAction(
            `stop:${service}`,
          );

          onMessage(
            `🛑 Stopping ${service}...`,
          );

          const result =
            await invoke<string>(
              "stop_service",
              {
                service,
              },
            );

          onMessage(result);

          const delay =
            service === "Docker"
              ? 8000
              : service ===
                  "Open WebUI"
                ? 4000
                : 2500;

          window.setTimeout(
            () => {
              healthCheck(false);
            },
            delay,
          );
        } catch (error) {
          onMessage(
            `Failed to stop ${service}: ${String(
              error,
            )}`,
          );
        } finally {
          setServiceAction(
            null,
          );
        }
      },
      [
        healthCheck,
        onMessage,
      ],
    );

  const openService =
    useCallback(
      async (
        service: string,
      ) => {
        try {
          setOpenAction(
            service,
          );

          const result =
            await invoke<string>(
              "open_service",
              {
                service,

                openclawUrl:
                  settings.openClawUrl,

                ollamaUrl:
                  settings.ollamaUrl,

                openWebUiUrl:
                  settings.openWebUiUrl,
              },
            );

          onMessage(result);
        } catch (error) {
          onMessage(
            `Failed to open ${service}: ${String(
              error,
            )}`,
          );
        } finally {
          setOpenAction(null);
        }
      },
      [
        onMessage,
        settings.openClawUrl,
        settings.ollamaUrl,
        settings.openWebUiUrl,
      ],
    );

  const runningCount =
    useMemo(
      () =>
        services.filter(
          (service) =>
            service.status ===
            "Running",
        ).length,
      [services],
    );

  const stoppedCount =
    useMemo(
      () =>
        services.filter(
          (service) =>
            service.status ===
            "Stopped",
        ).length,
      [services],
    );

  const unknownCount =
    useMemo(
      () =>
        services.filter(
          (service) =>
            service.status ===
            "Unknown",
        ).length,
      [services],
    );

  const allRunning =
    services.length > 0 &&
    runningCount ===
      services.length;

  const handleGlobalToggle =
    useCallback(() => {
      if (isBusy) {
        return;
      }

      if (allRunning) {
        stopAll();
      } else {
        startAll();
      }
    }, [
      allRunning,
      isBusy,
      startAll,
      stopAll,
    ]);

  return {
    services,
    lastUpdated,
    isChecking,
    globalAction,
    serviceAction,
    openAction,
    isBusy,
    runningCount,
    stoppedCount,
    unknownCount,
    allRunning,
    healthCheck,
    startService,
    stopService,
    openService,
    handleGlobalToggle,
  };
}

export default useServices;