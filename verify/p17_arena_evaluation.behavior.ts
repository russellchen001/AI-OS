import {
  parseArenaEvaluation,
} from "../src/services/arenaEvaluationParser";
import {
  buildArenaStrategyDriver,
} from "../src/services/arenaStrategyDrivers";
import type {
  ArenaAssemblyPlan,
  ArenaModelAssignment,
  ArenaSeatRequirement,
  ArenaStrategy,
} from "../src/types/arena";
import type {
  AiCenterModelChoice,
} from "../src/services/aiCenter";

function assert(
  value: unknown,
  message: string,
): void {
  if (!value) {
    throw new Error(message);
  }
}

function model(
  id: string,
): AiCenterModelChoice {
  return {
    providerId: `provider-${id}`,
    providerInstanceId: `instance-${id}`,
    modelId: id,
    label: id,
  };
}

function seat(
  id: string,
  role: ArenaSeatRequirement["role"],
  team?: string,
): ArenaSeatRequirement {
  return {
    id,
    title: id,
    purpose: id,
    requiredExpertise: [],
    role,
    team,
    modelConstraint: { mode: "auto" },
  };
}

function assignment(
  seatId: string,
): ArenaModelAssignment {
  return {
    seatId,
    choice: model(seatId),
    policy: "auto",
    fallbackChoices: [],
    rationale: "test",
  };
}

function assembly(
  strategy: ArenaStrategy,
): ArenaAssemblyPlan {
  const seats = [
    seat("p1", "participant", "blue"),
    seat("p2", "participant", "red"),
    seat("judge", "judge"),
    seat("moderator", "moderator"),
    seat("observer", "observer"),
    seat("rules", "rules-controller"),
    seat("environment", "environment"),
    seat("synth", "synthesizer"),
  ];

  return {
    id: `assembly-${strategy}`,
    objective: `test ${strategy}`,
    strategyPlan: {
      id: `plan-${strategy}`,
      objective: `test ${strategy}`,
      strategies: [strategy],
      rationale: "test",
      requiresHiddenState:
        strategy === "game" ||
        strategy === "role-play" ||
        strategy === "simulation",
      requiresEvaluation: true,
      suggestedMinParticipants: 2,
      suggestedMaxParticipants: 8,
      userModelConstraints: [],
    },
    seats,
    modelAssignments:
      seats.map((item) => assignment(item.id)),
    rationale: "test",
    chiefOfStaff: {
      mode: "deterministic-fallback",
      selectionRationale: "test",
      attempts: 0,
    },
  };
}

const valid = parseArenaEvaluation(
  JSON.stringify({
    summary: "evaluation",
    scores: [
      {
        scope: "participant",
        subjectId: "p1",
        criterionId: "quality",
        value: 90,
      },
      {
        scope: "team",
        subjectId: "blue",
        criterionId: "integration",
        value: 80,
      },
      {
        scope: "strategy",
        subjectId: "competition",
        criterionId: "quality",
        value: 75,
      },
    ],
    votes: [
      {
        scope: "participant",
        targetId: "p1",
        weight: 1,
      },
    ],
    explicitWinnerIds: ["p1"],
    consensusReached: false,
    ruleComplete: true,
    rationale: ["supported"],
  }),
);

assert(
  valid?.scores.length === 3,
  "participant/team/strategy scopes were not accepted",
);

assert(
  valid?.votes.length === 1,
  "valid vote was not accepted",
);

assert(
  parseArenaEvaluation(
    '{"scores":[{"scope":"participant","subjectId":"p","criterionId":"x","value":101}]}',
  ) === undefined,
  "out-of-range score accepted",
);

assert(
  parseArenaEvaluation(
    '{"scores":[{"scope":"participant","subjectId":"p","criterionId":"x","value":50}],"votes":[{"scope":"participant","targetId":"p","weight":0}]}',
  ) === undefined,
  "invalid vote weight accepted",
);

assert(
  parseArenaEvaluation("not json") === undefined,
  "malformed evaluation accepted",
);

const strategies: ArenaStrategy[] = [
  "compare",
  "debate",
  "collaboration",
  "competition",
  "role-play",
  "game",
  "simulation",
];

for (const strategy of strategies) {
  const driver =
    buildArenaStrategyDriver(
      strategy,
      assembly(strategy),
    );

  const governanceActions =
    driver.actions.filter((action) =>
      action.instruction.includes(
        "AI_OS_ARENA_EVALUATION_JSON",
      ),
    );

  assert(
    governanceActions.length >= 1,
    `${strategy} has no structured governance action`,
  );

  assert(
    governanceActions.every(
      (action) =>
        action.outputVisibility === "public" &&
        action.allowFailure === false,
    ),
    `${strategy} governance action is not bounded`,
  );
}

console.log(
  "PASS — participant/team/strategy evaluation accepted",
);
console.log(
  "PASS — malformed evaluation rejected",
);
console.log(
  "PASS — score bounds enforced",
);
console.log(
  "PASS — vote bounds enforced",
);
console.log(
  "PASS — all seven strategies expose bounded structured governance",
);
console.log(
  "P17 ARENA EVALUATION BEHAVIOR: PASS=5 FAIL=0",
);
