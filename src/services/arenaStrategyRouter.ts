import type { AiCenterModelChoice } from "./aiCenter";
import type {
  ArenaModelConstraint,
  ArenaSeatRole,
  ArenaStrategy,
  ArenaStrategyPlan,
} from "../types/arena";

function includesAny(value: string, terms: string[]): boolean {
  return terms.some((term) => value.includes(term));
}

function uniqueStrategies(strategies: ArenaStrategy[]): ArenaStrategy[] {
  return [...new Set(strategies)];
}

function inferStrategies(objective: string): ArenaStrategy[] {
  const lower = objective.toLowerCase();
  const strategies: ArenaStrategy[] = [];

  if (
    includesAny(lower, [
      "compare",
      "comparison",
      "versus",
      " vs ",
      "which is better",
      "哪个好",
      "比较",
      "对比",
      "区别",
      "选择哪个",
    ])
  ) {
    strategies.push("compare");
  }

  if (
    includesAny(lower, [
      "debate",
      "argue",
      "challenge",
      "pros and cons",
      "反驳",
      "辩论",
      "争论",
      "正方",
      "反方",
    ])
  ) {
    strategies.push("debate");
  }

  if (
    includesAny(lower, [
      "collaborate",
      "together",
      "brainstorm",
      "co-design",
      "共同",
      "合作",
      "一起",
      "头脑风暴",
    ])
  ) {
    strategies.push("collaboration");
  }

  if (
    includesAny(lower, [
      "compete",
      "competition",
      "score",
      "rank performance",
      "竞技",
      "竞赛",
      "比赛",
      "评分",
    ])
  ) {
    strategies.push("competition");
  }

  if (
    includesAny(lower, [
      "role play",
      "roleplay",
      "act as",
      "persona",
      "扮演",
      "角色扮演",
      "模拟人物",
    ])
  ) {
    strategies.push("role-play");
  }

  if (
    includesAny(lower, [
      "game",
      "werewolf",
      "mafia",
      "social deduction",
      "狼人杀",
      "游戏",
      "桌游",
    ])
  ) {
    strategies.push("game");
  }

  if (
    includesAny(lower, [
      "simulate",
      "simulation",
      "scenario",
      "what happens if",
      "推演",
      "仿真",
      "模拟",
      "情景",
      "预演",
    ])
  ) {
    strategies.push("simulation");
  }

  if (!strategies.length) {
    strategies.push("collaboration");
  }

  if (
    strategies.includes("compare") &&
    !strategies.includes("debate") &&
    includesAny(lower, [
      "architecture",
      "strategy",
      "decision",
      "trade-off",
      "tradeoff",
      "架构",
      "战略",
      "决策",
      "取舍",
    ])
  ) {
    strategies.push("debate");
  }

  if (
    strategies.includes("simulation") &&
    includesAny(lower, [
      "evaluate",
      "judge",
      "best",
      "result",
      "评估",
      "裁判",
      "结果",
    ])
  ) {
    strategies.push("competition");
  }

  return uniqueStrategies(strategies).slice(0, 4);
}

type ArenaModelMention = {
  label: string;
  start: number;
  end: number;
  choice?: AiCenterModelChoice;
  targetRole?: ArenaSeatRole;
};

function normalizeArenaModelText(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/\s+/g, " ");
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function modelAliases(
  model: AiCenterModelChoice,
): string[] {
  return [
    model.modelId,
    model.label,
    model.providerId,
  ]
    .map((value) => value?.trim())
    .filter(
      (value): value is string =>
        Boolean(value && value.length >= 2),
    )
    .filter(
      (value, index, all) =>
        all.findIndex(
          (candidate) =>
            candidate.toLowerCase() ===
            value.toLowerCase(),
        ) === index,
    )
    .sort((a, b) => b.length - a.length);
}

