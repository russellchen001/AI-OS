import type { MemoryEntry } from "./memory";

export type ResponseLanguage = "Chinese" | "English";
export type ResponseDetail = "concise" | "detailed";
export type CurrencyPreference = "AUD" | "USD" | "CNY";

export type BudgetPolicy = {
  amount: number;
  currency?: CurrencyPreference;
};

export type MemoryPolicy = {
  language?: ResponseLanguage;
  responseDetail?: ResponseDetail;
  currency?: CurrencyPreference;
  budget?: BudgetPolicy;
};

export type ResolvedMemoryPolicy = {
  longTermPolicy: MemoryPolicy;
  currentOverride: MemoryPolicy;
  resolvedPolicy: MemoryPolicy;
};

const languagePatterns: Record<ResponseLanguage, RegExp[]> = {
  Chinese: [
    /(?:喜欢|偏好|希望|想要|习惯|默认|请|要).{0,12}(?:中文|汉语).{0,12}(?:回答|回复|作答|响应)?/i,
    /\b(?:i\s+)?(?:prefer|want|like|would like)\b.{0,24}\b(?:answers?|responses?|replies?)?\s*(?:in\s+)?chinese\b/i,
    /\b(?:default to|always|please)\s+(?:answer|respond|reply)\s+in\s+chinese\b/i,
  ],
  English: [
    /(?:喜欢|偏好|希望|想要|习惯|默认|请|要).{0,12}(?:英文|英语).{0,12}(?:回答|回复|作答|响应)?/i,
    /\b(?:i\s+)?(?:prefer|want|like|would like)\b.{0,24}\b(?:answers?|responses?|replies?)?\s*(?:in\s+)?english\b/i,
    /\b(?:default to|always|please)\s+(?:answer|respond|reply)\s+in\s+english\b/i,
  ],
};

const currencyNames: Array<[CurrencyPreference, RegExp]> = [
  ["AUD", /\bAUD\b|澳币|澳元|澳大利亚元|Australian dollars?/i],
  ["USD", /\bUSD\b|美元|美金|US dollars?/i],
  ["CNY", /\bCNY\b|人民币|元人民币|Chinese yuan/i],
];

function matchingLanguage(content: string): ResponseLanguage | undefined {
  const chinese = languagePatterns.Chinese.some((pattern) => pattern.test(content));
  const english = languagePatterns.English.some((pattern) => pattern.test(content));
  return chinese === english ? undefined : chinese ? "Chinese" : "English";
}

function currentLanguage(content: string): ResponseLanguage | undefined {
  const english =
    /(?:这次|本次|当前|这条)?\s*(?:请)?\s*(?:用|以)\s*英文\s*(?:回答|回复|作答)/i.test(content) ||
    /\b(?:answer|respond|reply)(?:\s+to\s+(?:this|the current)\s+(?:message|request))?\s+in\s+english\b/i.test(content);
  const chinese =
    /(?:这次|本次|当前|这条)?\s*(?:请)?\s*(?:用|以)\s*(?:中文|汉语)\s*(?:回答|回复|作答)/i.test(content) ||
    /\b(?:answer|respond|reply)(?:\s+to\s+(?:this|the current)\s+(?:message|request))?\s+in\s+chinese\b/i.test(content);
  return english === chinese ? undefined : english ? "English" : "Chinese";
}

function memoryDetail(content: string): ResponseDetail | undefined {
  const concise =
    /(?:回答|回复).{0,6}(?:尽量|默认)?.{0,4}(?:简洁|简短)|(?:喜欢|默认).{0,8}(?:简短|简洁).{0,4}(?:回答|回复)?/i.test(content) ||
    /\b(?:i prefer concise answers|keep responses concise by default)\b/i.test(content);
  const detailed =
    /(?:回答|解释).{0,6}(?:详细|展开)|(?:喜欢|默认).{0,8}(?:详细解释|展开讲)/i.test(content) ||
    /\b(?:i prefer detailed answers|give detailed explanations by default)\b/i.test(content);
  return concise === detailed ? undefined : concise ? "concise" : "detailed";
}

function currentDetail(content: string): ResponseDetail | undefined {
  const concise =
    /(?:这次|本次).{0,8}(?:简单说|简短回答|回答简洁)|\b(?:just give me a concise answer|keep this answer brief)\b/i.test(content);
  const detailed =
    /(?:这次|本次).{0,8}(?:展开讲|详细说|讲详细一点)|\b(?:explain this in detail|give me a detailed answer)\b/i.test(content);
  return concise === detailed ? undefined : concise ? "concise" : "detailed";
}

