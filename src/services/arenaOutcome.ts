import {
  createArenaOutcome,
} from "./arenaGovernance";
import { validateArenaAnswer } from "./answerRequirements";
import {
  parseArenaEvaluation,
  type ParsedArenaEvaluation,
} from "./arenaEvaluationParser";
import {
  ARENA_EVALUATION_CRITERIA,
} from "./arenaEvaluationPolicy";
import type {
  ArenaAssemblyPlan,
  ArenaContextEntry,
  ArenaSessionStatus,
  ArenaStrategyDriverPlan,
} from "../types/arena";
import type {
  ArenaEvaluationProvenance,
  ArenaOutcome,
  ArenaScore,
  ArenaVote,
} from "../types/arenaGovernance";

/** Tag on an evaluator output replaced by its one correction. */
export const ARENA_SUPERSEDED_TAG = "arena-superseded";

type CollectionInput = {
  assembly: ArenaAssemblyPlan;
  driver: ArenaStrategyDriverPlan;
  contexts: ArenaContextEntry[];
  runtimeStatus: ArenaSessionStatus;
  roundsCompleted: number;
  seed: number;
  degraded: boolean;
};

type EvaluationSubjects = Pick<
  CollectionInput,
  "assembly" | "driver"
>;

export type ArenaOutcomeCollection = {
  outcome: ArenaOutcome;
  errors: string[];
};

function subjectValid(
  scope: "participant" | "team" | "strategy",
  id: string,
  input: EvaluationSubjects,
): boolean {
  if (scope === "strategy") {
    return id === input.driver.strategy;
  }

  if (scope === "participant") {
    return input.assembly.seats.some(
      (seat) =>
        seat.role === "participant" &&
        seat.id === id,
    );
  }

  return input.assembly.seats.some(
    (seat) => seat.team === id,
  );
}

function winnerScope(
  ids: string[],
  input: EvaluationSubjects,
): "participant" | "team" | undefined {
  if (!ids.length) return undefined;

  const scopes = ids.map((id) => {
    const participant = subjectValid(
      "participant",
      id,
      input,
    );
    const team = subjectValid(
      "team",
      id,
      input,
    );

    if (participant === team) {
      return undefined;
    }

    return participant
      ? "participant" as const
      : "team" as const;
  });

  if (
    scopes.some((scope) => !scope) ||
    new Set(scopes).size !== 1
  ) {
    return undefined;
  }

  return scopes[0];
}

/** Why an evaluation breaks the governance contract; empty when valid. */
export function arenaEvaluationViolations(
  parsed: ParsedArenaEvaluation,
  input: EvaluationSubjects,
): string[] {
  const allowed = ARENA_EVALUATION_CRITERIA[
    input.driver.strategy
  ];
  const criteria = new Set(allowed);
  const violations: string[] = [];
  const seen = new Set<string>();

  for (const score of parsed.scores) {
    if (!criteria.has(score.criterionId)) {
      violations.push(
        `Unknown criterion "${score.criterionId}"; use only ${allowed.join(", ")}.`,
      );
    }

    if (!subjectValid(score.scope, score.subjectId, input)) {
      violations.push(
        `Unknown ${score.scope} "${score.subjectId}" in scores.`,
      );
    }

    const key = `${score.scope}\u0000${score.subjectId}\u0000${score.criterionId}`;

    if (seen.has(key)) {
      violations.push(
        `${score.scope} "${score.subjectId}" was scored twice on "${score.criterionId}".`,
      );
    }

    seen.add(key);
  }

  for (const vote of parsed.votes) {
    if (!subjectValid(vote.scope, vote.targetId, input)) {
      violations.push(
        `Unknown ${vote.scope} "${vote.targetId}" in votes.`,
      );
    }
  }

  if (
    parsed.explicitWinnerIds.length &&
    !winnerScope(parsed.explicitWinnerIds, input)
  ) {
    violations.push(
      `Winner IDs ${parsed.explicitWinnerIds.join(", ")} must all be listed participant IDs or all be listed team IDs.`,
    );
  }

  return [...new Set(violations)];
}

/** Violations of one raw evaluator output, including unparseable JSON. */
export function arenaEvaluationOutputViolations(
  content: string,
  input: EvaluationSubjects,
): string[] {
  const parsed = parseArenaEvaluation(content);

  return parsed
    ? arenaEvaluationViolations(parsed, input)
    : ["The output was not one complete JSON object in the required shape."];
}

