import { invoke } from "@tauri-apps/api/core";

import type {
  LogEntry,
  LogQuery,
  LogSource,
} from "../types/index";

export async function getLogs(
  query: LogQuery,
): Promise<LogEntry[]> {
  return invoke<LogEntry[]>(
    "get_logs",
    {
      query,
    },
  );
}

export async function clearLogs(
  source: LogSource | "All",
): Promise<string> {
  return invoke<string>(
    "clear_logs",
    {
      source,
    },
  );
}