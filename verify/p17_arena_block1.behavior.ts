
const assert = {
  equal(
    actual: unknown,
    expected: unknown,
    message?: string,
  ): void {
    if (actual !== expected) {
      throw new Error(
        message ??
          `Expected ${String(expected)}, received ${String(actual)}`,
      );
    }
  },

  notEqual(
    actual: unknown,
    expected: unknown,
    message?: string,
  ): void {
    if (actual === expected) {
      throw new Error(
        message ??
          `Expected values to differ, received ${String(actual)}`,
      );
    }
  },

  ok(
    value: unknown,
    message?: string,
  ): void {
    if (!value) {
      throw new Error(
        message ?? "Expected value to be truthy.",
      );
    }
  },

  match(
    value: string,
    pattern: RegExp,
    message?: string,
  ): void {
    if (!pattern.test(value)) {
      throw new Error(
        message ??
          `Expected ${JSON.stringify(value)} to match ${String(pattern)}`,
      );
    }
  },

  deepEqual(
    actual: unknown,
    expected: unknown,
    message?: string,
  ): void {
    const actualJson = JSON.stringify(actual);
    const expectedJson = JSON.stringify(expected);

    if (actualJson !== expectedJson) {
      throw new Error(
        message ??
          `Expected ${expectedJson}, received ${actualJson}`,
      );
    }
  },

  throws(
    fn: () => unknown,
    expected?: RegExp,
    message?: string,
  ): void {
    try {
      fn();
    } catch (error) {
      if (
        expected &&
        !expected.test(
          error instanceof Error
            ? error.message
            : String(error),
        )
      ) {
        throw new Error(
          message ??
            `Thrown error did not match ${String(expected)}: ${
              error instanceof Error
                ? error.message
                : String(error)
            }`,
        );
      }

      return;
    }

    throw new Error(
      message ?? "Expected function to throw.",
    );
  },

  async rejects(
    promise: Promise<unknown>,
    expected?: RegExp,
    message?: string,
  ): Promise<void> {
    try {
      await promise;
    } catch (error) {
      if (
        expected &&
        !expected.test(
          error instanceof Error
            ? error.message
            : String(error),
        )
      ) {
        throw new Error(
          message ??
            `Rejected error did not match ${String(expected)}: ${
              error instanceof Error
                ? error.message
                : String(error)
            }`,
        );
      }

      return;
    }

    throw new Error(
      message ?? "Expected promise to reject.",
    );
  },
};

import {
  extractUserModelConstraints,
} from "../src/services/arenaStrategyRouter";
import {
  assignArenaModels,
} from "../src/services/arenaModelAssignment";
import {
  resolveUserConstraintsForSeats,
} from "../src/services/arenaConstraintBinding";

import type {
  AiCenterModelChoice,
} from "../src/services/aiCenter";

import type {
  ArenaModelConstraint,
  ArenaSeatRequirement,
  ArenaSeatRole,
} from "../src/types/arena";

type TestCase = {
  name: string;
  run: () => void;
};

const models: AiCenterModelChoice[] = [
  {
    providerId: "provider-a",
    providerInstanceId: "instance-a",
    modelId: "orion-x1",
    label: "Provider A · Orion X1",
  },
  {
    providerId: "provider-b",
    providerInstanceId: "instance-b",
    modelId: "nebula-r2",
    label: "Provider B · Nebula R2",
  },
  {
    providerId: "provider-c",
    providerInstanceId: "instance-c",
    modelId: "helios-j9",
    label: "Provider C · Helios J9",
  },
  {
    providerId: "provider-d",
    providerInstanceId: "instance-d",
    modelId: "aster-m4",
    label: "Provider D · Aster M4",
  },
];

function nonAuto(
  constraint: ArenaModelConstraint,
): Exclude<ArenaModelConstraint, { mode: "auto" }> {
  assert.notEqual(
    constraint.mode,
    "auto",
    "expected a non-auto constraint",
  );

  return constraint as Exclude<
    ArenaModelConstraint,
    { mode: "auto" }
  >;
}

function seat(
  id: string,
  role: ArenaSeatRole,
  modelConstraint: ArenaModelConstraint,
): ArenaSeatRequirement {
  return {
    id,
    title: `${role}-${id}`,
    purpose: `test ${role}`,
    requiredExpertise: [],
    role,
    modelConstraint,
  };
}

