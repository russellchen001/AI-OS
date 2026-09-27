import {
  ArenaRuntime,
} from "../src/services/arenaRuntime";
import type {
  ArenaAssemblyPlan,
  ArenaModelAssignment,
  ArenaSeatRequirement,
} from "../src/types/arena";
import type {
  AiCenterModelChoice,
  AiCenterStream,
} from "../src/services/aiCenter";

function assert(
  value: unknown,
  message: string,
): void {
  if (!value) throw new Error(message);
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
    policy: "pinned",
    fallbackChoices: [],
    rationale: "test",
  };
}

function competitionAssembly(): ArenaAssemblyPlan {
  const seats = [
    seat("p1", "participant", "blue"),
    seat("p2", "participant", "red"),
    seat("judge", "judge"),
  ];

  return {
    id: "block3-competition",
    objective: "Choose the stronger candidate.",
    strategyPlan: {
      id: "block3-plan",
      objective: "Choose the stronger candidate.",
      strategies: ["competition"],
      rationale: "test",
      requiresHiddenState: false,
      requiresEvaluation: true,
      suggestedMinParticipants: 2,
      suggestedMaxParticipants: 3,
      userModelConstraints: [],
    },
    seats,
    modelAssignments:
      seats.map((item) =>
        assignment(item.id),
      ),
    rationale: "test",
    chiefOfStaff: {
      mode: "deterministic-fallback",
      selectionRationale: "test",
      attempts: 0,
    },
  };
}

function fakeStream(
  text: string,
): AiCenterStream {
  return {
    operationId: crypto.randomUUID(),
    result: Promise.resolve({
      response: {
        providerId: "test",
        modelId: "test",
        text,
        metadata: {} as never,
      },
      cancelled: false,
    }),
    cancel: async () => undefined,
  };
}

function validEvaluation(): string {
  return JSON.stringify({
    summary: "P1 is stronger.",
    scores: [
      {
        scope: "participant",
        subjectId: "p1",
        criterionId: "quality",
        value: 90,
        rationale: "strong result",
      },
      {
        scope: "participant",
        subjectId: "p2",
        criterionId: "quality",
        value: 70,
      },
      {
        scope: "team",
        subjectId: "blue",
        criterionId: "efficiency",
        value: 80,
      },
      {
        scope: "strategy",
        subjectId: "competition",
        criterionId: "objective-fit",
        value: 85,
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
    consensusReached: true,
    ruleComplete: true,
    rationale: [
      "Evaluation used only projected context.",
    ],
  });
}

async function runWithJudge(
  judgeText: string,
) {
  const runtime = new ArenaRuntime({
    stream: (_messages, choice) => {
      const id = choice?.modelId;

      if (id === "judge") {
        return fakeStream(judgeText);
      }

      return fakeStream(
        `candidate from ${id}`,
      );
    },
    now: (() => {
      let value = 1000;
      return () => value += 10;
    })(),
    uuid: () =>
      `block3-${judgeText.length}`,
  });

  return runtime.run(
    competitionAssembly(),
    {
      seed: 42,
      initialPublicContext: [
        "public evidence",
      ],
      participantPrivateContext: {
        p1: ["p1 private evidence"],
      },
      worldState: {
        hidden: "runtime truth",
      },
    },
  );
}

const result =
  await runWithJudge(
    validEvaluation(),
  );

assert(
  result.status === "completed",
  "valid governed Arena did not complete",
);

assert(
  result.outcome.winnerIds[0] === "p1",
  "structured evaluation did not produce winner",
);

assert(
  result.outcome.winnerScope ===
    "participant",
  "winner scope was not preserved",
);

assert(
  result.outcome.scores.some(
    (score) =>
      score.scope === "participant" &&
      score.subjectId === "p1" &&
      score.evaluatorSeatId === "judge",
  ),
  "participant score was not collected",
);

assert(
  result.outcome.scores.some(
    (score) =>
      score.scope === "team" &&
      score.subjectId === "blue",
  ),
  "team score was not collected",
);

assert(
  result.outcome.scores.some(
    (score) =>
      score.scope === "strategy" &&
      score.subjectId === "competition",
  ),
  "strategy score was not collected",
);

const provenance =
  result.outcome.provenance[0];

assert(
  provenance?.evaluatorSeatId === "judge",
  "evaluator provenance missing",
);

assert(
  provenance?.modelId === "judge",
  "evaluation model provenance missing",
);

const privateIds =
  new Set(
    result.contexts
      .filter(
        (entry) =>
          entry.visibility.kind !== "public",
      )
      .map((entry) => entry.id),
  );

assert(
  provenance.sourceContextIds.every(
    (id) => !privateIds.has(id),
  ),
  "private/runtime context leaked into outcome provenance",
);

assert(
  result.outcome.rationale.some(
    (item) =>
      item.includes("P1 is stronger"),
  ),
  "evaluation rationale missing",
);

const malformed =
  await runWithJudge(
    "not valid evaluation json",
  );

assert(
  malformed.degraded === true,
  "malformed governance did not degrade run",
);

assert(
  malformed.outcome.status ===
    "degraded",
  "malformed governance did not degrade outcome",
);

assert(
  malformed.errors.some(
    (error) =>
      error.includes(
        "invalid governance output",
      ),
  ),
  "malformed governance error missing",
);

const unknown =
  await runWithJudge(
    JSON.stringify({
      scores: [
        {
          scope: "participant",
          subjectId: "ghost",
          criterionId: "quality",
          value: 100,
        },
      ],
      votes: [],
      explicitWinnerIds: ["ghost"],
      consensusReached: true,
      ruleComplete: true,
      rationale: [],
    }),
  );

assert(
  unknown.outcome.status ===
    "degraded",
  "unknown evaluation subject was accepted",
);

console.log(
  "PASS — Runtime produces structured ArenaOutcome",
);
console.log(
  "PASS — participant/team/strategy scores survive Runtime collection",
);
console.log(
  "PASS — evaluator/model provenance is retained",
);
console.log(
  "PASS — private and runtime-private context stay out of provenance",
);
console.log(
  "PASS — rationale is retained",
);
console.log(
  "PASS — malformed governance degrades safely",
);
console.log(
  "PASS — unknown evaluation subjects are rejected",
);
console.log(
  "P17 BLOCK 3 BEHAVIOR: PASS=7 FAIL=0",
);