export function collectArenaOutcome(
  input: CollectionInput,
): ArenaOutcomeCollection {
  const evaluationActions = new Map(
    input.driver.actions
      .filter((action) =>
        action.instruction.includes(
          "AI_OS_ARENA_EVALUATION_JSON",
        ),
      )
      .map((action) => [action.id, action]),
  );

  const scores: ArenaScore[] = [];
  const votes: ArenaVote[] = [];
  const provenance: ArenaEvaluationProvenance[] = [];
  const rationale: string[] = [];
  const explicitWinnerIds: string[] = [];
  const explicitWinnerScopes = new Set<
    "participant" | "team"
  >();

  const accepted: ParsedArenaEvaluation[] = [];
  const errors: string[] = [];

  input.contexts.forEach((entry, index) => {
    const actionId = entry.tags?.find(
      (tag) => evaluationActions.has(tag),
    );

    if (!actionId) return;

    const action =
      evaluationActions.get(actionId);

    if (
      !action ||
      entry.visibility.kind !== "public" ||
      !entry.authorId ||
      !action.actorSeatIds.includes(
        entry.authorId,
      )
    ) {
      errors.push(
        `Invalid Arena governance context "${entry.id}".`,
      );
      return;
    }

    if (entry.tags?.includes(ARENA_SUPERSEDED_TAG)) return;

    const parsed =
      parseArenaEvaluation(entry.content);
    const violations = parsed
      ? arenaEvaluationViolations(parsed, input)
      : [];

    if (!parsed || violations.length) {
      errors.push(
        `Arena evaluator "${entry.authorId}" returned invalid governance output${violations.length ? `: ${violations.join(" ")}` : "."}`,
      );
      return;
    }

    if (input.driver.strategy === "collaboration") {
      try {
        if (parsed.taskComplete !== true) throw new Error(parsed.incompleteReason || "Evaluator did not confirm task completion.");
        validateArenaAnswer(input.assembly.objective, parsed.answer ?? "");
      } catch (error) {
        errors.push(`Arena task incomplete: ${error instanceof Error ? error.message : String(error)}`);
        return;
      }
    }

    accepted.push(parsed);

    for (const score of parsed.scores) {
      scores.push({
        ...score,
        evaluatorSeatId: entry.authorId,
      });
    }

    for (const vote of parsed.votes) {
      votes.push({
        ...vote,
        evaluatorSeatId: entry.authorId,
      });
    }

    if (parsed.summary) {
      rationale.push(parsed.summary);
    }

    rationale.push(...parsed.rationale);

    const scope = winnerScope(
      parsed.explicitWinnerIds,
      input,
    );

    if (scope) {
      explicitWinnerScopes.add(scope);
      explicitWinnerIds.push(
        ...parsed.explicitWinnerIds,
      );
    }

    const assignment =
      input.assembly.modelAssignments.find(
        (candidate) =>
          candidate.seatId === entry.authorId,
      );

    provenance.push({
      evaluatorSeatId: entry.authorId,
      evaluationContextId: entry.id,
      sourceContextIds:
        input.contexts
          .slice(0, index)
          .filter(
            (candidate) =>
              candidate.visibility.kind ===
              "public",
          )
          .map((candidate) => candidate.id),
      providerId:
        assignment?.choice.providerId,
      providerInstanceId:
        assignment?.choice.providerInstanceId,
      modelId:
        assignment?.choice.modelId,
    });
  });

  if (
    input.runtimeStatus === "completed" &&
    input.assembly.strategyPlan
      .requiresEvaluation &&
    accepted.length === 0
  ) {
    errors.push(
      "Arena required evaluation but no valid governance result was produced.",
    );
  }

  if (explicitWinnerScopes.size > 1) {
    explicitWinnerIds.length = 0;
    errors.push(
      "Arena evaluators produced incompatible winner scopes.",
    );
  }

  const explicitWinnerScope =
    explicitWinnerScopes.size === 1
      ? [...explicitWinnerScopes][0]
      : undefined;

  return {
    outcome: { ...createArenaOutcome({
      strategy: input.driver.strategy,
      seed: input.seed,
      runtimeStatus: input.runtimeStatus,
      round: input.roundsCompleted,
      maxRounds: input.driver.maxRounds,
      degraded:
        input.degraded ||
        errors.length > 0,
      explicitWinnerIds:
        [...new Set(explicitWinnerIds)]
          .sort(),
      explicitWinnerScope,
      consensusReached:
        accepted.length > 0 &&
        accepted.every(
          (item) =>
            item.consensusReached,
        ),
      ruleComplete:
        accepted.length > 0 &&
        accepted.every(
          (item) =>
            item.ruleComplete,
        ),
      tiePolicy: "preserve",
      scores,
      votes,
      provenance,
      rationale,
    }), finalAnswer: accepted.map((item) => item.answer).filter(Boolean).join("\n\n") || undefined },
    errors,
  };
}
