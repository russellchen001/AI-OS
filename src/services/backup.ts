import { invoke } from "@tauri-apps/api/core";

import type {
  BackupRecord,
  BackupResult,
  CreateBackupRequest,
  RestoreBackupRequest,
} from "../types/index";

export async function createBackup(
  request: CreateBackupRequest,
): Promise<BackupResult> {
  return invoke<BackupResult>(
    "create_backup",
    {
      request,
    },
  );
}

export async function restoreBackup(
  request: RestoreBackupRequest,
): Promise<BackupResult> {
  return invoke<BackupResult>(
    "restore_backup",
    {
      request,
    },
  );
}

export async function listBackups(
  directory: string,
): Promise<BackupRecord[]> {
  if (!directory.trim()) {
    return [];
  }

  return invoke<BackupRecord[]>(
    "list_backups",
    {
      directory,
    },
  );
}

export async function revealBackup(
  archivePath: string,
): Promise<string> {
  return invoke<string>(
    "reveal_backup",
    {
      archivePath,
    },
  );
}

export async function deleteBackup(
  archivePath: string,
): Promise<string> {
  return invoke<string>(
    "delete_backup",
    {
      archivePath,
    },
  );
}