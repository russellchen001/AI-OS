import type { CouncilAnnualCostEstimate, CouncilRecommendation } from "../types/councilAssembly";

// Checks only explicit, mechanically verifiable requirements. This is not a factual-quality judge.
const numbers: Record<string, number> = { 一: 1, 二: 2, 两: 2, 三: 3, 四: 4, 五: 5, 六: 6, 七: 7, 八: 8, 九: 9, 十: 10 };
function count(text: string | undefined): number | undefined {
  if (text === undefined) return undefined;
  const tens = text.match(/^([一二两三四五六七八九]?)十([一二两三四五六七八九]?)$/);
  const value = numbers[text] ?? (tens ? (numbers[tens[1]] ?? 1) * 10 + (numbers[tens[2]] ?? 0) : Number(text));
  return Number.isFinite(value) ? value : undefined;
}

const REQUESTED_SECTIONS: Array<{ requested: RegExp; evidence: RegExp; label: string }> = [
    {
      requested: /3\s*[-‑–—]\s*2\s*[-‑–—]\s*1.{0,12}(?:对应|原则)|(?:对应|原则).{0,12}3\s*[-‑–—]\s*2\s*[-‑–—]\s*1/i,
      evidence: /3\s*[-‑–—]\s*2\s*[-‑–—]\s*1.{0,80}(?:副本|介质|异地|离线|copy|media|offsite)|(?:副本|介质|异地|离线|copy|media|offsite).{0,80}3\s*[-‑–—]\s*2\s*[-‑–—]\s*1/i,
      label: "a concrete 3-2-1 mapping",
    },
    {
      requested: /(?:自动化(?:方式)?|自动备份|automation)/i,
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

export function formatCouncilAnswer(report: {
  summary: string; recommendedPlan: string[]; rationale: string[]; disagreements: string[];
  risks: string[]; assumptions: string[]; uncertainty: string[];
  annualCostEstimate?: CouncilAnnualCostEstimate;
  simulationReport?: CouncilRecommendation["simulationReport"];
}): string {
  const section = (title: string, items: string[]) => items.length ? `\n\n## ${title}\n\n${items.map(item => `- ${item}`).join("\n")}` : "";
  return [report.summary,
    section("Recommended plan", report.recommendedPlan), section("Rationale", report.rationale),
    section("Disagreements", report.disagreements), section("Risks", report.risks),
    section("Assumptions", report.assumptions), section("Uncertainty", report.uncertainty),
    report.annualCostEstimate ? `\n\n${annualCostEstimateText(report.annualCostEstimate)}` : "",
    report.simulationReport ? [
      section("Simulation evidence", report.simulationReport.evidence),
      section("Likely responses", report.simulationReport.likelyResponses),
      section("Alternative scenarios", report.simulationReport.alternativeScenarios),
      section("Trigger conditions", report.simulationReport.triggerConditions),
      section("Counterarguments", report.simulationReport.counterarguments),
      section("Forecast confidence", report.simulationReport.confidence),
      section("Evidence that would change the forecast", report.simulationReport.evidenceThatWouldChangeForecast),
      section("Recommended response", report.simulationReport.recommendedResponse),
    ].join("") : "",
  ].join("");
}

export function councilResponseSchema(objective: string): Record<string, unknown> | undefined {
  const limits = answerRequirements(objective);
  if (!limits.maxChars || limits.maxChars > 2000 || requiresAnnualCostEstimate(objective)) return undefined;
  const required = requestedDeliverables(objective);
  const needEvidence = deliverablesNeedingEvidence(objective);
  const counts: Record<string, number> = {
    recommendedPlan: limits.plans ?? limits.steps ?? (required.length ? required.length : 0), rationale: limits.reasons ?? 0,
    disagreements: required.some(item => /分歧|disagreement/i.test(item)) ? 1 : 0,
    risks: required.some(item => /风险|risk/i.test(item)) ? 1 : 0,
    assumptions: required.some(item => /假设|assumption/i.test(item)) ? 1 : 0,
    uncertainty: required.some(item => /不确定|uncertainty/i.test(item)) ? 1 : 0,
  };
  const units = 1 + Object.values(counts).reduce((sum, count) => sum + count, 0);
  if (units > limits.maxChars) return undefined;
  const blank = Object.fromEntries(Object.entries(counts).map(([key, count]) => [key, Array(count).fill("")])) as Pick<CouncilRecommendation, "recommendedPlan" | "rationale" | "disagreements" | "risks" | "assumptions" | "uncertainty">;
  const overheadText = formatCouncilAnswer({ ...blank, summary: "" });
  const overhead = limits.hanOnly ? overheadText.match(/\p{Script=Han}/gu)?.length ?? 0 : [...overheadText].length;
  const budget = Math.floor((limits.maxChars - overhead) / units);
  if (budget < 1) return undefined;
  // The budget is guidance, not a grammar cut: a maxLength makes the local
  // engine stop mid-sentence ("…如果不计划删除或移"). Total length is checked
  // after generation and corrected once with the measured count.
  const guide = `One complete sentence of about ${budget} ${limits.hanOnly ? "Han characters" : "characters"}.`;
  const properties: Record<string, unknown> = { summary: { type: "string", minLength: 1, description: guide } };
  for (const [key, count] of Object.entries(counts)) properties[key] = { type: "array", minItems: count, maxItems: count, items: { type: "string", minLength: 1, description: guide } };
  if (needEvidence.length) properties.requirementCoverage = { type: "array", minItems: needEvidence.length, maxItems: needEvidence.length, items: {
    type: "object", properties: { requirement: { type: "string", enum: needEvidence }, evidence: { type: "string", minLength: 1 } },
    required: ["requirement", "evidence"], additionalProperties: false,
  } };
  return { type: "object", properties, required: Object.keys(properties), additionalProperties: false };
}

export function requestedDeliverables(objective: string): string[] {
  const list = objective.match(/(?:最终(?:建议|答案|回答|报告|方案|结果|输出)[^：:。！？]{0,80}?(?:包括|包含)|(?:需要|必须)(?:包括|包含)|(?:must|should|needs? to) include|include the following)\s*[：:]\s*([^。！？]+)/i)?.[1];
  if (!list) return [];
  const items: string[] = [];
  const numbered = /(?:^|\n)\s*(?:\d+[.)]|[-*•])\s/.test(list);
  let depth = 0, item = "";
  for (let index = 0; index < list.length; index++) {
    const character = list[index];
    if ("(（[【".includes(character)) depth++;
    if (")）]】".includes(character)) depth = Math.max(0, depth - 1);
    if (!depth && !numbered && character === "." && /[a-z]/i.test(list[index - 1] ?? "") && (!list[index + 1] || /\s/.test(list[index + 1]))) break;
    if (!depth && "、,，;；\n".includes(character)) {
      if (item.trim()) items.push(item.trim());
      item = "";
    } else item += character;
  }
  if (item.trim()) items.push(item.trim().replace(/[.!?]$/, ""));
  return [...new Set(items.map(value => value.replace(/^(?:\d+[.)、]|[-*•])\s*/, "").replace(/^and\s+/i, "").replace(/[.!?]$/, "").trim()).filter(Boolean))];
}

