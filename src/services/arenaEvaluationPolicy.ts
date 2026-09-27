import type {
  ArenaAssemblyPlan,
  ArenaStrategy,
} from "../types/arena";

export const ARENA_EVALUATION_CRITERIA: Record<
  ArenaStrategy,
  readonly string[]
> = {
  compare: [
    "objective-fit",
    "reasoning",
    "evidence",
    "uncertainty",
  ],
  debate: [
    "argument-quality",
    "rebuttal",
    "evidence",
    "uncertainty",
  ],
  collaboration: [
    "coverage",
    "integration",
    "correctness",
    "uncertainty",
  ],
  competition: [
    "objective-fit",
    "correctness",
    "quality",
    "efficiency",
  ],
  "role-play": [
    "role-consistency",
    "interaction-quality",
    "insight",
    "boundary-respect",
  ],
  game: [
    "rule-adherence",
    "decision-quality",
    "state-consistency",
    "fair-play",
  ],
  simulation: [
    "state-consistency",
    "causal-reasoning",
    "uncertainty",
    "counterfactual-quality",
  ],
};

export function buildArenaEvaluationInstruction(
  strategy: ArenaStrategy,
  assembly: ArenaAssemblyPlan,
  baseInstruction: string,
): string {
  const participants = assembly.seats
    .filter((seat) => seat.role === "participant")
    .map((seat) => seat.id)
    .sort();

  const teams = [
    ...new Set(
      assembly.seats.flatMap(
        (seat) => seat.team ? [seat.team] : [],
      ),
    ),
  ].sort();

  return [
    baseInstruction,
    "",
    "AI_OS_ARENA_EVALUATION_JSON",
    `Criteria: ${ARENA_EVALUATION_CRITERIA[strategy].join(", ")}`,
    `Participant IDs: ${participants.join(", ") || "(none)"}`,
    `Team IDs: ${teams.join(", ") || "(none)"}`,
    `Strategy ID: ${strategy}`,
    "",
    "Return ONLY compact JSON using this schema:",
    '{"summary":"...","scores":[{"scope":"participant|team|strategy","subjectId":"...","criterionId":"...","value":0,"rationale":"..."}],"votes":[{"scope":"participant|team|strategy","targetId":"...","weight":1}],"explicitWinnerIds":[],"consensusReached":false,"ruleComplete":true,"rationale":["..."]}',
    "",
    "Rules:",
    "- Score values must be finite numbers between 0 and 100.",
    "- Vote weights must be finite positive numbers.",
    "- Evaluate only visible Arena context.",
    "- Never infer participant-private, team-private, or runtime-private information that was not projected to you.",
    "- Do not execute tools, Skills, OpenClaw actions, system actions, or external side effects.",
  ].join("\n");
}
