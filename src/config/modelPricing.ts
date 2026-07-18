export type ModelPrice = {
  provider: string;
  model: string;
  inputPerMillion: number;
  outputPerMillion: number;
  currency: "USD";
  source: "official" | "custom";
  updatedAt: string;
};

export const MODEL_PRICING:
  ModelPrice[] = [
  {
    provider: "claude",
    model: "claude-sonnet-4-6",
    inputPerMillion: 3,
    outputPerMillion: 15,
    currency: "USD",
    source: "official",
    updatedAt: "2026-07-18",
  },
  {
    provider: "grok",
    model: "grok-4.5",
    inputPerMillion: 2,
    outputPerMillion: 6,
    currency: "USD",
    source: "official",
    updatedAt: "2026-07-18",
  },
  {
    provider: "deepseek",
    model: "deepseek-v4-flash",
    /*
     * Uses the cache-miss input price.
     * Current Analytics events do not distinguish cache hits.
     */
    inputPerMillion: 0.14,
    outputPerMillion: 0.28,
    currency: "USD",
    source: "official",
    updatedAt: "2026-07-18",
  },
  {
    provider: "ollama",
    model: "*",
    inputPerMillion: 0,
    outputPerMillion: 0,
    currency: "USD",
    source: "custom",
    updatedAt: "2026-07-18",
  },

  /*
   * Exact GPT-5.6 tiers are supported here, but the current
   * configured model "gpt-5.6" does not identify Sol/Terra/Luna.
   */
  {
    provider: "chatgpt",
    model: "gpt-5.6-sol",
    inputPerMillion: 5,
    outputPerMillion: 30,
    currency: "USD",
    source: "official",
    updatedAt: "2026-07-18",
  },
  {
    provider: "chatgpt",
    model: "gpt-5.6-terra",
    inputPerMillion: 2.5,
    outputPerMillion: 15,
    currency: "USD",
    source: "official",
    updatedAt: "2026-07-18",
  },
  {
    provider: "chatgpt",
    model: "gpt-5.6-luna",
    inputPerMillion: 1,
    outputPerMillion: 6,
    currency: "USD",
    source: "official",
    updatedAt: "2026-07-18",
  },
];

export const MODEL_PRICE_ALIASES:
  Record<string, string> = {
  /*
   * Compatibility aliases with verified equivalence.
   */
  "anthropic:claude-sonnet-4-6":
    "claude:claude-sonnet-4-6",

  "xai:grok-4.5":
    "grok:grok-4.5",

  /*
   * Do not map "chatgpt:gpt-5.6" automatically:
   * its Sol/Terra/Luna billing tier is ambiguous.
   *
   * Gemini remains unmatched until its exact API pricing
   * entry is confirmed.
   */
};