function inferRoleNearMention(
  objective: string,
  start: number,
  end: number,
): ArenaSeatRole | undefined {
  const before = objective.slice(
    Math.max(0, start - 32),
    start,
  );

  const after = objective.slice(
    end,
    Math.min(objective.length, end + 40),
  );

  const tests: Array<{
    role: ArenaSeatRole;
    before: RegExp;
    after: RegExp;
  }> = [
    {
      role: "judge",
      before:
        /(?:judge|裁判|评委)\s*(?:用|由|是|:|：)?\s*$/i,
      after:
        /^\s*(?:[,，:：\-–—]\s*)?(?:(?:as|is|当|作为|做|担任)\s*)?(?:judge|裁判|评委)/i,
    },
    {
      role: "moderator",
      before:
        /(?:moderator|主持人|主持)\s*(?:用|由|是|:|：)?\s*$/i,
      after:
        /^\s*(?:[,，:：\-–—]\s*)?(?:(?:as|is|当|作为|做|担任)\s*)?(?:moderator|主持人|主持)/i,
    },
    {
      role: "evaluator",
      before:
        /(?:evaluator|评估员|评审员|评审)\s*(?:用|由|是|:|：)?\s*$/i,
      after:
        /^\s*(?:[,，:：\-–—]\s*)?(?:(?:as|is|当|作为|做|担任)\s*)?(?:evaluator|评估员|评审员|评审)/i,
    },
    {
      role: "synthesizer",
      before:
        /(?:synthesizer|总结员|综合员)\s*(?:用|由|是|:|：)?\s*$/i,
      after:
        /^\s*(?:[,，:：\-–—]\s*)?(?:(?:as|is|当|作为|做|担任)\s*)?(?:synthesizer|总结员|综合员)/i,
    },
    {
      role: "observer",
      before:
        /(?:observer|观察员)\s*(?:用|由|是|:|：)?\s*$/i,
      after:
        /^\s*(?:[,，:：\-–—]\s*)?(?:(?:as|is|当|作为|做|担任)\s*)?(?:observer|观察员)/i,
    },
    {
      role: "participant",
      before:
        /(?:participant|参与者|选手|参赛者)\s*(?:用|由|是|:|：)?\s*$/i,
      after:
        /^\s*(?:[,，:：\-–—]\s*)?(?:(?:as|is|当|作为|做|担任)\s*)?(?:participant|参与者|选手|参赛者)/i,
    },
  ];

  for (const test of tests) {
    if (
      test.before.test(before) ||
      test.after.test(after)
    ) {
      return test.role;
    }
  }

  return undefined;
}

function overlap(
  aStart: number,
  aEnd: number,
  bStart: number,
  bEnd: number,
): boolean {
  return aStart < bEnd && aEnd > bStart;
}

function discoverCatalogModelMentions(
  objective: string,
  availableModels: AiCenterModelChoice[],
): ArenaModelMention[] {
  type Candidate = ArenaModelMention & {
    aliasLength: number;
    identity: string;
  };

  const candidates: Candidate[] = [];

  for (const model of availableModels) {
    const identity = [
      model.providerId,
      model.providerInstanceId,
      model.modelId,
    ].join(":");

    for (const alias of modelAliases(model)) {
      const matcher = new RegExp(
        escapeRegExp(alias),
        "gi",
      );

      for (const match of objective.matchAll(matcher)) {
        const start = match.index ?? -1;
        if (start < 0) continue;

        const label = match[0];
        const end = start + label.length;

        candidates.push({
          label,
          start,
          end,
          choice: model,
          targetRole: inferRoleNearMention(
            objective,
            start,
            end,
          ),
          aliasLength: alias.length,
          identity,
        });
      }
    }
  }

  candidates.sort(
    (left, right) =>
      left.start - right.start ||
      right.aliasLength - left.aliasLength,
  );

  const selected: Candidate[] = [];
  const identities = new Set<string>();

  for (const candidate of candidates) {
    if (identities.has(candidate.identity)) {
      continue;
    }

    if (
      selected.some((existing) =>
        overlap(
          candidate.start,
          candidate.end,
          existing.start,
          existing.end,
        ),
      )
    ) {
      continue;
    }

    selected.push(candidate);
    identities.add(candidate.identity);
  }

  return selected.map(
    ({
      aliasLength: _aliasLength,
      identity: _identity,
      ...mention
    }) => mention,
  );
}

function objectiveHasHardSelectionLanguage(
  objective: string,
): boolean {
  return /(?:必须|只用|固定用|指定用|一定要用|must use|only use|use exactly|\bpin(?:ned)?\b)/i.test(
    objective,
  );
}

function objectiveHasPreferenceLanguage(
  objective: string,
): boolean {
  return /(?:最好用|优先用|尽量用|倾向用|prefer|preferably|try to use)/i.test(
    objective,
  );
}

function looksLikeExplicitArenaParticipantSelection(
  objective: string,
  mentions: ArenaModelMention[],
): boolean {
  if (!mentions.length) {
    return false;
  }

  // Explicit role assignment is always deliberate user selection.
  if (
    mentions.some(
      (mention) => Boolean(mention.targetRole),
    )
  ) {
    return true;
  }

  // Natural-language participation syntax:
  // "让 A 和 B 比一下"
  // "A vs B"
  // "use A and B to debate"
  // No vendor/model names are encoded here.
  if (
    /(?:让|叫|用|由)\s*.+(?:比较|对比|比一下|辩论|比赛|竞技|合作|讨论|评审|评估)/i.test(
      objective,
    )
  ) {
    return true;
  }

  if (
    /(?:compare|debate|compete|versus|\bvs\.?\b|collaborate|review)\s+/i.test(
      objective,
    ) ||
    /\s(?:versus|\bvs\.?\b)\s/i.test(objective)
  ) {
    return true;
  }

  // Compact shorthand:
  // "model-A + model-B + model-C"
  //
  // This only activates when those actual spans were independently resolved
  // from the live AI Center catalog. It does NOT contain hardcoded model names.
  if (mentions.length >= 2) {
    for (let index = 0; index < mentions.length - 1; index += 1) {
      const between = objective.slice(
        mentions[index].end,
        mentions[index + 1].start,
      );

      if (
        !/^\s*(?:\+|\/|&|,|，|、|和|与|及|and)\s*$/i.test(
          between,
        )
      ) {
        return false;
      }
    }

    return true;
  }

  return false;
}

