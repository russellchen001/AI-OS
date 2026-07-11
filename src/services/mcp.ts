import { invoke } from "@tauri-apps/api/core";

import type {
  McpActionResult,
  McpServer,
  McpServerInput,
} from "../types/index";

export async function listMcpServers(): Promise<
  McpServer[]
> {
  return invoke<McpServer[]>(
    "list_mcp_servers",
  );
}

export async function saveMcpServer(
  server: McpServerInput,
): Promise<McpActionResult> {
  return invoke<McpActionResult>(
    "save_mcp_server",
    {
      server,
    },
  );
}

export async function updateMcpServer(
  id: string,
  server: McpServerInput,
): Promise<McpActionResult> {
  return invoke<McpActionResult>(
    "update_mcp_server",
    {
      id,
      server,
    },
  );
}

export async function toggleMcpServer(
  id: string,
  enabled: boolean,
): Promise<McpActionResult> {
  return invoke<McpActionResult>(
    "toggle_mcp_server",
    {
      id,
      enabled,
    },
  );
}

export async function deleteMcpServer(
  id: string,
): Promise<McpActionResult> {
  return invoke<McpActionResult>(
    "delete_mcp_server",
    {
      id,
    },
  );
}