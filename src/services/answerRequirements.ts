// Checks only explicit, mechanically verifiable requirements. This is not a factual-quality judge.
const numbers: Record<string, number> = { 一: 1, 二: 2, 两: 2, 三: 3, 四: 4, 五: 5, 六: 6, 七: 7, 八: 8, 九: 9, 十: 10 };
function count(text: string | undefined): number | undefined {
  return text === undefined ? undefined : numbers[text] ?? Number(text);
}

export function answerRequirements(objective: string) {
  return {
    maxChars: count(objective.match(/不超过\s*(\d+)\s*(?:个)?(?:汉字|字|字符)/)?.[1]),
    reasons: count(objective.match(/([\d一二两三四五六七八九十]+)\s*条理由/)?.[1]),
    plans: count(objective.match(/([\d一二两三四五六七八九十]+)\s*条(?:可逆的?|具体的?)?(?:本地)?备份建议/)?.[1]),
    steps: count(objective.match(/([\d一二两三四五六七八九十]+)\s*步(?:的|骤|计划|清单|方案)/)?.[1]),
  };
}

export function validateCouncilAnswer(objective: string, report: {
  summary: string; recommendedPlan: string[]; rationale: string[]; disagreements: string[];
  risks: string[]; assumptions: string[]; uncertainty: string[];
}): void {
  const limits = answerRequirements(objective);
  const text = [report.summary, ...report.recommendedPlan, ...report.rationale,
    ...report.disagreements, ...report.risks, ...report.assumptions, ...report.uncertainty].join("");
  if (limits.maxChars && [...text].length > limits.maxChars) throw new Error(`Council answer exceeds ${limits.maxChars} characters.`);
  if (limits.reasons && report.rationale.length !== limits.reasons) throw new Error(`Council answer requires exactly ${limits.reasons} reasons.`);
  if (limits.plans && report.recommendedPlan.length !== limits.plans) throw new Error(`Council answer requires exactly ${limits.plans} backup suggestion(s).`);
  if (/不删除|不格式化|不修改任何文件|read.only/i.test(objective) &&
      report.recommendedPlan.some((item) => [...item.matchAll(/移动|删除|抹盘|格式化|\b(?:move|delete|erase|format)\b/gi)].some((match) => {
        const clause = item.slice(0, match.index).split(/[，。；,.;]|但|而|却|\bbut\b/i).pop() ?? "";
        return !/不|勿|禁止|无需|避免|不要|不得|never|avoid|without|do not|don't/i.test(clause);
      }))) {
    throw new Error("Council backup plan changes originals despite the read-only/non-destructive constraint.");
  }
}

export function validateArenaAnswer(objective: string, answer: string): void {
  if (!answer.trim()) throw new Error("Arena has no final task answer; evaluation alone is not completion.");
  const { steps } = answerRequirements(objective);
  if (steps) {
    const items = answer.match(/(?:^|\n)\s*(?:\d+[.、)]|[一二三四五六七八九十]+、|[-*]\s)/g) ?? [];
    if (items.length !== steps) throw new Error(`Arena final answer requires ${steps} explicit steps; received ${items.length}.`);
  }
}