function resolvedConstraint(
  mention: ArenaModelMention,
  mode: "prefer" | "pinned",
): ArenaModelConstraint {
  if (!mention.choice) {
    if (mode === "pinned") {
      return {
        mode: "pinned",
        label: mention.label.slice(0, 120),
        targetRole: mention.targetRole,
      };
    }

    return {
      mode: "prefer",
      label: mention.label.slice(0, 120),
      targetRole: mention.targetRole,
    };
  }

  if (mode === "pinned") {
    return {
      mode: "pinned",
      providerId: mention.choice.providerId,
      providerInstanceId:
        mention.choice.providerInstanceId,
      modelId: mention.choice.modelId,
      label: mention.label,
      targetRole: mention.targetRole,
    };
  }

  return {
    mode: "prefer",
    providerId: mention.choice.providerId,
    providerInstanceId:
      mention.choice.providerInstanceId,
    modelId: mention.choice.modelId,
    label: mention.label,
    targetRole: mention.targetRole,
  };
}

function constraintIdentity(
  constraint: ArenaModelConstraint,
): string {
  if (constraint.mode === "auto") {
    return "auto";
  }

  return [
    constraint.mode,
    constraint.providerId ?? "",
    constraint.providerInstanceId ?? "",
    constraint.modelId ?? "",
    normalizeArenaModelText(
      constraint.label ?? "",
    ),
    constraint.targetRole ?? "",
  ].join("|");
}

export function extractUserModelConstraints(
  objective: string,
  availableModels: AiCenterModelChoice[] = [],
): ArenaModelConstraint[] {
  const mentions = discoverCatalogModelMentions(
    objective,
    availableModels,
  );

  if (!mentions.length) {
    return [];
  }

  const hardLanguage =
    objectiveHasHardSelectionLanguage(objective);

  const preferLanguage =
    objectiveHasPreferenceLanguage(objective);

  const explicitParticipants =
    looksLikeExplicitArenaParticipantSelection(
      objective,
      mentions,
    );

  const constraints = mentions.map((mention) => {
    const mode: "prefer" | "pinned" =
      hardLanguage ||
      explicitParticipants ||
      Boolean(mention.targetRole)
        ? "pinned"
        : preferLanguage
          ? "prefer"
          : "prefer";

    return resolvedConstraint(
      mention,
      mode,
    );
  });

  const seen = new Set<string>();
  const result: ArenaModelConstraint[] = [];

  for (const constraint of constraints) {
    const key = constraintIdentity(constraint);

    if (seen.has(key)) {
      continue;
    }

    seen.add(key);
    result.push(constraint);
  }

  return result.slice(0, 12);
}

export function routeArenaStrategy(
  objective: string,
  userModelConstraints: ArenaModelConstraint[] = [],
): ArenaStrategyPlan {
  const normalized = objective.trim();
  if (!normalized) {
    throw new Error("Arena objective is required.");
  }

  const strategies = inferStrategies(normalized);
  const requiresHiddenState =
    strategies.includes("game") ||
    /secret|hidden role|private role|狼人|隐藏身份|秘密/.test(
      normalized.toLowerCase(),
    );

  const requiresEvaluation =
    strategies.includes("compare") ||
    strategies.includes("competition") ||
    strategies.includes("debate");

  let minParticipants = 2;
  let maxParticipants = 4;

  if (strategies.includes("collaboration")) {
    minParticipants = Math.max(minParticipants, 3);
    maxParticipants = Math.max(maxParticipants, 6);
  }

  if (strategies.includes("simulation")) {
    minParticipants = Math.max(minParticipants, 3);
    maxParticipants = Math.max(maxParticipants, 8);
  }

  if (strategies.includes("game")) {
    minParticipants = Math.max(minParticipants, 4);
    maxParticipants = Math.max(maxParticipants, 12);
  }

  const detectedConstraints = extractUserModelConstraints(normalized);
  const mergedConstraints = [
    ...userModelConstraints,
    ...detectedConstraints,
  ].slice(0, 12);

  return {
    id: crypto.randomUUID(),
    objective: normalized,
    strategies,
    rationale:
      `Arena selected ${strategies.join(" → ")} from the user's objective. ` +
      "Strategies are internal orchestration choices; the user does not need to select a mode.",
    requiresHiddenState,
    requiresEvaluation,
    suggestedMinParticipants: minParticipants,
    suggestedMaxParticipants: maxParticipants,
    userModelConstraints: mergedConstraints,
  };
}
