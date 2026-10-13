const CREATIVE_REQUEST =
  /(?:写(?:一首|个|篇)|创作|改写|润色|翻译|角色扮演|脑暴|想象|虚构|诗|故事|文案|write (?:a|an)|create|rewrite|translate|roleplay|brainstorm|imagine|fiction|poem|story)/iu;

const EXECUTION_REQUEST =
  /(?:删除|安装|下载|运行|启动|停止|关闭|移除|切换|选择|部署|更新|升级|delete|install|download|run|start|stop|remove|switch|select|deploy|update|upgrade)/iu;

/*
 * Owner decision, 2026-10-14: only an explicit search/verification request goes
 * online. A product question ("DS925+ 是哪个品牌的？") is answered by the
 * selected model, with its model shown, and a plain statement is background.
 * Words such as 产品, 设备, 目前 or 配置 occur in both, so a fact topic only
 * marks a question when the turn is actually phrased as one.
 */
const SEARCH_REQUEST =
  /(?:联网搜索|网上搜索|搜索|搜一下|搜搜|查一下|查查|查找|查询|查证|核实|验证|上网查|search|look up|browse|verify|fact[ -]?check)/iu;

const QUESTION =
  /(?:[？?]|吗|什么|哪个|哪款|哪家|哪里|哪儿|哪些|谁|多少|几(?:个|款|代|年|盘|核)|怎么|如何|是否|有没有|是不是|能不能|可不可以|支不支持|^\s*(?:what|which|who|whose|how|does|do|is|are|can|when|where)\b)/iu;

const FACT_TOPIC =
  /(?:是什么|哪个品牌|什么品牌|谁生产|谁制造|谁家|厂商|厂家|品牌|型号|规格|参数|配置|属于|产品|设备|最新|价格|多少钱|版本|发布|上市|停产|支持|兼容|盘位|内存|处理器|接口|功耗|manufacturer|makes|made|brand|model|spec(?:ification)?s?|product|device|latest|price|cost|version|release|support|compatible)/iu;

function isPublicFactRequest(text: string): boolean {
  return SEARCH_REQUEST.test(text);
}

/** A question about a mentioned product; it stays with the model, not search. */
export function isProductFactQuestion(request: string): boolean {
  return (
    Boolean(publicProductMentionSubject(request)) &&
    !isPublicFactRequest(request) &&
    QUESTION.test(request) &&
    FACT_TOPIC.test(request)
  );
}

function modelTokens(text: string): string[] {
  const tokens = text.match(/[a-z0-9][a-z0-9+._/-]{1,31}/giu) ?? [];
  const modelIndexes = tokens
    .map((token, index) => ({ token, index }))
    .filter(({ token }) => /[a-z]/iu.test(token) && /\d/u.test(token))
    .map(({ index }) => index);
  if (!modelIndexes.length) return [];

  const selected = new Set<number>();
  for (const index of modelIndexes) {
    selected.add(index);
    for (const preceding of [index - 2, index - 1]) {
      if (
        preceding >= 0 &&
        (/^[A-Z][A-Za-z]{1,11}$/u.test(tokens[preceding]) ||
          /^(?:nas|gpu|cpu|ssd|hdd)$/iu.test(tokens[preceding]))
      ) {
        selected.add(preceding);
      }
    }
  }
  return [...selected]
    .sort((a, b) => a - b)
    .map((index) => tokens[index].toLowerCase());
}

/**
 * Returns only a compact public product identifier, never the full chat text.
 * Undefined means the turn remains an ordinary local-first ASK.
 */
export function publicProductMentionSubject(request: string): string | undefined {
  const text = request.trim();
  if (!text || CREATIVE_REQUEST.test(text) || EXECUTION_REQUEST.test(text)) {
    return undefined;
  }

  const tokens = modelTokens(text);
  if (!tokens.length) return undefined;

  return tokens.slice(0, 4).join(" ");
}

export function publicProductFactSubject(request: string): string | undefined {
  if (!isPublicFactRequest(request)) return undefined;
  return publicProductMentionSubject(request);
}

export function publicProductRoutingIntent(
  request: string,
): { taskType: "ASK" | "DO"; subject?: string } | undefined {
  const subject = publicProductMentionSubject(request);
  if (!subject) return undefined;

  return isPublicFactRequest(request)
    ? { taskType: "DO", subject }
    : { taskType: "ASK" };
}

/*
 * Owner decision, 2026-10-14: the model answers a product question first; when
 * it is not confident it says so with this marker, and AI-OS searches instead.
 */
export const PRODUCT_UNSURE_MARKER = "NEED_SEARCH";

export function buildProductAnswerPrompt(question: string): string {
  return [
    "You are AI-OS. Answer the user's product question directly in at most three sentences.",
    `Only answer if you are certain of the facts. If you are not certain of the manufacturer or of any fact the question asks for, reply with exactly ${PRODUCT_UNSURE_MARKER} and nothing else.`,
    "Never guess and never invent specifications.",
    "Answer in Simplified Chinese unless the question is written in another language.",
    "",
    `Question: ${question}`,
  ].join("\n");
}

export function isUncertainProductAnswer(text: string): boolean {
  const answer = text.trim();
  return !answer || answer.toUpperCase().includes(PRODUCT_UNSURE_MARKER);
}

export function buildPublicProductTaskPrompt(subject: string): string {
  return `请使用 browser.search 核实 ${subject} 的品牌和官方参数。优先厂商官网；用简洁中文回答，并附来源 URL。`;
}
