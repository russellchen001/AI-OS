import {
  invoke,
} from "@tauri-apps/api/core";

import type {
  OpenClawActionResult,
  OpenClawConnectionResult,
  OpenClawDashboardSummary,
  OpenClawExportResult,
  OpenClawGatewayRequest,
  OpenClawGatewayResponse,
  OpenClawImportResult,
  OpenClawRemoteStatus,
  OpenClawRuntimeConfig,
  OpenClawServer,
  OpenClawServerInput,
} from "../types/index";

export async function listOpenClawServers():
Promise<OpenClawServer[]> {
  return invoke<OpenClawServer[]>(
    "list_openclaw_servers",
  );
}

export async function saveOpenClawServer(
  server: OpenClawServerInput,
): Promise<OpenClawActionResult> {
  return invoke<OpenClawActionResult>(
    "save_openclaw_server",
    {
      server,
    },
  );
}

export async function updateOpenClawServer(
  id: string,
  server: OpenClawServerInput,
): Promise<OpenClawActionResult> {
  return invoke<OpenClawActionResult>(
    "update_openclaw_server",
    {
      id,
      server,
    },
  );
}

export async function deleteOpenClawServer(
  id: string,
): Promise<OpenClawActionResult> {
  return invoke<OpenClawActionResult>(
    "delete_openclaw_server",
    {
      id,
    },
  );
}

export async function duplicateOpenClawServer(
  id: string,
): Promise<OpenClawActionResult> {
  return invoke<OpenClawActionResult>(
    "duplicate_openclaw_server",
    {
      id,
    },
  );
}

export async function toggleOpenClawServer(
  id: string,
  enabled: boolean,
): Promise<OpenClawActionResult> {
  return invoke<OpenClawActionResult>(
    "toggle_openclaw_server",
    {
      id,
      enabled,
    },
  );
}

export async function setActiveOpenClawServer(
  id: string,
): Promise<OpenClawActionResult> {
  return invoke<OpenClawActionResult>(
    "set_active_openclaw_server",
    {
      id,
    },
  );
}

export async function testOpenClawConnection(
  id: string,
): Promise<OpenClawConnectionResult> {
  return invoke<OpenClawConnectionResult>(
    "test_openclaw_connection",
    {
      id,
    },
  );
}

export async function testOpenClawConnectionInput(
  server: OpenClawServerInput,
): Promise<OpenClawConnectionResult> {
  return invoke<OpenClawConnectionResult>(
    "test_openclaw_connection_input",
    {
      server,
    },
  );
}

export async function testAllOpenClawServers():
Promise<OpenClawConnectionResult[]> {
  return invoke<OpenClawConnectionResult[]>(
    "test_all_openclaw_servers",
  );
}

export async function getActiveOpenClawStatus():
Promise<OpenClawRemoteStatus> {
  return invoke<OpenClawRemoteStatus>(
    "get_active_openclaw_status",
  );
}

export async function getOpenClawDashboardSummary():
Promise<OpenClawDashboardSummary> {
  return invoke<OpenClawDashboardSummary>(
    "get_openclaw_dashboard_summary",
  );
}

export async function getOpenClawRuntimeConfig():
Promise<OpenClawRuntimeConfig> {
  return invoke<OpenClawRuntimeConfig>(
    "get_openclaw_runtime_config",
  );
}

export async function exportOpenClawServers(
  includeSecrets = false,
): Promise<OpenClawExportResult> {
  return invoke<OpenClawExportResult>(
    "export_openclaw_servers",
    {
      includeSecrets,
    },
  );
}

export async function importOpenClawServers(
  json: string,
  replaceExisting = false,
): Promise<OpenClawImportResult> {
  return invoke<OpenClawImportResult>(
    "import_openclaw_servers",
    {
      json,
      replaceExisting,
    },
  );
}

export async function invokeActiveOpenClawGateway<
  T = unknown,
>(
  request: OpenClawGatewayRequest,
): Promise<OpenClawGatewayResponse<T>> {
  return invoke<
    OpenClawGatewayResponse<T>
  >(
    "invoke_active_openclaw_gateway",
    {
      request,
    },
  );
}