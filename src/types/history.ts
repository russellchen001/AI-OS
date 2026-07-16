export type HistoryProviderId =
  | "chatgpt"
  | "grok"
  | "gemini"
  | "claude"
  | "deepseek"
  | "ollama";

export type ConversationRecord = {
  id: string;
  createdAt: number;
  mode: "compare" | "router";
  prompt: string;
  routedProviderId?: HistoryProviderId;
  responses: Partial<
    Record<HistoryProviderId, string>
  >;
};
