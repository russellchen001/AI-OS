import { invoke } from "@tauri-apps/api/core";
import type { SystemMetrics } from "../types";

export type OpenServiceSettings = {
  openClawUrl: string;
  openWebUiUrl: string;
  ollamaUrl: string;
};

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

export async function startSingleService(
  service: string,
): Promise<string> {
  return invoke<string>("start_service", {
    service,
  });
}

export async function stopSingleService(
  service: string,
): Promise<string> {
  return invoke<string>("stop_service", {
    service,
  });
}

export async function openSingleService(
  service: string,
  settings: OpenServiceSettings,
): Promise<string> {
  return invoke<string>("open_service", {
    service,
    openClawUrl: settings.openClawUrl,
    openWebUiUrl: settings.openWebUiUrl,
    ollamaUrl: settings.ollamaUrl,
  });
}