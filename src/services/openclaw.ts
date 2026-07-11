import { invoke } from "@tauri-apps/api/core";

import type {
  OpenClawActionResult,
  OpenClawConnectionResult,
  OpenClawRemoteStatus,
  OpenClawServer,
  OpenClawServerInput,
} from "../types/index";

export async function listOpenClawServers(): Promise<
  OpenClawServer[]
> {
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

export async function getActiveOpenClawStatus(): Promise<
  OpenClawRemoteStatus
> {
  return invoke<OpenClawRemoteStatus>(
    "get_active_openclaw_status",
  );
}