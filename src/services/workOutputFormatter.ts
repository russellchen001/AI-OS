type SearchResult = {
  title?: unknown;
  snippet?: unknown;
  url?: unknown;
};

function cleanText(value: unknown): string {
  if (typeof value !== "string") return "";

  return value
    .replace(/[\u0000-\u001f\u007f]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function safeWebUrl(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;

  try {
    const parsed = new URL(value);

    if (
      !["http:", "https:"].includes(parsed.protocol) ||
      !parsed.hostname ||
      parsed.username ||
      parsed.password
    ) {
      return undefined;
    }

    return parsed.href;
  } catch {
    return undefined;
  }
}

function formatSearchResult(
  result: SearchResult,
  index: number,
): string | undefined {
  const title = cleanText(result.title);
  const snippet = cleanText(result.snippet);
  const url = safeWebUrl(result.url);

  if (!title || !url) return undefined;

  const lines = [
    `**${index}. ${title.replace(/[*_[\]`]/g, "")}**`,
  ];

  if (snippet) {
    lines.push(`摘要：${snippet}`);
  }

  lines.push(`来源：${url}`);

  return lines.join("\n\n");
}

export function formatWorkExecutionOutput(output: unknown): string {
  const fallback =
    "AI-OS 已执行任务，但结果暂时无法转换为可读内容。请查看任务状态或诊断日志。";

  if (!output || typeof output !== "object") {
    return fallback;
  }

  const root = output as Record<string, unknown>;
  const invocation = root.skillInvocation;

  if (!invocation || typeof invocation !== "object") {
    return fallback;
  }

  const skill = invocation as Record<string, unknown>;

  if (skill.capability === "computer.use.execute") {
    const raw = skill.output;
    if (!raw || typeof raw !== "object") return fallback;
    const result = raw as Record<string, unknown>;
    const steps = typeof result.stepsUsed === "number" && Number.isInteger(result.stepsUsed)
      ? result.stepsUsed
      : undefined;
    if (result.status === "completed" && result.stopReason === "completed") {
      return `Computer Use 已完成${steps === undefined ? "" : `（${steps} 步）`}。`;
    }
    if (result.status === "stopped") {
      const reason = cleanText(result.stopReason);
      return `Computer Use 未完成${reason ? `（${reason}）` : ""}。请检查任务范围后重试。`;
    }
    return fallback;
  }

  if (skill.capability !== "browser.search") {
    return fallback;
  }

  const raw = skill.output;

  if (!raw || typeof raw !== "object") {
    return fallback;
  }

  const data = raw as Record<string, unknown>;

  if (!Array.isArray(data.results)) {
    return fallback;
  }

  const formatted = data.results
    .slice(0, 3)
    .map((item, index) => {
      if (!item || typeof item !== "object") {
        return undefined;
      }

      return formatSearchResult(
        item as SearchResult,
        index + 1,
      );
    })
    .filter((item): item is string => Boolean(item));

  if (formatted.length === 0) {
    return "搜索已执行，但没有返回可展示的网页结果。";
  }

  const query = cleanText(data.query);

  return [
    query ? `### 搜索结果：${query}` : "### 搜索结果",
    ...formatted,
    "以上为网页搜索结果，未独立核实文章发布日期。",
  ].join("\n\n");
}

export type SearchEvidence = {
  title: string;
  url: string;
  snippet: string;
};

/** The cleaned browser.search hits, or undefined for any other output. */
export function searchEvidence(output: unknown): SearchEvidence[] | undefined {
  const skill = (output as { skillInvocation?: { capability?: unknown; output?: unknown } } | null)
    ?.skillInvocation;
  if (!skill || skill.capability !== "browser.search") return undefined;
  const results = (skill.output as { results?: unknown } | null)?.results;
  if (!Array.isArray(results)) return undefined;

  const evidence = results
    .map((item) => {
      const hit = (item ?? {}) as SearchResult;
      const title = cleanText(hit.title).replace(/[*_[\]`]/g, "");
      const url = safeWebUrl(hit.url);
      return title && url ? { title, url, snippet: cleanText(hit.snippet) } : undefined;
    })
    .filter((item): item is SearchEvidence => Boolean(item))
    .slice(0, 3);
  return evidence.length ? evidence : undefined;
}

/**
 * The model may only restate what the retrieved snippets say. It receives the
 * user's question and the snippets, nothing else from the conversation.
 */
export function buildGroundedSearchPrompt(
  question: string,
  evidence: SearchEvidence[],
): string {
  return [
    "你是 AI-OS。请仅根据下面的网页搜索证据，用简体中文简洁回答用户的问题（不超过 5 句）。",
    "规则：只使用证据中明确出现的事实；证据不足以回答的部分，直说无法从搜索结果确认，不要猜测或补充记忆中的参数。",
    "优先采用厂商官网的信息。不要输出网址、来源列表或 Markdown 标题，来源由 AI-OS 另行附上。",
    "",
    `用户问题：${question}`,
    "",
    "搜索证据：",
    ...evidence.map(
      (item, index) => `[${index + 1}] ${item.title}\n${item.url}\n${item.snippet || "（无摘要）"}`,
    ),
  ].join("\n");
}

export function formatGroundedSearchAnswer(
  answer: string,
  evidence: SearchEvidence[],
): string {
  const sources = evidence.map((item) => `- [${item.title}](${item.url})`);
  return [answer.trim(), "**来源**", sources.join("\n")].join("\n\n");
}
