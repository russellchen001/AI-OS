import type { CouncilAnnualCostEstimate } from "../types/councilAssembly";

// Checks only explicit, mechanically verifiable requirements. This is not a factual-quality judge.
const numbers: Record<string, number> = { 一: 1, 二: 2, 两: 2, 三: 3, 四: 4, 五: 5, 六: 6, 七: 7, 八: 8, 九: 9, 十: 10 };
function count(text: string | undefined): number | undefined {
  return text === undefined ? undefined : numbers[text] ?? Number(text);
}

export function answerRequirements(objective: string) {
  return {
    maxChars: count(objective.match(/不超过\s*(\d+)\s*(?:个)?(?:汉字|字|字符)/)?.[1]),
    // "汉字" limits Han characters; "字"/"字符" limits every character.
    hanOnly: /不超过\s*\d+\s*(?:个)?汉字/.test(objective),
    reasons: count(objective.match(/([\d一二两三四五六七八九十]+)\s*条理由/)?.[1]),
    plans: count(objective.match(/([\d一二两三四五六七八九十]+)\s*条(?:可逆的?|具体的?)?(?:本地)?备份建议/)?.[1]),
    steps: count(objective.match(/([\d一二两三四五六七八九十]+)\s*步(?:的|骤|计划|清单|方案)/)?.[1]),
  };
}

function humanReadableReport(report: {
  summary: string; recommendedPlan: string[]; rationale: string[]; disagreements: string[];
  risks: string[]; assumptions: string[]; uncertainty: string[];
  annualCostEstimate?: CouncilAnnualCostEstimate;
}): string {
  return [report.summary, ...report.recommendedPlan, ...report.rationale,
    ...report.disagreements, ...report.risks, ...report.assumptions, ...report.uncertainty,
    annualCostEstimateText(report.annualCostEstimate)].join("\n");
}

export function requiresAnnualCostEstimate(objective: string): boolean {
  return /(?:年度|每年|annual|yearly).{0,8}(?:费用|成本|预算|cost|price|budget).{0,8}(?:估算|估计|金额|逐项|estimate|breakdown)/i.test(objective);
}

export function annualCostEstimateText(cost?: CouncilAnnualCostEstimate): string {
  if (!cost) return "";
  return [`年度费用 / Annual cost (${cost.currency}; ${cost.pricing})`, cost.scope,
    ...cost.items.map((item) => `${item.name}: ${item.amount} ${cost.currency}/年 — ${item.basis}`),
    `合计 / Total: ${cost.total} ${cost.currency}/年`].join("\n");
}

function validateStructuredCost(cost: CouncilAnnualCostEstimate | undefined): void {
  const nonempty = (value: unknown): value is string => typeof value === "string" && Boolean(value.trim());
  if (!cost || !nonempty(cost.currency) || !nonempty(cost.scope) ||
      !["assumed", "verified"].includes(cost.pricing) || !Array.isArray(cost.items) || cost.items.length < 2 ||
      !Number.isFinite(cost.total) || cost.total < 0 ||
      cost.items.some((item) => !item || !nonempty(item.name) || !nonempty(item.basis) ||
        !Number.isFinite(item.amount) || item.amount < 0 ||
        /预算分配|支出上限|预备金|预留|budget allocation|spending cap|contingency|reserve/i.test(`${item.name} ${item.basis}`)) ||
      new Set(cost.items.map((item) => item.name.trim().toLowerCase())).size !== cost.items.length ||
      Math.abs(cost.items.reduce((sum, item) => sum + item.amount, 0) - cost.total) > 0.01) {
    throw new Error("Council annualCostEstimate requires currency, scope, pricing (assumed or verified), at least two distinct expense items with numeric annual amounts and bases, and their arithmetic total. Budget allocations and reserves are not expense estimates. State an explicit assumed price when no quote is verified.");
  }
}

