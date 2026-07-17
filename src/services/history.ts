import type {
  ConversationRecord,
  HistoryCategory,
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

export function classifyHistoryCategory(
  value: string,
): HistoryCategory {
  const normalized =
    value.toLowerCase();

  if (
    /code|coding|program|debug|typescript|javascript|python|rust|java|golang|sql|api|docker|git|react|css|html/.test(
      normalized,
    )
  ) {
    return "coding";
  }

  if (
    /write|writing|rewrite|story|poem|blog|article|marketing|copywriting|slogan|email|essay/.test(
      normalized,
    )
  ) {
    return "writing";
  }

  if (
    /math|calculate|solve|equation|proof|algebra|geometry|calculus|integral|derivative|matrix|probability/.test(
      normalized,
    ) ||
    /[0-9x-y]\s*[\+\-\*\/=]\s*[0-9x-y]/i.test(
      value,
    ) ||
    /[²³√∫∑π∞]/.test(value)
  ) {
    return "math";
  }

  return "general";
}

function normalizeRecord(
  record: StoredConversation,
): ConversationRecord {
  const createdAt =
    typeof record.createdAt ===
    "number"
      ? record.createdAt
      : Date.now();

  const updatedAt =
    typeof record.updatedAt ===
    "number"
      ? record.updatedAt
      : createdAt;

  const prompt =
    typeof record.prompt ===
    "string"
      ? record.prompt
      : "";

  const category:
    HistoryCategory =
    record.category === "coding" ||
    record.category === "writing" ||
    record.category === "math" ||
    record.category === "general"
      ? record.category
      : classifyHistoryCategory(
          prompt,
        );

  return {
    id:
      typeof record.id === "string"
        ? record.id
        : crypto.randomUUID(),
    createdAt,
    updatedAt,
    lastOpenedAt:
      typeof record.lastOpenedAt ===
      "number"
        ? record.lastOpenedAt
        : updatedAt,
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
    category,
    favorite:
      record.favorite ?? false,
    pinned:
      record.pinned ?? false,
    tags: Array.isArray(record.tags)
      ? Array.from(
          new Set(
            record.tags
              .filter(
                (
                  tag,
                ): tag is string =>
                  typeof tag ===
                  "string",
              )
              .map((tag) =>
                tag.trim(),
              )
              .filter(Boolean),
          ),
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
