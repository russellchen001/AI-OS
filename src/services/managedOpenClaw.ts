import { invoke } from "@tauri-apps/api/core";

export type ManagedOpenClawState =
  | "not-installed"
  | "installed-not-configured"
  | "existing-unmanaged"
  | "configured-no-gateway"
  | "gateway-stopped"
  | "ready"
  | "broken";

export type ManagedOpenClawStatus = {
  state: ManagedOpenClawState;
  installed: boolean;
  configured: boolean;
  gatewayServiceInstalled: boolean;
  connected: boolean;
  version?: string;
  managed: boolean;
  credentialRequired: boolean;
  diagnostic?: string;
};

export type ManagedOpenClawInstallResult = {
  success: boolean;
  state: "ready" | "failed";
  message: string;
  version?: string;
};

export async function getManagedOpenClawStatus():
Promise<ManagedOpenClawStatus> {
  return invoke<ManagedOpenClawStatus>(
    "get_managed_openclaw_status",
  );
}

export async function installManagedOpenClaw():
Promise<ManagedOpenClawInstallResult> {
  return invoke<ManagedOpenClawInstallResult>(
    "install_managed_openclaw",
  );
}

export type ManagedOpenClawStartResult = {
  success: boolean;
  state: "ready" | "failed";
  message: string;
  version?: string;
};

export async function startManagedOpenClaw():
Promise<ManagedOpenClawStartResult> {
  return invoke<ManagedOpenClawStartResult>(
    "start_managed_openclaw",
  );
}

export async function completeManagedOpenClawSetup():
Promise<ManagedOpenClawInstallResult> {
  return invoke<ManagedOpenClawInstallResult>(
    "complete_managed_openclaw_setup",
  );
}

export async function adoptExistingOpenClaw(
  gatewayToken: string,
): Promise<ManagedOpenClawInstallResult> {
  return invoke<ManagedOpenClawInstallResult>(
    "adopt_existing_openclaw",
    {
      input: {
        gatewayToken,
      },
    },
  );
}

export async function finishManagedOpenClawSetup():
Promise<ManagedOpenClawInstallResult> {
  return invoke<ManagedOpenClawInstallResult>(
    "finish_managed_openclaw_setup",
  );
}

export async function repairManagedOpenClaw():
Promise<ManagedOpenClawInstallResult> {
  return invoke<ManagedOpenClawInstallResult>(
    "repair_managed_openclaw",
  );
}
