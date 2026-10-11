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
