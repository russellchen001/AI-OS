import { invoke } from "@tauri-apps/api/core";
import type { SystemMetrics } from "../types";

export async function fetchHealthStatus(): Promise<string> {
  return invoke<string>("health_check");
}

export async function fetchSystemMetrics(): Promise<SystemMetrics> {
  return invoke<SystemMetrics>("system_metrics");
}

export async function startAllServices(): Promise<string> {
  return invoke<string>("start_all");
}

export async function stopAllServices(): Promise<string> {
  return invoke<string>("stop_all");
}
