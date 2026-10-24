import {
  ArenaContextStore,
} from "../src/services/arenaContextStore";
import {
  buildArenaStrategyDriver,
} from "../src/services/arenaStrategyDrivers";
import {
  ArenaRuntime,
} from "../src/services/arenaRuntime";
import {
  LocalModelGate,
} from "../src/services/localModelGate";
import type {
  ArenaAssemblyPlan,
  ArenaModelAssignment,
  ArenaSeatRequirement,
  ArenaStrategy,
} from "../src/types/arena";
import type {
  AiCenterModelChoice,
  AiCenterStream,
} from "../src/services/aiCenter";

function requireAssignedChoice<T>(
  choice: T | undefined,
): T {
  if (!choice) {
    throw new Error(
      "Arena Runtime behavior test expected an assigned AI Center model.",
    );
  }

  return choice;
}


const assert = {
  equal(actual: unknown, expected: unknown, message?: string): void {
    if (actual !== expected) {
      throw new Error(
        message ??
          `Expected ${String(expected)}, received ${String(actual)}`,
      );
    }
  },

  ok(value: unknown, message?: string): void {
    if (!value) {
      throw new Error(message ?? "Expected value to be truthy.");
    }
  },

  match(value: string, pattern: RegExp, message?: string): void {
    if (!pattern.test(value)) {
      throw new Error(
        message ??
          `Expected ${JSON.stringify(value)} to match ${String(pattern)}`,
      );
    }
  },
};

