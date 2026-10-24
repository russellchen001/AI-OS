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
    "Return ONLY compact JSON following this valid example; replace example scores and text with your evaluation:",
    JSON.stringify({
      summary: "Brief evaluation",
      scores: [{
        scope: participants.length ? "participant" : "strategy",
        subjectId: participants[0] ?? strategy,
        criterionId: ARENA_EVALUATION_CRITERIA[strategy][0],
        value: 50,
        rationale: "Brief evidence",
      }],
      votes: [],
      explicitWinnerIds: [],
      consensusReached: false,
      ruleComplete: true,
      rationale: ["Brief conclusion"],
    }),
    "",
    "Rules:",
    '- Each scope must be exactly one of "participant", "team", or "strategy"; use only the listed IDs and criteria.',
    `- EVERY score requires scope, subjectId, criterionId and numeric value. For strategy scores, subjectId MUST be "${strategy}"; never omit subjectId.`,
    "- Keep scores compact: at most one score per participant and one strategy score. JSON field requirements take priority over prose length limits.",
    "- Keep the summary and each rationale to one short sentence; finish the complete JSON object.",
    "- Score values must be finite numbers between 0 and 100.",
    "- Vote weights must be finite positive numbers.",
    "- Evaluate only visible Arena context.",
    "- Never infer participant-private, team-private, or runtime-private information that was not projected to you.",
    "- Do not execute tools, Skills, OpenClaw actions, system actions, or external side effects.",
  ].join("\n");
}
