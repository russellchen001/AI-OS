import type {
  LogEntry,
  LogLevel,
} from "../types";

export function createLogId(): string {
  return `${Date.now()}-${Math.random()
    .toString(16)
    .slice(2)}`;
}

export function currentTime(): string {
  return new Date().toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function createLogEntry(
  level: LogLevel,
  message: string,
): LogEntry {
  return {
    id: createLogId(),
    timestamp: currentTime(),
    level,
    message,
  };
}

export function prependLog(
  logs: LogEntry[],
  level: LogLevel,
  message: string,
  limit = 500,
): LogEntry[] {
  return [
    createLogEntry(level, message),
    ...logs,
  ].slice(0, limit);
}