function model(
  id: string,
): AiCenterModelChoice {
  return {
    providerId: `provider-${id}`,
    providerInstanceId: `instance-${id}`,
    modelId: id,
    label: `Model ${id}`,
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
  seats: ArenaSeatRequirement[],
): ArenaAssemblyPlan {
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
    modelAssignments: seats.map((item) =>
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
  responseText: string,
): AiCenterStream {
  return {
    operationId: crypto.randomUUID(),
    result: Promise.resolve({
      response: {
        providerId: "test",
        modelId: "test",
        text: responseText,
        metadata: {} as never,
      },
      cancelled: false,
    }),
    cancel: async () => undefined,
  };
}

const tests: Array<{
  name: string;
  run: () => void | Promise<void>;
}> = [
  {
    name: "all seven strategies use one shared driver contract",
    run: () => {
      const strategies: ArenaStrategy[] = [
        "compare",
        "debate",
        "collaboration",
        "competition",
        "role-play",
        "game",
        "simulation",
      ];

      const base = [
        seat("p1", "participant"),
        seat("p2", "participant"),
        seat("judge", "judge"),
        seat("moderator", "moderator"),
        seat("rules", "rules-controller"),
        seat("environment", "environment"),
        seat("observer", "observer"),
        seat("synth", "synthesizer"),
      ];

      for (const strategy of strategies) {
        const driver = buildArenaStrategyDriver(
          strategy,
          assembly(strategy, base),
        );

        assert.equal(driver.strategy, strategy);
        assert.ok(driver.actions.length > 0);
        assert.ok(driver.maxRounds >= 2);
      }
    },
  },

  {
    name: "participant-private context never leaks",
    run: () => {
      const store = new ArenaContextStore();

      store.add({
        round: 0,
        visibility: { kind: "public" },
        content: "public fact",
      });

      store.add({
        round: 0,
        visibility: {
          kind: "participant-private",
          participantId: "alice",
        },
        content: "alice secret",
      });

      const alice = {
        seatId: "alice",
        title: "Alice",
        role: "participant" as const,
        assignedModel: model("alice"),
      };

      const bob = {
        seatId: "bob",
        title: "Bob",
        role: "participant" as const,
        assignedModel: model("bob"),
      };

      const aliceView =
        store.renderForParticipant(alice);
      const bobView =
        store.renderForParticipant(bob);

      assert.match(aliceView, /alice secret/);
      assert.equal(
        bobView.includes("alice secret"),
        false,
      );
      assert.match(bobView, /public fact/);
    },
  },

  {
    name: "team-private context only reaches same team",
    run: () => {
      const store = new ArenaContextStore();

      store.add({
        round: 0,
        visibility: {
          kind: "team-private",
          teamId: "wolves",
        },
        content: "wolf channel",
      });

      const wolfA = {
        seatId: "a",
        title: "A",
        role: "participant" as const,
        team: "wolves",
        assignedModel: model("a"),
      };

      const wolfB = {
        seatId: "b",
        title: "B",
        role: "participant" as const,
        team: "wolves",
        assignedModel: model("b"),
      };

      const villager = {
        seatId: "c",
        title: "C",
        role: "participant" as const,
        team: "village",
        assignedModel: model("c"),
      };

      assert.match(
        store.renderForParticipant(wolfA),
        /wolf channel/,
      );
      assert.match(
        store.renderForParticipant(wolfB),
        /wolf channel/,
      );
      assert.equal(
        store
          .renderForParticipant(villager)
          .includes("wolf channel"),
        false,
      );
    },
  },

  {
    name: "runtime-private world state is invisible to every participant",
    run: () => {
      const store = new ArenaContextStore();

      store.add({
        round: 0,
        visibility: {
          kind: "runtime-private",
        },
        content: "internal world truth",
      });

      const player = {
        seatId: "p",
        title: "P",
        role: "participant" as const,
        assignedModel: model("p"),
      };

      assert.equal(
        store
          .renderForParticipant(player)
          .includes("internal world truth"),
        false,
      );

      assert.equal(
        store.projectForRuntime().length,
        1,
      );
    },
  },

  {
    name: "game driver requires hidden state and world state",
    run: () => {
      const plan = buildArenaStrategyDriver(
        "game",
        assembly("game", [
          seat("p1", "participant"),
          seat("p2", "participant"),
          seat("rules", "rules-controller"),
          seat("observer", "observer"),
        ]),
      );

      assert.equal(plan.requiresHiddenState, true);
      assert.equal(plan.requiresWorldState, true);
    },
  },

  {
    name: "simulation is distinct and stateful",
    run: () => {
      const plan = buildArenaStrategyDriver(
        "simulation",
        assembly("simulation", [
          seat("actor-a", "participant"),
          seat("actor-b", "participant"),
          seat("env", "environment"),
          seat("eval", "evaluator"),
        ]),
      );

      assert.equal(plan.strategy, "simulation");
      assert.equal(plan.requiresWorldState, true);
      assert.ok(
        plan.actions.some(
          (item) =>
            item.id === "simulation-transition",
        ),
      );
    },
  },

  {
    name: "runtime executes strategy through assigned AI Center models",
    run: async () => {
      let invocations = 0;

      const runtime = new ArenaRuntime({
        stream: (_messages, choice) => {
          invocations += 1;
          return fakeStream(
            `response from ${requireAssignedChoice(choice).modelId}`,
          );
        },
        now: (() => {
          let value = 1000;
          return () => {
            value += 10;
            return value;
          };
        })(),
        uuid: () => "runtime-test-session",
      });

      const result = await runtime.run(
        assembly("competition", [
          seat("p1", "participant"),
          seat("p2", "participant"),
          seat("judge", "judge"),
        ]),
        {
          seed: 42,
        },
      );

      assert.equal(result.status, "completed");
      assert.equal(result.strategy, "competition");
      assert.equal(result.roundsCompleted, 2);
      // The judge's non-JSON reply gets its one correction re-ask.
      assert.equal(invocations, 4);
      assert.equal(result.metrics.length, 4);
      assert.ok(
        result.contexts.some(
          (entry) =>
            entry.authorId === "judge",
        ),
      );
    },
  },

  {
    name: "hidden role is visible only to its own player during real runtime projection",
    run: async () => {
      const captured = new Map<string, string>();

      const runtime = new ArenaRuntime({
        stream: (messages, choice) => {
          captured.set(
            requireAssignedChoice(choice).modelId,
            messages
              .map((message) => message.content)
              .join("\n"),
          );

          return fakeStream("public action");
        },
        now: () => 1000,
        uuid: () => "hidden-role-session",
      });

      await runtime.run(
        assembly("game", [
          seat("alice", "participant"),
          seat("bob", "participant"),
          seat("rules", "rules-controller"),
          seat("observer", "observer"),
        ]),
        {
          seed: 7,
          hiddenRoles: {
            alice: "werewolf",
            bob: "villager",
          },
        },
      );

      const alicePrompt =
        captured.get("alice") ?? "";
      const bobPrompt =
        captured.get("bob") ?? "";

      assert.match(alicePrompt, /werewolf/);
      assert.equal(
        alicePrompt.includes("villager"),
        false,
      );
      assert.match(bobPrompt, /villager/);
      assert.equal(
        bobPrompt.includes("werewolf"),
        false,
      );
    },
  },

  {
    name: "participant failure is isolated on allowFailure action",
    run: async () => {
      const runtime = new ArenaRuntime({
        stream: (_messages, choice) => {
          if (requireAssignedChoice(choice).modelId === "p1") {
            return {
              operationId: "bad",
              result: Promise.reject(
                new Error("provider failure"),
              ),
              cancel: async () => undefined,
            };
          }

          return fakeStream(
            `ok ${requireAssignedChoice(choice).modelId}`,
          );
        },
        now: () => 1000,
        uuid: () => "failure-session",
      });

      const result = await runtime.run(
        assembly("compare", [
          seat("p1", "participant"),
          seat("p2", "participant"),
          seat("judge", "judge"),
        ]),
        {
          maxRetriesPerInvocation: 0,
        },
      );

      assert.equal(result.degraded, true);
      assert.ok(
        result.metrics.some(
          (metric) =>
            metric.seatId === "p1" &&
            metric.status === "failed",
        ),
      );
    },
  },

  {
    name: "pinned assignment has zero model fallback inside runtime",
    run: async () => {
      const plan = assembly("competition", [
        seat("p1", "participant"),
        seat("p2", "participant"),
        seat("judge", "judge"),
      ]);

      plan.modelAssignments[0] = {
        ...plan.modelAssignments[0],
        policy: "pinned",
        fallbackChoices: [
          model("must-not-run"),
        ],
      };

      const invoked: string[] = [];

      const runtime = new ArenaRuntime({
        stream: (_messages, choice) => {
          invoked.push(requireAssignedChoice(choice).modelId);

          if (requireAssignedChoice(choice).modelId === "p1") {
            return {
              operationId: "pinned-fail",
              result: Promise.reject(
                new Error("pinned unavailable"),
              ),
              cancel: async () => undefined,
            };
          }

          return fakeStream("ok");
        },
        now: () => 1000,
        uuid: () => "pinned-session",
      });

      await runtime.run(plan, {
        maxRetriesPerInvocation: 5,
      });

      assert.equal(
        invoked.includes("must-not-run"),
        false,
      );
    },
  },

  {
    name: "invalid evaluator JSON gets exactly one correction naming each violation",
    run: async () => {
      const prompts: string[] = [];
      const valid = JSON.stringify({
        answer: "1. Copy notes\n2. Verify copy\n3. Restore one note",
        taskComplete: true,
        incompleteReason: "",
        summary: "ok",
        scores: [
          { scope: "participant", subjectId: "p1", criterionId: "coverage", value: 80 },
          { scope: "participant", subjectId: "p1", criterionId: "correctness", value: 70 },
          { scope: "participant", subjectId: "p2", criterionId: "coverage", value: 75 },
        ],
        votes: [],
        explicitWinnerIds: [],
        consensusReached: true,
        ruleComplete: true,
        rationale: ["ok"],
      });
      const invalid = JSON.stringify({
        ...JSON.parse(valid),
        scores: [
          { scope: "participant", subjectId: "p1", criterionId: "consensus", value: 80 },
          { scope: "participant", subjectId: "p2", criterionId: "coverage", value: 75 },
          { scope: "participant", subjectId: "p2", criterionId: "coverage", value: 60 },
        ],
      });

      const runtime = new ArenaRuntime({
        stream: (messages) => {
          const text = messages.map((message) => message.content).join("\n");

          if (!text.includes("AI_OS_ARENA_EVALUATION_JSON")) {
            return fakeStream("1. draft");
          }

          prompts.push(text);
          return fakeStream(prompts.length === 1 ? invalid : valid);
        },
        now: () => 1000,
        uuid: () => "correction-session",
      });

      const result = await runtime.run(
        assembly("collaboration", [
          seat("p1", "participant"),
          seat("p2", "participant"),
          seat("synth", "synthesizer"),
        ]),
      );

      assert.equal(prompts.length, 2);
      assert.match(prompts[1], /Unknown criterion "consensus"/);
      assert.match(prompts[1], /scored twice on "coverage"/);
      assert.equal(result.degraded, false, result.errors.join(" | "));
      assert.equal(result.outcome.scores.length, 3);
      assert.ok(
        result.contexts.some((entry) =>
          entry.tags?.includes("arena-superseded"),
        ),
      );
    },
  },

  {
    name: "evaluator still invalid after its one correction is rejected, never passed",
    run: async () => {
      let evaluations = 0;
      const runtime = new ArenaRuntime({
        stream: (messages) => {
          const text = messages.map((message) => message.content).join("\n");

          if (!text.includes("AI_OS_ARENA_EVALUATION_JSON")) {
            return fakeStream("1. draft");
          }

          evaluations += 1;
          return fakeStream(JSON.stringify({
            answer: "1. a\n2. b\n3. c",
            taskComplete: true,
            summary: "x",
            scores: [{ scope: "participant", subjectId: "ghost", criterionId: "coverage", value: 50 }],
            votes: [],
            explicitWinnerIds: [],
            consensusReached: false,
            ruleComplete: true,
            rationale: [],
          }));
        },
        now: () => 1000,
        uuid: () => "still-invalid-session",
      });

      const result = await runtime.run(
        assembly("collaboration", [
          seat("p1", "participant"),
          seat("p2", "participant"),
          seat("synth", "synthesizer"),
        ]),
      );

      assert.equal(evaluations, 2);
      assert.equal(result.degraded, true);
      assert.equal(result.outcome.scores.length, 0);
      assert.ok(
        result.errors.some((error) => /Unknown participant "ghost"/.test(error)),
        result.errors.join(" | "),
      );
    },
  },

  {
    name: "different local models never generate at the same time; cloud and same-model calls do",
    run: async () => {
      const gate = new LocalModelGate();
      const active = new Map<string, number>();
      let overlapOfDifferentLocal = false;
      let cloudOverlap = false;
      let sameLocalOverlap = false;

      const choices: Record<string, AiCenterModelChoice> = {
        q1: { providerId: "omlx", providerInstanceId: "omlx-local", modelId: "qwen-a", label: "a" },
        q2: { providerId: "omlx", providerInstanceId: "omlx-local", modelId: "qwen-a", label: "a" },
        d1: { providerId: "omlx", providerInstanceId: "omlx-local", modelId: "deepseek", label: "d" },
        c1: model("cloud"),
      };

      const plan = assembly("compare", [
        seat("q1", "participant"),
        seat("q2", "participant"),
        seat("d1", "participant"),
        seat("c1", "participant"),
        seat("judge", "judge"),
      ]);
      plan.modelAssignments = plan.modelAssignments.map((item) => ({
        ...item,
        choice: choices[item.seatId] ?? item.choice,
      }));

      const runtime = new ArenaRuntime({
        stream: (_messages, choice) => {
          const id = requireAssignedChoice(choice).modelId;
          const local = requireAssignedChoice(choice).providerId === "omlx";
          const others = [...active.entries()].filter(([, count]) => count > 0);

          if (local && others.some(([other]) => other !== id && other !== "cloud")) {
            overlapOfDifferentLocal = true;
          }
          if (local && (active.get(id) ?? 0) > 0) sameLocalOverlap = true;
          if (!local && others.length) cloudOverlap = true;
          if (local && (active.get("cloud") ?? 0) > 0) cloudOverlap = true;

          active.set(id, (active.get(id) ?? 0) + 1);

          return {
            operationId: crypto.randomUUID(),
            result: new Promise((resolve) => setTimeout(() => {
              active.set(id, (active.get(id) ?? 1) - 1);
              resolve({
                response: { providerId: "test", modelId: id, text: "ok", metadata: {} as never },
                cancelled: false,
              });
            }, 20)),
            cancel: async () => undefined,
          };
        },
        now: () => 1000,
        uuid: () => "local-gate-session",
        localGate: gate,
      });

      await runtime.run(plan);

      assert.equal(overlapOfDifferentLocal, false);
      assert.equal(sameLocalOverlap, true);
      assert.equal(cloudOverlap, true);
    },
  },
];

let passed = 0;
let failed = 0;

for (const test of tests) {
  try {
    await test.run();
    passed += 1;
    console.log(`PASS — ${test.name}`);
  } catch (error) {
    failed += 1;
    console.error(`FAIL — ${test.name}`);
    console.error(error);
  }
}

console.log("");
console.log(
  `P17 BLOCK 2 BEHAVIOR: PASS=${passed} FAIL=${failed}`,
);

if (failed > 0) {
  throw new Error(
    `P17 Block 2 behavior verification failed: ${failed} test(s) failed.`,
  );
}
