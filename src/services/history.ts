import type {
  ConversationRecord,
  HistoryProviderId,
} from "../types/history";

const STORAGE_KEY =
  "ai-os.multillm.history.v1";

type StoredConversation =
  Partial<ConversationRecord> & {
    responses?: Partial<
      Record<
        HistoryProviderId,
        string
      >
    >;
  };

function createTitle(
  prompt: string,
): string {
  const value =
    prompt.trim() || "Untitled";

  return value.length > 60
    ? `${value.slice(0, 60)}…`
    : value;
}

function normalizeRecord(
  record: StoredConversation,
): ConversationRecord {
  const createdAt =
    typeof record.createdAt ===
    "number"
      ? record.createdAt
      : Date.now();

  const prompt =
    typeof record.prompt ===
    "string"
      ? record.prompt
      : "";

  return {
    id:
      typeof record.id === "string"
        ? record.id
        : crypto.randomUUID(),
    createdAt,
    updatedAt:
      typeof record.updatedAt ===
      "number"
        ? record.updatedAt
        : createdAt,
    mode:
      record.mode === "router"
        ? "router"
        : "compare",
    title:
      typeof record.title ===
        "string" &&
      record.title.trim()
        ? record.title
        : createTitle(prompt),
    prompt,
    routedProviderId:
      record.routedProviderId,
    favorite:
      record.favorite ?? false,
    tags: Array.isArray(record.tags)
      ? record.tags.filter(
          (
            tag,
          ): tag is string =>
            typeof tag === "string",
        )
      : [],
    responses:
      record.responses ?? {},
  };
}

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

    const parsed: unknown =
      JSON.parse(raw);

    if (!Array.isArray(parsed)) {
      return [];
    }

    const normalized =
      parsed.map((record) =>
        normalizeRecord(
          record as StoredConversation,
        ),
      );

    // Persist migrated records.
    saveHistory(normalized);

    return normalized;
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

export function updateHistory(
  id: string,
  updater: (
    record: ConversationRecord,
  ) => ConversationRecord,
): ConversationRecord[] {
  const next =
    loadHistory().map(
      (record) =>
        record.id === id
          ? {
              ...updater(record),
              updatedAt: Date.now(),
            }
          : record,
    );

  saveHistory(next);
  return next;
}

export function deleteHistory(
  id: string,
): ConversationRecord[] {
  const next =
    loadHistory().filter(
      (record) =>
        record.id !== id,
    );

  saveHistory(next);
  return next;
}
