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
  updatedAt: number;
  mode: "compare" | "router";
  title: string;
  prompt: string;
  routedProviderId?: HistoryProviderId;
  favorite: boolean;
  tags: string[];
  responses: Partial<
    Record<HistoryProviderId, string>
  >;
};