export function validateCouncilParticipation(text: string, verifiedModels: string[]): void {
  for (const clause of text.split(/[。；;\n]/)) {
    if (!/未见.{0,20}提交|未参与|没有提交|缺席|did not (?:contribute|participate|submit)/i.test(clause)) continue;
    if (verifiedModels.some((model) => model.length >= 4 && clause.toLowerCase().includes(model.toLowerCase()))) {
      throw new Error("Council report contradicts runtime-confirmed model participation. Do not claim a successful contributor or the current synthesizer was absent; distinguish a failed stage from non-participation.");
    }
  }
}

function validateCostEstimate(objective: string, text: string): void {
  if (!requiresAnnualCostEstimate(objective)) return;

  const amounts = [...text.matchAll(/(?:¥|￥|CNY|RMB|USD|\$)?\s*\d+(?:\.\d+)?\s*(?:元|人民币|美元|\/年|每年|year|annum)/gi)];
  const hasTotal = /(?:合计|总计|总费用|年度费用|年费|每年约|total|annual total|yearly total)/i.test(text);
  const amountLines = text.split("\n").filter((line) => /\d+(?:\.\d+)?\s*(?:元|人民币|美元|\/年|每年|year|annum)/i.test(line));
  const hasBasis = amountLines.some((line) => /(?:按|假设|估算依据|价格|套餐|单价|范围|约|以.{0,12}为准|assum|basis|price|range|approximately|about)/i.test(line));
  const onlyRepeatsBudgetCeiling = amounts.length <= 1 && /(?:预算|上限|不超过|以内|budget|ceiling|up to|under)/i.test(text);
  const onlyBudgetAllocation = /预算分配|预算上限分配|预算分配框架|支出上限分配|budget allocation/i.test(text);

  if (amounts.length < 2 || !hasTotal || !hasBasis || onlyRepeatsBudgetCeiling || onlyBudgetAllocation) {
    throw new Error("Council answer requires an annual cost estimate with component amounts, an explicit total, and an estimate basis; repeating the budget ceiling is not an estimate.");
  }
}

function validateRequestedSections(objective: string, text: string): void {
  const requirements: Array<{ requested: RegExp; evidence: RegExp; label: string }> = [
    {
      requested: /3\s*[-‑–—]\s*2\s*[-‑–—]\s*1.{0,12}(?:对应|原则)|(?:对应|原则).{0,12}3\s*[-‑–—]\s*2\s*[-‑–—]\s*1/i,
      evidence: /3\s*[-‑–—]\s*2\s*[-‑–—]\s*1.{0,80}(?:副本|介质|异地|离线|copy|media|offsite)|(?:副本|介质|异地|离线|copy|media|offsite).{0,80}3\s*[-‑–—]\s*2\s*[-‑–—]\s*1/i,
      label: "a concrete 3-2-1 mapping",
    },
    {
      requested: /(?:自动化方式|自动备份|automation)/i,
      evidence: /(?:自动|定时|计划任务|同步|增量|schedule|automatic|incremental).{0,80}(?:备份|快照|复制|同步|backup|snapshot|copy|sync)/i,
      label: "an automation method",
    },
    {
      requested: /(?:每季度|季度).{0,12}(?:恢复|还原).{0,12}(?:演练|测试|步骤)|quarterly.{0,20}(?:restore|recovery).{0,20}(?:drill|test|steps?)/i,
      evidence: /(?:每季度|季度|每三个月|quarterly).{0,100}(?:恢复|还原|抽样|校验|restore|recovery|sample|verify)/i,
      label: "quarterly restore-drill steps",
    },
    {
      requested: /(?:主要风险.{0,8}(?:取舍|权衡)|risks?.{0,12}trade[- ]?offs?)/i,
      evidence: /(?:风险|故障|丢失|勒索|泄露|中断|risk|failure|loss|ransomware|breach).{0,100}(?:取舍|权衡|代价|但|相比|trade[- ]?off|cost|however|versus)/i,
      label: "risks and trade-offs",
    },
  ];

  for (const requirement of requirements) {
    if (requirement.requested.test(objective) && !requirement.evidence.test(text)) {
      throw new Error(`Council answer is missing ${requirement.label} explicitly requested by the user.`);
    }
  }
}

