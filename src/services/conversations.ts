import { invoke } from "@tauri-apps/api/core";
import type { AiCenterInvocationMetadata } from "./aiCenterObservability";

export const CONVERSATIONS_CHANGED_EVENT = "ai-os:conversations-changed";
const CONVERSATIONS_KEY = "ai-os.conversations.v1";
const DEFAULT_CONTEXT_TOKENS = 32_000;
let conversationCache: Conversation[] | undefined;
let initialization: Promise<Conversation[]> | undefined;

export type ConversationAttachment = {
  id: string;
  name: string;
  mimeType: string;
  size: number;
  text?: string;
};

export type ConversationMessage = {
  id: string;
  role: "user" | "assistant";
  content: string;
  createdAt: string;
  attachments?: ConversationAttachment[];
  invocation?: AiCenterInvocationMetadata;
};

export type Conversation = {
  id: string;
  title: string;
  messages: ConversationMessage[];
  summary?: string;
  createdAt: string;
  updatedAt: string;
};

function notifyChanged(): void {
  window.dispatchEvent(new Event(CONVERSATIONS_CHANGED_EVENT));
}

function isConversation(value: unknown): value is Conversation {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<Conversation>;
  return (
    typeof candidate.id === "string" &&
    typeof candidate.title === "string" &&
    Array.isArray(candidate.messages) &&
    typeof candidate.createdAt === "string" &&
    typeof candidate.updatedAt === "string"
  );
}

function readLegacyConversations(): Conversation[] {
  try {
    const parsed: unknown = JSON.parse(
      localStorage.getItem(CONVERSATIONS_KEY) ?? "[]",
    );
    return Array.isArray(parsed)
      ? parsed
          .filter(isConversation)
          .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
      : [];
  } catch {
    return [];
  }
}

export function listConversations(): Conversation[] {
  conversationCache ??= readLegacyConversations();
  return [...conversationCache].sort((left, right) =>
    right.updatedAt.localeCompare(left.updatedAt),
  );
}

export function initializeConversations(): Promise<Conversation[]> {
  initialization ??= (async () => {
    const legacy = readLegacyConversations();
    try {
      let native = (await invoke<unknown[]>("list_native_conversations"))
        .filter(isConversation);
      if (native.length === 0 && legacy.length > 0) {
        await invoke<number>("import_native_conversations", {
          conversations: legacy,
        });
        native = (await invoke<unknown[]>("list_native_conversations"))
          .filter(isConversation);
      }
      conversationCache = native;
    } catch {
      conversationCache = legacy;
    }
    notifyChanged();
    return listConversations();
  })();
  return initialization;
}

function writeConversations(
  conversations: Conversation[],
  changed?: Conversation,
): void {
  conversationCache = conversations;
  localStorage.setItem(CONVERSATIONS_KEY, JSON.stringify(conversations));
  if (changed) {
    void invoke("save_native_conversation", { conversation: changed }).catch(
      () => undefined,
    );
  }
  notifyChanged();
}

export function createConversation(): Conversation {
  const now = new Date().toISOString();
  const conversation: Conversation = {
    id: crypto.randomUUID(),
    title: "New conversation",
    messages: [],
    createdAt: now,
    updatedAt: now,
  };
  writeConversations([conversation, ...listConversations()], conversation);
  return conversation;
}

export function ensureConversation(): Conversation {
  return listConversations()[0] ?? createConversation();
}

export function getConversation(id: string): Conversation | undefined {
  return listConversations().find((conversation) => conversation.id === id);
}

export function saveConversation(conversation: Conversation): void {
  const saved = { ...conversation, updatedAt: new Date().toISOString() };
  writeConversations([
    saved,
    ...listConversations().filter(
      (candidate) => candidate.id !== conversation.id,
    ),
  ], saved);
}

export function renameConversation(id: string, title: string): void {
  const conversation = getConversation(id);
  const cleanTitle = title.trim().slice(0, 80);
  if (!conversation || !cleanTitle) return;
  saveConversation({ ...conversation, title: cleanTitle });
}

export function deleteConversation(id: string): void {
  writeConversations(
    listConversations().filter((conversation) => conversation.id !== id),
  );
  void invoke("delete_native_conversation", { id }).catch(() => undefined);
}

export async function attachmentsFromFiles(
  files: FileList | File[],
): Promise<ConversationAttachment[]> {
  const result: ConversationAttachment[] = [];
  for (const file of Array.from(files).slice(0, 10)) {
    const extension = file.name.split(".").pop()?.toLowerCase() ?? "";
    const textFile =
      file.type.startsWith("text/") ||
      [
        "md", "json", "csv", "tsv", "js", "jsx", "ts", "tsx", "py", "rs",
        "html", "css", "yaml", "yml", "toml", "sql", "sh",
      ].includes(extension);
    result.push({
      id: crypto.randomUUID(),
      name: file.name,
      mimeType: file.type || "application/octet-stream",
      size: file.size,
      text:
        textFile && file.size <= 1_000_000
          ? (await file.text()).slice(0, 200_000)
          : undefined,
    });
  }
  return result;
}

function approximateTokens(value: string): number {
  return Math.ceil(value.length / 4);
}

function summarizeMessages(messages: ConversationMessage[]): string {
  return messages
    .map((message) => {
      const clean = message.content.replace(/\s+/g, " ").trim();
      return `${message.role === "user" ? "User" : "Assistant"}: ${clean.slice(0, 240)}`;
    })
    .join("\n")
    .slice(0, 8_000);
}

export function buildConversationContext(
  conversation: Conversation,
  tokenBudget = DEFAULT_CONTEXT_TOKENS,
): { messages: Array<{ role: "user" | "assistant"; content: string }>; summary?: string } {
  const expanded = conversation.messages.map((message) => {
    const attachmentText = (message.attachments ?? [])
      .map((attachment) =>
        attachment.text
          ? `\n\n[Attached file: ${attachment.name}]\n${attachment.text}`
          : `\n\n[Attached file: ${attachment.name}; binary content unavailable]`,
      )
      .join("");
    return { role: message.role, content: message.content + attachmentText };
  });
  if (
    expanded.reduce(
      (total, message) => total + approximateTokens(message.content),
      0,
    ) <= tokenBudget
  ) {
    return { messages: expanded, summary: conversation.summary };
  }

  const recent: typeof expanded = [];
  let used = 0;
  for (const message of [...expanded].reverse()) {
    const cost = approximateTokens(message.content);
    if (recent.length && used + cost > tokenBudget * 0.7) break;
    recent.unshift(message);
    used += cost;
  }
  const olderCount = expanded.length - recent.length;
  const summary = summarizeMessages(
    conversation.messages.slice(0, Math.max(0, olderCount)),
  );
  return {
    messages: summary
      ? [
          {
            role: "user",
            content: `[Summary of earlier conversation]\n${summary}`,
          },
          ...recent,
        ]
      : recent,
    summary,
  };
}
