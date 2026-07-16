import type {
  ConversationRecord,
} from "../types/history";

const STORAGE_KEY =
  "ai-os.multillm.history.v1";

export function loadHistory():
  ConversationRecord[] {
  try {
    const raw =
      localStorage.getItem(
        STORAGE_KEY,
      );

    if (!raw) {
      return [];
    }

    return JSON.parse(
      raw,
    ) as ConversationRecord[];
  } catch {
    return [];
  }
}

export function saveHistory(
  history: ConversationRecord[],
): void {
  localStorage.setItem(
    STORAGE_KEY,
    JSON.stringify(history),
  );
}