/**
 * Requested deliverables the validator cannot check on its own. Labels such as
 * "结论", "2条理由", "1条备份建议", 3-2-1 mapping, automation, quarterly drill,
 * risks/trade-offs and annual cost are verified mechanically from the report,
 * which is stronger than a model quoting itself; asking small local models to
 * quote anyway rejected compliant answers for imprecise self-quotes.
 */
export function deliverablesNeedingEvidence(objective: string): string[] {
  const limits = answerRequirements(objective);
  return requestedDeliverables(objective).filter((label) => !(
    /^(?:最终)?(?:结论|答案|建议结论)$|^(?:conclusion|summary|answer)$/i.test(label) ||
    (limits.reasons !== undefined && /理由|reasons?/i.test(label)) ||
    ((limits.plans !== undefined || limits.steps !== undefined) && /建议|步骤|plan|steps?/i.test(label)) ||
    REQUESTED_SECTIONS.some((rule) => rule.requested.test(label)) ||
    (requiresAnnualCostEstimate(objective) && /费用|成本|cost/i.test(label))
  ));
}

function validateRequirementCoverage(objective: string, coverage: Array<{ requirement: string; evidence: string }> | undefined, text: string): void {
  const required = deliverablesNeedingEvidence(objective);
  if (!required.length) return;
  const key = (value: string) => value.normalize("NFKC").replace(/\s+/g, "").toLowerCase();
  // Entries for mechanically checked labels are allowed but not relied on.
  const checked = new Set(requestedDeliverables(objective).filter((label) => !required.includes(label)).map(key));
  if (Array.isArray(coverage)) coverage = coverage.filter((entry) => !(entry && typeof entry.requirement === "string" && checked.has(key(entry.requirement))));
  if (!Array.isArray(coverage) || coverage.length !== required.length ||
      required.some(requirement => coverage.filter(entry => entry && typeof entry.requirement === "string" && key(entry.requirement) === key(requirement)).length !== 1) ||
      coverage.some(entry => !entry || typeof entry.evidence !== "string" || !entry.evidence.trim() || !text.includes(entry.evidence))) {
    throw new Error(`Council requirementCoverage must link every explicitly listed deliverable to a verbatim excerpt of the actual report, exactly once. Required labels: ${JSON.stringify(required)}. Evidence must come from the seven report text fields or rendered annual cost, not an invented completion claim. This traceability check does not establish factual quality.`);
  }
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
  const requirements = REQUESTED_SECTIONS;

  const missing = requirements.filter(requirement => requirement.requested.test(objective) && !requirement.evidence.test(text));
  if (missing.length) throw new Error(`Council answer is missing ${missing.map(requirement => requirement.label).join(", ")} explicitly requested by the user.`);
}

