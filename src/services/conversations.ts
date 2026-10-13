import { invoke } from "@tauri-apps/api/core";
import type { AiCenterInvocationMetadata } from "./aiCenterObservability";

export const CONVERSATIONS_CHANGED_EVENT = "ai-os:conversations-changed";
const CONVERSATIONS_KEY = "ai-os.conversations.v1";
const DEFAULT_CONTEXT_TOKENS = 32_000;
const DEFAULT_OUTPUT_RESERVE_TOKENS = 4_000;
const CONTEXT_SAFETY_MARGIN_TOKENS = 256;
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
  projectId?: string;
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

export async function reloadNativeConversations(): Promise<Conversation[]> {
  const native = (await invoke<unknown[]>("list_native_conversations"))
    .filter(isConversation);

  conversationCache = native;
  localStorage.setItem(CONVERSATIONS_KEY, JSON.stringify(native));
  notifyChanged();
  return listConversations();
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

export function estimateContextTokens(value: string): number {
  let tokens = 0;
  for (const character of value) {
    if (/\s/u.test(character)) tokens += 0.25;
    else if (character.codePointAt(0)! <= 0x7f) tokens += 0.25;
    else tokens += 1;
  }
  return Math.max(1, Math.ceil(tokens));
}

function truncateToTokenBudget(value: string, tokenBudget: number): string {
  if (estimateContextTokens(value) <= tokenBudget) return value;
  const suffix = "\n[Content truncated to fit the selected model context window.]";
  let low = 0;
  let high = value.length;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    if (estimateContextTokens(value.slice(0, middle) + suffix) <= tokenBudget) {
      low = middle;
    } else {
      high = middle - 1;
    }
  }
  return value.slice(0, low) + suffix;
}

function summarizeMessages(
  messages: ConversationMessage[],
  tokenBudget: number,
): string {
  if (messages.length === 0 || tokenBudget <= 0) return "";
  const header = `[Earlier conversation compacted: ${messages.length} messages; ${messages[0].id}..${messages[messages.length - 1].id}]`;
  const protectedLines: string[] = [];
  const ordinaryLines: string[] = [];
  const important = /\b(?:decid(?:e|ed|ing|ion)|must|should|deadline|budget|version|approved?|rejected?|require(?:d|ment)?|do not|never|always)\b|(?:决定|必须|应该|截止|预算|版本|批准|拒绝|要求|不要|永远|始终)|(?:\d[\d,.]*\s*(?:%|USD|AUD|CNY|RMB|元|美元|澳元|天|小时|分钟)?)/iu;
  const lineFor = (message: ConversationMessage) => {
    const clean = message.content.replace(/\s+/g, " ").trim();
    return clean
      ? `[${message.id}] ${message.role === "user" ? "User" : "Assistant"}: ${clean.slice(0, 360)}`
      : "";
  };
  for (const message of messages) {
    if (!important.test(message.content)) continue;
    const line = lineFor(message);
    if (!line) continue;
    const candidate = [header, "Protected decisions, facts, and numbers:", ...protectedLines, line].join("\n");
    if (estimateContextTokens(candidate) > tokenBudget) break;
    protectedLines.push(line);
  }
  for (const message of [...messages].reverse()) {
    if (important.test(message.content)) continue;
    const line = lineFor(message);
    if (!line) continue;
    const candidate = [
      header,
      ...(protectedLines.length ? ["Protected decisions, facts, and numbers:", ...protectedLines] : []),
      "Recent earlier context:",
      line,
      ...ordinaryLines,
    ].join("\n");
    if (estimateContextTokens(candidate) > tokenBudget) break;
    ordinaryLines.unshift(line);
  }
  return truncateToTokenBudget(
    [
      header,
      ...(protectedLines.length ? ["Protected decisions, facts, and numbers:", ...protectedLines] : []),
      ...(ordinaryLines.length ? ["Recent earlier context:", ...ordinaryLines] : []),
    ].join("\n"),
    tokenBudget,
  );
}

function searchTerms(query: string): string[] {
  const normalized = query.toLocaleLowerCase();
  const words = normalized.match(/[\p{L}\p{N}][\p{L}\p{N}._-]*/gu) ?? [];
  const cjk = [...normalized].filter((character) => /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]/u.test(character));
  return [...new Set([...words.filter((word) => word.length > 1), ...cjk])].slice(0, 24);
}

export type ConversationSearchHit = {
  conversationId: string;
  projectId?: string;
  conversationTitle: string;
  messageId: string;
  role: "user" | "assistant";
  excerpt: string;
  createdAt: string;
  score: number;
};

