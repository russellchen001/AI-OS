import {
  useCallback,
  useState,
} from "react";

import { fetchSystemMetrics } from "../services/tauri";

import type {
  SystemMetrics,
} from "../types";

const EMPTY_METRICS: SystemMetrics = {
  cpuUsage: 0,
  memoryUsedGb: 0,
  memoryTotalGb: 0,
  diskUsedGb: 0,
  diskTotalGb: 0,
};

export function useMetrics() {
  const [metrics, setMetrics] =
    useState<SystemMetrics>(
      EMPTY_METRICS,
    );

  const refreshMetrics =
    useCallback(async () => {
      try {
        const result =
          await fetchSystemMetrics();

        setMetrics(result);
      } catch {
        setMetrics(EMPTY_METRICS);
      }
    }, []);

  return {
    metrics,
    refreshMetrics,
  };
}