import { invoke } from "@tauri-apps/api/core";

export type MemoryEntry = {
  id: string;
  type: string;
  content: string;
  metadata?: unknown;
  createdAt: string;
  updatedAt: string;
};

export async function saveMemory(
  entry: MemoryEntry,
): Promise<void> {
  await invoke("save_memory", {
    entry,
  });
}

export async function listMemory(): Promise<MemoryEntry[]> {
  return invoke<MemoryEntry[]>("list_memory");
}

export async function deleteMemory(
  id: string,
): Promise<void> {
  await invoke("delete_memory", {
    id,
  });
}