export function searchConversationHistory(
  query: string,
  options: { projectId?: string | null; excludeConversationId?: string; limit?: number } = {},
): ConversationSearchHit[] {
  const terms = searchTerms(query);
  if (terms.length === 0) return [];
  const hits: ConversationSearchHit[] = [];
  for (const conversation of listConversations()) {
    if (
      Object.prototype.hasOwnProperty.call(options, "projectId") &&
      (conversation.projectId ?? null) !== (options.projectId ?? null)
    ) continue;
    if (conversation.id === options.excludeConversationId) continue;
    for (const message of conversation.messages) {
      const haystack = `${conversation.title}\n${message.content}`.toLocaleLowerCase();
      const matched = terms.filter((term) => haystack.includes(term));
      if (matched.length === 0) continue;
      const firstMatch = Math.max(0, haystack.indexOf(matched[0]) - 100);
      hits.push({
        conversationId: conversation.id,
        projectId: conversation.projectId,
        conversationTitle: conversation.title,
        messageId: message.id,
        role: message.role,
        excerpt: message.content.replace(/\s+/g, " ").slice(firstMatch, firstMatch + 420),
        createdAt: message.createdAt,
        score: matched.length * 10 + Math.min(5, matched.reduce((total, term) => total + haystack.split(term).length - 1, 0)),
      });
    }
  }
  return hits
    .sort((left, right) => right.score - left.score || right.createdAt.localeCompare(left.createdAt))
    .slice(0, Math.max(1, Math.min(options.limit ?? 8, 20)));
}

export function buildConversationRetrievalContext(
  query: string,
  options: { projectId?: string | null; excludeConversationId?: string; tokenBudget: number },
): { role: "system"; content: string } | undefined {
  const hits = searchConversationHistory(query, { ...options, limit: 12 });
  if (hits.length === 0) return undefined;
  const lines = [
    "Relevant saved conversation evidence. Treat each bracketed message id as provenance and prefer the current request when it conflicts:",
  ];
  for (const hit of hits) {
    const line = `- [conversation:${hit.conversationId}; message:${hit.messageId}] ${hit.conversationTitle}: ${hit.excerpt}`;
    if (estimateContextTokens([...lines, line].join("\n")) > options.tokenBudget) break;
    lines.push(line);
  }
  return lines.length > 1 ? { role: "system", content: lines.join("\n") } : undefined;
}

export type ConversationContextOptions = {
  contextWindowTokens?: number;
  outputReserveTokens?: number;
  systemTokens?: number;
};

export type ConversationContextProjection = {
  messages: Array<{ role: "system" | "user" | "assistant"; content: string }>;
  summary?: string;
  inputBudgetTokens: number;
  estimatedInputTokens: number;
  retainedMessageIds: string[];
  compactedMessageIds: string[];
};

export function buildConversationContext(
  conversation: Conversation,
  options: ConversationContextOptions = {},
): ConversationContextProjection {
  const contextWindowTokens = Math.max(
    2_048,
    Math.floor(options.contextWindowTokens ?? DEFAULT_CONTEXT_TOKENS),
  );
  const outputReserveTokens = Math.min(
    Math.floor(contextWindowTokens / 2),
    Math.max(1_024, Math.floor(options.outputReserveTokens ?? DEFAULT_OUTPUT_RESERVE_TOKENS)),
  );
  const inputBudgetTokens = Math.max(
    512,
    contextWindowTokens -
      outputReserveTokens -
      Math.max(0, Math.floor(options.systemTokens ?? 0)) -
      CONTEXT_SAFETY_MARGIN_TOKENS,
  );
  const expanded = conversation.messages.map((message) => {
    const attachmentText = (message.attachments ?? [])
      .map((attachment) =>
        attachment.text
          ? `\n\n[Attached file: ${attachment.name}]\n${attachment.text}`
          : `\n\n[Attached file: ${attachment.name}; binary content unavailable]`,
      )
      .join("");
    return { id: message.id, role: message.role, content: message.content + attachmentText };
  });
  const fullHistoryTokens = expanded.reduce(
    (total, message) => total + estimateContextTokens(message.content) + 4,
    0,
  );
  if (fullHistoryTokens <= inputBudgetTokens) {
    return {
      messages: expanded.map(({ role, content }) => ({ role, content })),
      summary: conversation.summary,
      inputBudgetTokens,
      estimatedInputTokens: fullHistoryTokens,
      retainedMessageIds: expanded.map((message) => message.id),
      compactedMessageIds: [],
    };
  }

  const recent: typeof expanded = [];
  let used = 0;
  const summaryBudget = Math.min(2_048, Math.max(256, Math.floor(inputBudgetTokens * 0.15)));
  const recentBudget = Math.max(256, inputBudgetTokens - summaryBudget);
  for (const message of [...expanded].reverse()) {
    const cost = estimateContextTokens(message.content) + 4;
    if (recent.length && used + cost > recentBudget) break;
    recent.unshift(
      recent.length === 0 && cost > recentBudget
        ? { ...message, content: truncateToTokenBudget(message.content, recentBudget - 4) }
        : message,
    );
    used += cost;
  }
  const olderCount = expanded.length - recent.length;
  const summary = summarizeMessages(
    conversation.messages.slice(0, Math.max(0, olderCount)),
    summaryBudget,
  );
  const messages: ConversationContextProjection["messages"] = summary
    ? [{ role: "system", content: summary }, ...recent.map(({ role, content }) => ({ role, content }))]
    : recent.map(({ role, content }) => ({ role, content }));
  return {
    messages,
    summary,
    inputBudgetTokens,
    estimatedInputTokens: messages.reduce(
      (total, message) => total + estimateContextTokens(message.content) + 4,
      0,
    ),
    retainedMessageIds: recent.map((message) => message.id),
    compactedMessageIds: expanded.slice(0, olderCount).map((message) => message.id),
  };
}
