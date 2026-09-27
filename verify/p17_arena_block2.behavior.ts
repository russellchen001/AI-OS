import {
  ArenaContextStore,
} from "../src/services/arenaContextStore";
import {
  buildArenaStrategyDriver,
} from "../src/services/arenaStrategyDrivers";
import {
  ArenaRuntime,
} from "../src/services/arenaRuntime";
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
      assert.equal(invocations, 3);
      assert.equal(result.metrics.length, 3);
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