export function validateCouncilAnswer(objective: string, report: {
  summary: string; recommendedPlan: string[]; rationale: string[]; disagreements: string[];
  risks: string[]; assumptions: string[]; uncertainty: string[];
  annualCostEstimate?: CouncilAnnualCostEstimate;
}): void {
  if (requiresAnnualCostEstimate(objective) || report.annualCostEstimate) validateStructuredCost(report.annualCostEstimate);
  const yuanBudget = objective.match(/(?:预算|budget)[^。；;\n]{0,24}?(\d+(?:\.\d+)?)\s*(?:元|人民币|CNY|RMB)/i)?.[1];
  if (yuanBudget && report.annualCostEstimate &&
      (!/^(?:CNY|RMB|人民币|元)$/i.test(report.annualCostEstimate.currency.trim()) || report.annualCostEstimate.total > Number(yuanBudget))) {
    throw new Error(`Council annual expense estimate must use the requested CNY currency and fit the ${yuanBudget} CNY budget; otherwise state a feasible reduced scope instead of presenting an over-budget plan.`);
  }
  const limits = answerRequirements(objective);
  const readableText = humanReadableReport(report);
  const text = readableText.replaceAll("\n", "");
  const length = limits.hanOnly ? text.match(/\p{Script=Han}/gu)?.length ?? 0 : [...text].length;
  if (limits.maxChars && length > limits.maxChars) {
    throw new Error(`Council answer exceeds ${limits.maxChars} ${limits.hanOnly ? "Han characters" : "characters"} (it has ${length}).`);
  }
  if (limits.reasons && report.rationale.length !== limits.reasons) throw new Error(`Council answer requires exactly ${limits.reasons} reasons.`);
  if (limits.plans && report.recommendedPlan.length !== limits.plans) throw new Error(`Council answer requires exactly ${limits.plans} backup suggestion(s).`);
  if (/不删除|不格式化|不修改任何文件|read.only/i.test(objective) &&
      report.recommendedPlan.some((item) => [...item.matchAll(/移动|删除|抹盘|格式化|\b(?:move|delete|erase|format)\b/gi)].some((match) => {
        const clause = item.slice(0, match.index).split(/[，。；,.;]|但|而|却|\bbut\b/i).pop() ?? "";
        return !/不|勿|禁止|无需|避免|不要|不得|never|avoid|without|do not|don't/i.test(clause);
      }))) {
    throw new Error("Council backup plan changes originals despite the read-only/non-destructive constraint.");
  }
  if (!report.annualCostEstimate) validateCostEstimate(objective, readableText);
  validateRequestedSections(objective, readableText);
}

export function validateArenaAnswer(objective: string, answer: string): void {
  if (!answer.trim()) throw new Error("Arena has no final task answer; evaluation alone is not completion.");
  const { steps, maxChars, hanOnly } = answerRequirements(objective);
  const length = hanOnly ? answer.match(/\p{Script=Han}/gu)?.length ?? 0 : [...answer].length;
  if (maxChars && length > maxChars) throw new Error(`Arena final answer exceeds ${maxChars} ${hanOnly ? "Han characters" : "characters"} (it has ${length}).`);
  if (steps) {
    const items = answer.match(/(?:^|\n)\s*(?:\d+[.、)]|[一二三四五六七八九十]+、|[-*]\s)/g) ?? [];
    if (items.length !== steps) throw new Error(`Arena final answer requires ${steps} explicit steps; received ${items.length}.`);
  }
  validateRequestedSections(objective, answer);
  validateCostEstimate(objective, answer);
}
