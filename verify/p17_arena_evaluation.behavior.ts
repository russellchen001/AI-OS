import {
  parseArenaEvaluation,
} from "../src/services/arenaEvaluationParser";
import {
  ARENA_EVALUATION_CRITERIA,
  buildArenaEvaluationInstruction,
} from "../src/services/arenaEvaluationPolicy";
import {
  buildArenaStrategyDriver,
} from "../src/services/arenaStrategyDrivers";
import { collectArenaOutcome } from "../src/services/arenaOutcome";
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

for (const strategy of Object.keys(ARENA_EVALUATION_CRITERIA) as ArenaStrategy[]) {
  const instruction = buildArenaEvaluationInstruction(strategy, assembly(strategy), "Evaluate");
  const example = instruction.split("\n").find((line) => line.startsWith("{"));
  const parsed = example ? parseArenaEvaluation(example) : undefined;
  assert(parsed?.scores[0]?.subjectId === "p1" &&
    ARENA_EVALUATION_CRITERIA[strategy].includes(parsed.scores[0].criterionId),
    `${strategy}: generated evaluation example must parse with actual IDs and criteria`);
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

const qualityPlan = { ...assembly("collaboration"), objective: "制定三步的本地笔记备份检查清单。" };
const qualityOutcome = (answer?: string, taskComplete = true, objective = qualityPlan.objective) => collectArenaOutcome({
  assembly: { ...qualityPlan, objective }, driver: buildArenaStrategyDriver("collaboration", { ...qualityPlan, objective }),
  runtimeStatus: "completed", roundsCompleted: 3, seed: 1, degraded: false,
  contexts: [{ id: "quality-eval", round: 3, authorId: "judge", visibility: { kind: "public" },
    content: JSON.stringify({ answer, taskComplete, incompleteReason: "Missing deliverable", summary: "Looks good", scores: [{ scope: "participant", subjectId: "p1", criterionId: "coverage", value: 99 }], ruleComplete: true }),
    tags: ["collaboration-synthesis"], createdAt: 1 }],
});
for (const rejected of [qualityOutcome(), qualityOutcome("1. 检查\n2. 复制"), qualityOutcome("1. 检查\n2. 复制\n3. 核对", false)]) {
  assert(rejected.outcome.status === "degraded" && rejected.outcome.scores.length === 0, "incomplete answer received normal scores");
  assert(rejected.errors.some((error) => error.includes("incomplete")), "incomplete reason was lost");
}
const rawAnswer = "1. 核对两份笔记路径，不修改原件。\n2. 检查已有本地副本是否存在。\n3. 只读比较副本内容与日期。";
const complete = qualityOutcome(rawAnswer);
assert(complete.outcome.finalAnswer === rawAnswer && complete.outcome.scores.length === 1, "final answer was transformed or lost");
console.log("PASS — incomplete task answers cannot receive normal scores; complete raw answer preserved exactly");
for (const objective of [
  "说明3-2-1原则对应关系、自动化方式、每季度恢复演练步骤、主要风险与取舍。",
  "给出年度费用估算，逐项金额、依据、合计。",
  "不超过5个汉字。",
]) {
  const rejected = qualityOutcome(rawAnswer, true, objective);
  assert(rejected.outcome.status === "degraded" && rejected.outcome.scores.length === 0, "missing requested coverage or length violation received normal Arena scores");
}
const estimate = "已有设备按新增0元/年估算；云端按每月30元估算360元/年；年度合计360元。";
assert(qualityOutcome(estimate, true, "给出年度费用估算，逐项金额、依据、合计。").outcome.scores.length === 1, "complete expense answer was rejected");
assert(qualityOutcome("年度费用（预算分配上限）：云端360元，NAS电费118元，预留122元，合计600元。", true, "给出年度费用估算。").outcome.scores.length === 0, "budget allocation was scored as an estimate");

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
