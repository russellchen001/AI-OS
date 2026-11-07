import { invoke } from "@tauri-apps/api/core";
import { getManagedOpenClawStatus, repairManagedOpenClaw } from "./managedOpenClaw";
import { initializeProviderInstances, listProviderInstances } from "./providers";

export type FirstLaunchPermission = {
  id: string;
  state: string;
  granted: boolean;
  description: string;
};

export type FirstLaunchDiagnostics = {
  providerConnected: boolean;
  openClawState: string;
  permissions: FirstLaunchPermission[];
  diagnostic?: string;
  vault?: { pid: number; readCalls: number; writeCalls: number; legacyReadCalls: number };
};

type PermissionResponse = {
  operationResult?: { permissions?: FirstLaunchPermission[] };
};

export async function runFirstLaunchDiagnostics(onConnection?: (result: FirstLaunchDiagnostics) => void): Promise<FirstLaunchDiagnostics> {
  await initializeProviderInstances();
  const providerConnected = listProviderInstances().some((provider) => provider.connectionState === "connected");
  onConnection?.({ providerConnected, openClawState: "checking", permissions: [] });
  const bounded = <T,>(promise: Promise<T>): Promise<T> => new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("Optional diagnostic timed out")), 2000);
    promise.then(value => { clearTimeout(timer); resolve(value); }, error => { clearTimeout(timer); reject(error); });
  });
  const [openClaw, permissionResponse, vault] = await Promise.allSettled([
    bounded(getManagedOpenClawStatus()),
    bounded(invoke<PermissionResponse>("list_macos_permission_statuses")),
    bounded(invoke<NonNullable<FirstLaunchDiagnostics["vault"]>>("get_credential_vault_diagnostics")),
  ]);

  return {
    providerConnected,
    vault: vault.status === "fulfilled" ? vault.value : undefined,
    openClawState: openClaw.status === "fulfilled" ? openClaw.value.state : "unavailable",
    permissions: permissionResponse.status === "fulfilled"
      ? permissionResponse.value.operationResult?.permissions ?? [] : [],
    diagnostic: [
      openClaw.status === "rejected" ? "OpenClaw check failed. Open Manage Agents to review it." : "",
      permissionResponse.status === "rejected" ? "Mac permission check failed. Run diagnostics again." : "",
    ].filter(Boolean).join(" ") || undefined,
  };
}

export async function openPermissionSettings(permission: string): Promise<void> {
  await invoke("open_macos_permission_settings", { permission });
}

export async function repairFirstLaunchOpenClaw(): Promise<FirstLaunchDiagnostics> {
  const result = await repairManagedOpenClaw();
  if (!result.success) throw new Error("OpenClaw repair did not complete. Open Manage Agents to review it.");
  return runFirstLaunchDiagnostics();
}
