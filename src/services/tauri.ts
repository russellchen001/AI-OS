import { invoke } from "@tauri-apps/api/core";
import type { SystemMetrics } from "../types";

export async function fetchHealthStatus(): Promise<string> {
  return invoke<string>("health_check");
}

export async function fetchSystemMetrics(): Promise<SystemMetrics> {
  return invoke<SystemMetrics>("system_metrics");
}
