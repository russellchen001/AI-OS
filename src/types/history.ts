export type HistoryProviderId =
  | "chatgpt"
  | "grok"
  | "gemini"
  | "claude"
  | "deepseek"
  | "ollama";

export type HistoryCategory =
  | "coding"
  | "writing"
  | "math"
  | "general";

export type ConversationRecord = {
  id: string;
  createdAt: number;
  updatedAt: number;
  lastOpenedAt: number;

  mode: "compare" | "router";
  title: string;
  prompt: string;

  routedProviderId?: HistoryProviderId;

  category: HistoryCategory;
  favorite: boolean;
  pinned: boolean;
  tags: string[];

  responses: Partial<
    Record<HistoryProviderId, string>
  >;
};
