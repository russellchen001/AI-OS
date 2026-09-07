import { invoke } from "@tauri-apps/api/core";

export type LocalMediaReadiness =
  | "not-installed"
  | "installed-not-configured"
  | "installed-broken"
  | "ready";

export type ComfyUiManagedProfileSetupResult = {
  profileId: string;
  action:
    | "installed"
    | "repaired"
    | "already-installed"
    | "adopted";
  checkpointFile: string;
  checkpointPath: string;
  backupPath?: string | null;
  sizeBytes: number;
  sha256: string;
  license: string;
  workflowReady: boolean;
  requiredAssetsReady: boolean;
  customNodesReady: boolean;
  integrityOk: boolean;
  smokeGenerationChecked: boolean;
  outputRetrievalChecked: boolean;
  readiness: LocalMediaReadiness;
};

export async function setupComfyUiManagedProfile(
  confirmed: boolean,
): Promise<ComfyUiManagedProfileSetupResult> {
  return invoke<ComfyUiManagedProfileSetupResult>(
    "setup_comfyui_managed_profile",
    { confirmed },
  );
}
