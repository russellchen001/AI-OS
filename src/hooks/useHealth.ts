import {
  useCallback,
  useState,
} from "react";

import { fetchHealthStatus } from "../services/tauri";

import type {
  LogLevel,
  Service,
} from "../types";

type AddLog = (
  level: LogLevel,
  message: string,
) => void;

type UseHealthOptions = {
  setServices: React.Dispatch<
    React.SetStateAction<Service[]>
  >;
  setMessage: React.Dispatch<
    React.SetStateAction<string>
  >;
  addLog: AddLog;
};

function getCurrentTime(): string {
  return new Date().toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function useHealth({
  setServices,
  setMessage,
  addLog,
}: UseHealthOptions) {
  const [isChecking, setIsChecking] =
    useState(false);

  const [lastUpdated, setLastUpdated] =
    useState("Not checked");

  const healthCheck = useCallback(
    async (showMessage = true) => {
      try {
        setIsChecking(true);

        const result =
          await fetchHealthStatus();

        if (showMessage) {
          setMessage(result);
        }

        setServices((currentServices) =>
          currentServices.map((service) => ({
            ...service,
            status: result.includes(
              `${service.name}: 🟢`,
            )
              ? "Running"
              : "Stopped",
          })),
        );

        setLastUpdated(getCurrentTime());
      } catch (error) {
        const errorMessage =
          `Health Check failed: ${String(error)}`;

        setMessage(errorMessage);
        addLog("error", errorMessage);
      } finally {
        setIsChecking(false);
      }
    },
    [
      addLog,
      setMessage,
      setServices,
    ],
  );

  return {
    healthCheck,
    isChecking,
    lastUpdated,
  };
}