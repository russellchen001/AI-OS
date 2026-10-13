import type { MemoryEntry } from "./memory";

/*
 * Select factual memories for the CURRENT request.
 * Never modify or delete stored memories.
 */

const generalTerms = new Set([
  "正在", "研究", "之前", "刚才", "什么", "如何",
  "怎么", "可以", "需要", "应该", "我们", "我的",
  "现在", "这个", "那个", "问题", "内容", "方案",
  "建议", "帮助", "文件", "相关",
]);

function keywords(text: string): Set<string> {
  const result = new Set<string>();

  for (const word of text.toLowerCase().match(/[a-z0-9][a-z0-9.-]{2,}/g) ?? []) {
    if (!["the", "and", "for", "with", "what", "how", "about"].includes(word)) {
      result.add(word);
    }
  }

  for (const segment of text.match(/[\u3400-\u9fff]+/gu) ?? []) {
    for (let i = 0; i < segment.length - 1; i++) {
      const term = segment.slice(i, i + 2);
      if (!generalTerms.has(term)) result.add(term);
    }
  }

  return result;
}

export function selectRelevantMemoryFacts(
  memories: MemoryEntry[],
  request: string,
): MemoryEntry[] {
  const text = request.trim();

  // Simple statements must not trigger unrelated factual memories.
  const informationRequest =
    /[？?]/u.test(text) ||
    /(?:怎么|如何|为什么|哪些|推荐|建议|比较|对比|分析|解释|回顾|记得)/u.test(text) ||
    /\b(?:how|why|what|which|recommend|compare|explain)\b/i.test(text);

  if (!informationRequest) return [];

  const requestTerms = keywords(text);
  if (requestTerms.size === 0) return [];

  const asksPriorBudget =
    /(?:之前|以前|上次|过去).{0,20}预算|预算.{0,20}(?:之前|以前|上次)/u.test(text);

  return memories
    .filter((memory) => memory.type === "user" && memory.content.trim())
    .filter((memory) => {
      // Behavioral preferences are handled by MemoryPolicy.
      if (/(?:默认|偏好|喜欢).{0,15}(?:中文|英文|简洁|详细回答)/u.test(memory.content)) {
        return false;
      }

      // Historical budgets are not general-purpose spending limits.
      if (/预算|\bbudget\b/i.test(memory.content) && !asksPriorBudget) {
        return false;
      }

      const memoryTerms = keywords(memory.content);
      return [...requestTerms].some((term) => memoryTerms.has(term));
    })
    .slice(0, 4);
}

/*
 * Historical budgets must not become global response policy.
 * An explicit current-request budget remains visible in the user message.
 */
export function scopedMemoryPolicy<T extends {
  budget?: unknown;
}>(policy: T, currentOverride: { budget?: T["budget"] }): T {
  return {
    ...policy,
    budget: currentOverride.budget,
  };
}