const tests: TestCase[] = [
  {
    name: "catalog-driven arbitrary model names: explicit two-model comparison",
    run: () => {
      const result = extractUserModelConstraints(
        "让 orion-x1 和 nebula-r2 比较这个架构",
        models,
      );

      assert.equal(result.length, 2);

      assert.equal(result[0].mode, "pinned");
      assert.equal(result[1].mode, "pinned");

      assert.equal(
        nonAuto(result[0]).modelId,
        "orion-x1",
      );
      assert.equal(
        nonAuto(result[1]).modelId,
        "nebula-r2",
      );

      assert.equal(
        nonAuto(result[0]).targetRole,
        undefined,
      );
      assert.equal(
        nonAuto(result[1]).targetRole,
        undefined,
      );
    },
  },

  {
    name: "arbitrary model can be assigned Judge role",
    run: () => {
      const result = extractUserModelConstraints(
        "让 orion-x1 和 nebula-r2 比较，helios-j9 当裁判",
        models,
      );

      assert.equal(result.length, 3);

      const judge = result.find(
        (constraint) =>
          constraint.mode !== "auto" &&
          constraint.modelId === "helios-j9",
      );

      if (!judge) {
        throw new Error("Expected judge model constraint.");
      }

      if (judge.mode === "auto") {
        throw new Error(
          "Judge model constraint must not use auto mode.",
        );
      }

      assert.equal(judge.mode, "pinned");
      assert.equal(judge.targetRole, "judge");
    },
  },

  {
    name: "arbitrary model can be assigned Moderator role",
    run: () => {
      const result = extractUserModelConstraints(
        "aster-m4 当主持人",
        models,
      );

      assert.equal(result.length, 1);

      const constraint = nonAuto(result[0]);

      assert.equal(constraint.modelId, "aster-m4");
      assert.equal(constraint.targetRole, "moderator");
      assert.equal(constraint.mode, "pinned");
    },
  },

  {
    name: "hard selection language becomes pinned",
    run: () => {
      const result = extractUserModelConstraints(
        "必须用 nebula-r2",
        models,
      );

      assert.equal(result.length, 1);
      assert.equal(result[0].mode, "pinned");
      assert.equal(
        nonAuto(result[0]).modelId,
        "nebula-r2",
      );
    },
  },

  {
    name: "preference language remains prefer",
    run: () => {
      const result = extractUserModelConstraints(
        "最好用 orion-x1",
        models,
      );

      assert.equal(result.length, 1);
      assert.equal(result[0].mode, "prefer");
      assert.equal(
        nonAuto(result[0]).modelId,
        "orion-x1",
      );
    },
  },

  {
    name: "plain single model mention is a soft preference",
    run: () => {
      const result = extractUserModelConstraints(
        "orion-x1",
        models,
      );

      assert.equal(result.length, 1);
      assert.equal(result[0].mode, "prefer");
    },
  },

  {
    name: "semantic binder sends role-targeted model to Judge seat",
    run: () => {
      const constraints =
        extractUserModelConstraints(
          "让 orion-x1 和 nebula-r2 比较，helios-j9 当裁判",
          models,
        );

      const resolved =
        resolveUserConstraintsForSeats(
          [
            "participant",
            "participant",
            "judge",
          ],
          constraints,
        );

      assert.equal(
        nonAuto(resolved[2]).modelId,
        "helios-j9",
      );
      assert.equal(
        nonAuto(resolved[2]).targetRole,
        "judge",
      );

      const participantIds = [
        nonAuto(resolved[0]).modelId,
        nonAuto(resolved[1]).modelId,
      ].sort();

      assert.deepEqual(
        participantIds,
        ["nebula-r2", "orion-x1"],
      );
    },
  },

  {
    name: "semantic binder sends arbitrary model to Moderator seat",
    run: () => {
      const constraints =
        extractUserModelConstraints(
          "aster-m4 当主持人",
          models,
        );

      const resolved =
        resolveUserConstraintsForSeats(
          ["participant", "moderator"],
          constraints,
        );

      assert.equal(resolved[0].mode, "auto");

      assert.equal(
        nonAuto(resolved[1]).modelId,
        "aster-m4",
      );
      assert.equal(
        nonAuto(resolved[1]).targetRole,
        "moderator",
      );
    },
  },

  {
    name: "unscoped required model is not accidentally placed into Judge",
    run: () => {
      const constraint: ArenaModelConstraint = {
        mode: "pinned",
        providerId: "provider-a",
        providerInstanceId: "instance-a",
        modelId: "orion-x1",
        label: "orion-x1",
      };

      const resolved =
        resolveUserConstraintsForSeats(
          ["judge", "participant"],
          [constraint],
        );

      assert.equal(resolved[0].mode, "auto");
      assert.equal(
        nonAuto(resolved[1]).modelId,
        "orion-x1",
      );
    },
  },

  {
    name: "required role without compatible seat hard-fails",
    run: () => {
      const constraint: ArenaModelConstraint = {
        mode: "pinned",
        providerId: "provider-c",
        providerInstanceId: "instance-c",
        modelId: "helios-j9",
        label: "helios-j9",
        targetRole: "judge",
      };

      assert.throws(
        () =>
          resolveUserConstraintsForSeats(
            ["participant", "participant"],
            [constraint],
          ),
        /cannot satisfy.*required model constraint/i,
      );
    },
  },

  {
    name: "no user model constraint leaves seats AUTO",
    run: () => {
      const resolved =
        resolveUserConstraintsForSeats(
          [
            "participant",
            "judge",
            "moderator",
          ],
          [],
        );

      assert.deepEqual(
        resolved.map((constraint) => constraint.mode),
        ["auto", "auto", "auto"],
      );
    },
  },

  {
    name: "resolved pinned assignment has zero fallbacks",
    run: () => {
      const assignments = assignArenaModels(
        [
          seat(
            "a",
            "participant",
            {
              mode: "pinned",
              providerId: "provider-a",
              providerInstanceId: "instance-a",
              modelId: "orion-x1",
              label: "orion-x1",
            },
          ),
        ],
        models,
      );

      assert.equal(
        assignments[0].choice.modelId,
        "orion-x1",
      );
      assert.equal(
        assignments[0].policy,
        "pinned",
      );
      assert.deepEqual(
        assignments[0].fallbackChoices,
        [],
      );
    },
  },

  {
    name: "unavailable required model hard-fails with zero silent substitution",
    run: () => {
      assert.throws(
        () =>
          assignArenaModels(
            [
              seat(
                "missing",
                "participant",
                {
                  mode: "pinned",
                  label: "nonexistent-model-xyz",
                },
              ),
            ],
            models,
          ),
        /unavailable.*will not silently replace/i,
      );
    },
  },

  {
    name: "ambiguous unresolved required model hard-fails",
    run: () => {
      const ambiguousModels: AiCenterModelChoice[] = [
        {
          providerId: "provider-a",
          providerInstanceId: "instance-a",
          modelId: "orion-x1",
          label: "Provider A · Orion Alpha",
        },
        {
          providerId: "provider-z",
          providerInstanceId: "instance-z",
          modelId: "orion-x2",
          label: "Provider Z · Orion Beta",
        },
      ];

      assert.throws(
        () =>
          assignArenaModels(
            [
              seat(
                "ambiguous",
                "participant",
                {
                  mode: "pinned",
                  label: "orion",
                },
              ),
            ],
            ambiguousModels,
          ),
        /ambiguous.*will not guess or silently substitute/i,
      );
    },
  },

  {
    name: "prefer may legally fall back",
    run: () => {
      const assignments = assignArenaModels(
        [
          seat(
            "prefer",
            "participant",
            {
              mode: "prefer",
              label: "not-connected-preference",
            },
          ),
        ],
        models,
      );

      assert.equal(
        assignments[0].policy,
        "prefer",
      );
      assert.ok(
        assignments[0].choice,
      );
      assert.ok(
        assignments[0].fallbackChoices.length > 0,
      );
      assert.match(
        assignments[0].rationale,
        /Preferred model was unavailable/i,
      );
    },
  },

  {
    name: "AUTO assignment still uses AI Center catalog",
    run: () => {
      const assignments = assignArenaModels(
        [
          seat(
            "auto",
            "participant",
            { mode: "auto" },
          ),
        ],
        models,
      );

      assert.equal(
        assignments[0].policy,
        "auto",
      );
      assert.equal(
        assignments[0].choice.modelId,
        models[0].modelId,
      );
    },
  },
];

let passed = 0;
let failed = 0;

for (const test of tests) {
  try {
    test.run();
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
  `P17 BLOCK 1 BEHAVIOR: PASS=${passed} FAIL=${failed}`,
);

if (failed > 0) {
  throw new Error(
    `P17 Block 1 behavior verification failed: ${failed} test(s) failed.`,
  );
}