function findCurrency(content: string): CurrencyPreference | undefined {
  return currencyNames.find(([, pattern]) => pattern.test(content))?.[0];
}

function memoryCurrency(content: string): CurrencyPreference | undefined {
  const hasPreference =
    /(?:金额)?默认(?:使用|用)|(?:我)?习惯用/i.test(content) ||
    /\b(?:use|prefer)\s+(?:AUD|USD|CNY|Australian dollars?|US dollars?|Chinese yuan)\s+by default\b/i.test(content);
  return hasPreference ? findCurrency(content) : undefined;
}

function currentCurrency(content: string): CurrencyPreference | undefined {
  const hasOverride =
    /(?:这次|本次).{0,10}(?:金额)?.{0,6}(?:使用|用)/i.test(content) ||
    /\bfor this (?:answer|request).{0,12}\buse\b/i.test(content);
  return hasOverride ? findCurrency(content) : undefined;
}

function parseBudget(content: string): BudgetPolicy | undefined {
  const match =
    content.match(/预算(?:一般控制在|可以到|是|为)?\s*(\d+(?:\.\d+)?)/i) ??
    content.match(/\bbudget(?:\s+is|\s+of)?\s*(\d+(?:\.\d+)?)/i);
  if (!match) return undefined;
  return { amount: Number(match[1]), currency: findCurrency(content) };
}

function isCurrentBudget(content: string): boolean {
  return (
    /(?:这次|本次).{0,12}预算|预算.{0,12}(?:这次|本次)/i.test(content) ||
    /\bfor this (?:request|answer).{0,20}\bbudget\b/i.test(content)
  );
}

function orderedUserMemories(memories: MemoryEntry[]): MemoryEntry[] {
  return memories
    .filter((entry) => entry.type === "user" && entry.content.trim())
    .map((entry, index) => ({ entry, index }))
    .sort((left, right) => {
      const timeDifference =
        Date.parse(left.entry.updatedAt) - Date.parse(right.entry.updatedAt);
      return Number.isNaN(timeDifference) || timeDifference === 0
        ? left.index - right.index
        : timeDifference;
    })
    .map(({ entry }) => entry);
}

function longTermPolicy(memories: MemoryEntry[]): MemoryPolicy {
  const ordered = orderedUserMemories(memories);
  const languages = new Set(
    ordered.map((entry) => matchingLanguage(entry.content)).filter(Boolean),
  );
  const policy: MemoryPolicy = {
    language:
      languages.size === 1
        ? (languages.values().next().value as ResponseLanguage)
        : undefined,
  };
  for (const entry of ordered) {
    policy.responseDetail = memoryDetail(entry.content) ?? policy.responseDetail;
    policy.currency = memoryCurrency(entry.content) ?? policy.currency;
    policy.budget = parseBudget(entry.content) ?? policy.budget;
  }
  return policy;
}

function requestOverride(content: string): MemoryPolicy {
  const budget = isCurrentBudget(content) ? parseBudget(content) : undefined;
  return {
    language: currentLanguage(content),
    responseDetail: currentDetail(content),
    currency: currentCurrency(content),
    budget,
  };
}

export function resolveMemoryPolicy(
  memories: MemoryEntry[],
  currentRequest: string,
): ResolvedMemoryPolicy {
  const longTerm = longTermPolicy(memories);
  const current = requestOverride(currentRequest);
  return {
    longTermPolicy: longTerm,
    currentOverride: current,
    resolvedPolicy: {
      language: current.language ?? longTerm.language,
      responseDetail: current.responseDetail ?? longTerm.responseDetail,
      currency: current.currency ?? longTerm.currency,
      budget: current.budget ?? longTerm.budget,
    },
  };
}

export function describeMemoryPolicy(policy: MemoryPolicy): string[] {
  const lines: string[] = [];
  if (policy.language) lines.push(`- Language: ${policy.language}`);
  if (policy.responseDetail) lines.push(`- Response detail: ${policy.responseDetail}`);
  if (policy.currency) lines.push(`- Currency: ${policy.currency}`);
  if (policy.budget) {
    const currency = policy.budget.currency ?? policy.currency;
    lines.push(`- Budget: ${policy.budget.amount}${currency ? ` ${currency}` : ""}`);
  }
  return lines;
}

export function applyMemoryPolicyToOutbound<T extends { content: string }>(
  message: T,
  policy: MemoryPolicy,
): T {
  const lines = describeMemoryPolicy(policy);
  return {
    ...message,
    content:
      lines.length === 0
        ? message.content
        : `${message.content}\n\n[Current response policy:\n${lines.join("\n")}\n]`,
  };
}
