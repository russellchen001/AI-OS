import type {
  ArenaGovernanceInput,
  ArenaOutcome,
  ArenaScore,
  ArenaVote,
} from "../types/arenaGovernance";

type WinnerScope = "participant" | "team";

function normalized(values: string[]): string[] {
  return [...new Set(values)].sort();
}

function seededRank(value: string, seed: number): number {
  let hash = seed >>> 0;
  for (const char of value) {
    hash = Math.imul(
      hash ^ char.charCodeAt(0),
      16777619,
    ) >>> 0;
  }
  return hash;
}

function topIds(totals: Map<string, number>): string[] {
  if (!totals.size) return [];
  const best = Math.max(...totals.values());
  return [...totals]
    .filter(([, value]) => value === best)
    .map(([id]) => id)
    .sort();
}

function singleScope(
  items: Array<{ scope: string }>,
): WinnerScope | undefined {
  const scopes = [
    ...new Set(
      items
        .map((item) => item.scope)
        .filter(
          (scope): scope is WinnerScope =>
            scope === "participant" ||
            scope === "team",
        ),
    ),
  ];

  return scopes.length === 1
    ? scopes[0]
    : undefined;
}

function scoreWinners(
  scores: ArenaScore[],
): {
  scope?: WinnerScope;
  ids: string[];
} {
  const scope = singleScope(scores);
  if (!scope) return { ids: [] };

  const totals = new Map<string, number>();

  for (const score of scores) {
    if (score.scope !== scope) continue;

    totals.set(
      score.subjectId,
      (totals.get(score.subjectId) ?? 0) +
        score.value,
    );
  }

  return {
    scope,
    ids: topIds(totals),
  };
}

function voteWinners(
  votes: ArenaVote[],
): {
  scope?: WinnerScope;
  ids: string[];
} {
  const scope = singleScope(votes);
  if (!scope) return { ids: [] };

  const totals = new Map<string, number>();

  for (const vote of votes) {
    if (vote.scope !== scope) continue;

    totals.set(
      vote.targetId,
      (totals.get(vote.targetId) ?? 0) +
        vote.weight,
    );
  }

  return {
    scope,
    ids: topIds(totals),
  };
}

function resolveTie(
  ids: string[],
  seed: number,
  seeded: boolean,
): string[] {
  if (ids.length <= 1 || !seeded) {
    return ids;
  }

  return [
    [...ids].sort(
      (a, b) =>
        seededRank(a, seed) -
          seededRank(b, seed) ||
        a.localeCompare(b),
    )[0],
  ];
}

export function createArenaOutcome(
  input: ArenaGovernanceInput,
): ArenaOutcome {
  const scores = input.scores ?? [];
  const votes = input.votes ?? [];
  const provenance = input.provenance ?? [];
  const rationale = input.rationale ?? [];

  if (
    input.runtimeStatus === "stopped" ||
    input.runtimeStatus === "cancelled" ||
    input.runtimeStatus === "failed"
  ) {
    return {
      strategy: input.strategy,
      seed: input.seed,
      status: input.runtimeStatus,
      completionReason: input.runtimeStatus,
      winnerIds: [],
      scores,
      votes,
      provenance,
      rationale,
    };
  }

  let winnerScope =
    input.explicitWinnerScope;

  let winners =
    normalized(input.explicitWinnerIds ?? []);

  if (!winners.length) {
    const scored = scoreWinners(scores);
    winners = scored.ids;
    winnerScope = scored.scope;
  }

  if (!winners.length) {
    const voted = voteWinners(votes);
    winners = voted.ids;
    winnerScope = voted.scope;
  }

  const tied = winners.length > 1;

  winners = resolveTie(
    winners,
    input.seed,
    input.tiePolicy === "seeded",
  );

  const noDecision =
    !winners.length &&
    !input.consensusReached &&
    !input.ruleComplete;

  const completionReason =
    winners.length
      ? "winner"
      : input.consensusReached
        ? "consensus"
        : input.ruleComplete
          ? "rule-complete"
          : "max-rounds";

  return {
    strategy: input.strategy,
    seed: input.seed,
    status:
      input.degraded
        ? "degraded"
        : tied &&
            input.tiePolicy !== "seeded"
          ? "no-consensus"
          : noDecision
            ? "no-consensus"
            : "completed",
    completionReason,
    winnerIds: winners,
    winnerScope:
      winners.length
        ? winnerScope
        : undefined,
    scores,
    votes,
    provenance,
    rationale,
  };
}
