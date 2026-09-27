export type ArenaEvaluationScope =
  | "participant"
  | "team"
  | "strategy";

export type ParsedArenaEvaluationScore = {
  scope: ArenaEvaluationScope;
  subjectId: string;
  criterionId: string;
  value: number;
  rationale?: string;
};

export type ParsedArenaEvaluationVote = {
  scope: ArenaEvaluationScope;
  targetId: string;
  weight: number;
};

export type ParsedArenaEvaluation = {
  summary?: string;
  scores: ParsedArenaEvaluationScore[];
  votes: ParsedArenaEvaluationVote[];
  explicitWinnerIds: string[];
  consensusReached: boolean;
  ruleComplete: boolean;
  rationale: string[];
};

function isScope(
  value: unknown,
): value is ArenaEvaluationScope {
  return value === "participant" ||
    value === "team" ||
    value === "strategy";
}

function isRecord(
  value: unknown,
): value is Record<string, unknown> {
  return typeof value === "object" &&
    value !== null &&
    !Array.isArray(value);
}

function stringList(
  value: unknown,
): string[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value.filter(
    (item): item is string =>
      typeof item === "string" &&
      item.trim().length > 0,
  );
}

export function parseArenaEvaluation(
  text: string,
): ParsedArenaEvaluation | undefined {
  const match = text.match(/\{[\s\S]*\}/);

  if (!match) {
    return undefined;
  }

  let raw: unknown;

  try {
    raw = JSON.parse(match[0]);
  } catch {
    return undefined;
  }

  if (!isRecord(raw) || !Array.isArray(raw.scores)) {
    return undefined;
  }

  if (
    raw.votes !== undefined &&
    !Array.isArray(raw.votes)
  ) {
    return undefined;
  }

  const scores: ParsedArenaEvaluationScore[] = [];

  for (const candidate of raw.scores) {
    if (!isRecord(candidate)) {
      return undefined;
    }

    if (
      !isScope(candidate.scope) ||
      typeof candidate.subjectId !== "string" ||
      !candidate.subjectId.trim() ||
      typeof candidate.criterionId !== "string" ||
      !candidate.criterionId.trim() ||
      typeof candidate.value !== "number" ||
      !Number.isFinite(candidate.value) ||
      candidate.value < 0 ||
      candidate.value > 100
    ) {
      return undefined;
    }

    if (
      candidate.rationale !== undefined &&
      typeof candidate.rationale !== "string"
    ) {
      return undefined;
    }

    scores.push({
      scope: candidate.scope,
      subjectId: candidate.subjectId.trim(),
      criterionId: candidate.criterionId.trim(),
      value: candidate.value,
      rationale:
        typeof candidate.rationale === "string"
          ? candidate.rationale
          : undefined,
    });
  }

  const votes: ParsedArenaEvaluationVote[] = [];

  for (const candidate of raw.votes ?? []) {
    if (!isRecord(candidate)) {
      return undefined;
    }

    if (
      !isScope(candidate.scope) ||
      typeof candidate.targetId !== "string" ||
      !candidate.targetId.trim() ||
      typeof candidate.weight !== "number" ||
      !Number.isFinite(candidate.weight) ||
      candidate.weight <= 0
    ) {
      return undefined;
    }

    votes.push({
      scope: candidate.scope,
      targetId: candidate.targetId.trim(),
      weight: candidate.weight,
    });
  }

  return {
    summary:
      typeof raw.summary === "string"
        ? raw.summary
        : undefined,
    scores,
    votes,
    explicitWinnerIds:
      stringList(raw.explicitWinnerIds),
    consensusReached:
      raw.consensusReached === true,
    ruleComplete:
      raw.ruleComplete === true,
    rationale:
      stringList(raw.rationale),
  };
}