export function validateCouncilAnswer(objective: string, report: {
  summary: string; recommendedPlan: string[]; rationale: string[]; disagreements: string[];
  risks: string[]; assumptions: string[]; uncertainty: string[];
  annualCostEstimate?: CouncilAnnualCostEstimate;
  requirementCoverage?: Array<{ requirement: string; evidence: string }>;
}): void {
  const violations: string[] = [];
  const check = (action: () => void) => {
    try { action(); } catch (error) { violations.push(error instanceof Error ? error.message : String(error)); }
  };
  let costValid = true;
  if (requiresAnnualCostEstimate(objective) || report.annualCostEstimate) {
    check(() => { try { validateStructuredCost(report.annualCostEstimate); } catch (error) { costValid = false; throw error; } });
  }
  const yuanBudget = objective.match(/(?:预算|budget)[^。；;\n]{0,24}?(\d+(?:\.\d+)?)\s*(?:元|人民币|CNY|RMB)/i)?.[1];
  if (costValid && yuanBudget && report.annualCostEstimate &&
      (!/^(?:CNY|RMB|人民币|元)$/i.test(report.annualCostEstimate.currency.trim()) || report.annualCostEstimate.total > Number(yuanBudget))) {
    violations.push(`Council annual expense estimate must use the requested CNY currency and fit the ${yuanBudget} CNY budget; otherwise state a feasible reduced scope instead of presenting an over-budget plan.`);
  }
  const limits = answerRequirements(objective);
  const readableText = formatCouncilAnswer({ ...report, annualCostEstimate: costValid ? report.annualCostEstimate : undefined });
  const text = readableText;
  const length = limits.hanOnly ? text.match(/\p{Script=Han}/gu)?.length ?? 0 : [...text].length;
  if (limits.maxChars && length > limits.maxChars) {
    violations.push(`Council answer exceeds ${limits.maxChars} ${limits.hanOnly ? "Han characters" : "characters"} (it has ${length}).`);
  }
  if (limits.reasons && report.rationale.length !== limits.reasons) violations.push(`Council answer requires exactly ${limits.reasons} reasons; received ${report.rationale.length}.`);
  if (limits.plans && report.recommendedPlan.length !== limits.plans) violations.push(`Council answer requires exactly ${limits.plans} backup suggestion(s); received ${report.recommendedPlan.length}.`);
  if (!limits.plans && limits.steps && report.recommendedPlan.length !== limits.steps) violations.push(`Council answer requires exactly ${limits.steps} plan step(s); received ${report.recommendedPlan.length}.`);
  if (/不删除|不格式化|不修改任何文件|read.only/i.test(objective) &&
      report.recommendedPlan.some((item) => [...item.matchAll(/移动|删除|抹盘|格式化|\b(?:move|delete|erase|format)\b/gi)].some((match) => {
        const clause = item.slice(0, match.index).split(/[，。；,.;]|但|而|却|\bbut\b/i).pop() ?? "";
        return !/不|勿|禁止|无需|避免|不要|不得|never|avoid|without|do not|don't/i.test(clause);
      }))) {
    violations.push("Council backup plan changes originals despite the read-only/non-destructive constraint.");
  }
  const prohibited = objective.match(/(?:不要|禁止|不得)推荐([^。！？]+)/)?.[1]?.split(/[、，,]|或(?:任何)?/).map(term => term.trim()).filter(Boolean) ?? [];
  for (const term of prohibited) {
    const forbidden = /云服务|云端|云存储|cloud/i.test(term) ? /云(?:端|服务|存储|盘)|icloud|onedrive|dropbox|google drive|cloud/i : undefined;
    if (report.recommendedPlan.some(item => {
      const index = forbidden ? item.search(forbidden) : item.toLowerCase().indexOf(term.toLowerCase());
      if (index < 0) return false;
      const clause = item.slice(0, index).split(/[，。；,.;]|但|而|却|\bbut\b/i).pop() ?? "";
      return !/不|勿|禁止|无需|避免|不要|不得|never|avoid|without|do not|don't/i.test(clause);
    })) violations.push(`Council recommendation includes a user-prohibited option: ${term}.`);
  }
  if (!report.annualCostEstimate) check(() => validateCostEstimate(objective, readableText));
  check(() => validateRequestedSections(objective, readableText));
  check(() => validateRequirementCoverage(objective, report.requirementCoverage, readableText));
  if (violations.length) throw new Error(violations.join(" "));
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
