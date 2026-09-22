const assert = {
  ok(value: unknown, message = "Assertion failed"): void {
    if (!value) {
      throw new Error(message);
    }
  },

  equal(actual: unknown, expected: unknown, message?: string): void {
    if (actual !== expected) {
      throw new Error(
        message ??
          `Assertion failed: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`,
      );
    }
  },

  strictEqual(actual: unknown, expected: unknown, message?: string): void {
    if (actual !== expected) {
      throw new Error(
        message ??
          `Assertion failed: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`,
      );
    }
  },

  notEqual(actual: unknown, expected: unknown, message?: string): void {
    if (actual === expected) {
      throw new Error(
        message ??
          `Assertion failed: did not expect ${JSON.stringify(expected)}`,
      );
    }
  },

  deepEqual(actual: unknown, expected: unknown, message?: string): void {
    const actualJson = JSON.stringify(actual);
    const expectedJson = JSON.stringify(expected);

    if (actualJson !== expectedJson) {
      throw new Error(
        message ??
          `Assertion failed: expected ${expectedJson}, got ${actualJson}`,
      );
    }
  },
};

import {
  SimulationCouncilService,
} from "../src/services/councilSimulation";
import type {
  CouncilAssemblyPlan,
} from "../src/types/councilAssembly";

const profile: any = {
  profile: {
    profileId: "profile-alice",
    revision: 3,
    status: "active",
    subjectKind: "person",
    evidenceBundleId: "alice-bundle",
    identity: [],
    domainExpertise: [],
    knowledgeModel: [],
    decisionPatterns: [
      {
        statement: "Tests alternatives before committing.",
        confidence: 0.8,
        evidenceIds: ["evidence-1"],
      },
    ],
    reasoningFrameworks: [],
    preferences: [],
    constraints: [],
    behavioralPatterns: [],
    communicationStyle: [],
    representativeExamples: [],
    contradictions: [],
  },
  activeRevision: 3,
  revisionCount: 3,
};

const skill: any = {
  profileId: "profile-alice",
  profileRevision: 3,
  instructions: "Compare alternatives before commitment.",
  examples: [],
};

const basePlan: CouncilAssemblyPlan = {
  id: "assembly-1",
  objective: "objective",
  mode: "dynamic",
  rationale: "Paperclip-backed dynamic assembly.",
  seats: [
    {
      id: "risk-1",
      title: "Risk Analyst",
      purpose: "Challenge assumptions.",
      requiredExpertise: ["risk"],
      kind: "agency-agent",
      source: {
        kind: "agency-agent",
        sourceId: "risk-agent",
        sourceCommit: "agency-commit",
        sourcePath: "risk.md",
        provenanceReferences: ["agency:risk-agent"],
      },
      assignedModel: {
        providerId: "test",
        providerInstanceId: "test-instance",
        modelId: "test-model",
        label: "Test Model",
        rationale: "fixture",
        fallbackModelIds: [],
      },
      memberContext: "risk context",
    },
    {
      id: "synth-2",
      title: "Decision Synthesizer",
      purpose: "Synthesize.",
      requiredExpertise: ["synthesis"],
      kind: "agency-agent",
      source: {
        kind: "agency-agent",
        sourceId: "synth-agent",
        sourceCommit: "agency-commit",
        sourcePath: "synth.md",
        provenanceReferences: ["agency:synth-agent"],
      },
      assignedModel: {
        providerId: "test",
        providerInstanceId: "test-instance",
        modelId: "test-model",
        label: "Test Model",
        rationale: "fixture",
        fallbackModelIds: [],
      },
      memberContext: "synthesis context",
    },
  ],
  deliberation: {
    maxRounds: 2,
    stages: [],
  },
  synthesizerSeatId: "synth-2",
  provenance: {
    paperclip: {
      status: "consumed",
      companyIds: ["company-1"],
      detail: "fixture",
    },
    agencyAgents: {
      status: "consumed",
      selectedAgentPaths: ["risk.md", "synth.md"],
      detail: "fixture",
    },
    references: ["paperclip:api", "agency:risk-agent"],
  },
};

let runtimeRequest: any;

const service = new SimulationCouncilService({
  listProfiles: async () => [] as any,
  getProfile: async () => profile,
  buildPersonaSkill: async () => skill,
  chiefOfStaff: {
    assemble: async () => basePlan,
    assessExecutionBlocker: () => {
      throw new Error("unused");
    },
  } as any,
  runtime: {
    run: async (request: any) => {
      runtimeRequest = request;
      return {
        session: {
          id: "session-1",
          title: "simulation",
          prompt: request.prompt,
          createdAt: 1,
          updatedAt: 1,
          favorite: false,
          steps: [],
          finalAnswer: "done",
          assemblyPlan: request.assemblyPlan,
          metadata: {
            integrationIds: [],
            provenanceReferences:
              request.assemblyPlan.provenance.references,
            assemblyPlanId: request.assemblyPlan.id,
          },
        },
        finalAnswer: "done",
        steps: [],
        metadata: {
          integrationIds: [],
          provenanceReferences:
            request.assemblyPlan.provenance.references,
          contextSources: [],
        },
      };
    },
    cancel: async () => {},
  } as any,
});

const prepared = await service.prepare({
  objective: "How is Alice likely to respond if the supplier raises price by 20%?",
  profileId: "profile-alice",
});

assert.equal(prepared.plan.mode, "simulation");
assert.equal(prepared.plan.simulation?.profileId, "profile-alice");
assert.equal(prepared.plan.simulation?.profileRevision, 3);
assert.equal(prepared.plan.simulation?.humanReviewed, true);
assert.equal(prepared.plan.simulation?.evidenceBundleId, "alice-bundle");

const persona = prepared.plan.seats.find(
  (seat) => seat.kind === "distilled-persona",
);

assert.ok(persona, "distilled persona must become a first-class Council seat");
assert.equal(persona?.source.sourceId, "profile-alice");
assert.ok(
  persona?.source.provenanceReferences.includes(
    "person-profile:profile-alice@3",
  ),
);
assert.ok(
  persona?.source.provenanceReferences.includes(
    "evidence-bundle:alice-bundle",
  ),
);
assert.ok(
  persona?.memberContext.includes(
    "bounded behavioral simulation",
  ),
);
assert.ok(
  prepared.plan.seats.some(
    (seat) => seat.kind === "agency-agent",
  ),
  "supporting Agency expert seats must be preserved",
);
assert.equal(prepared.plan.deliberation.maxRounds, 2);
assert.ok(
  prepared.plan.deliberation.stages
    .find((stage) => stage.id === "cross-review")
    ?.participantSeatIds.includes(persona!.id),
  "simulated person must participate in cross-review",
);

await service.run({
  objective: "How is Alice likely to respond if the supplier raises price by 20%?",
  profileId: "profile-alice",
});

assert.equal(runtimeRequest.assemblyPlan.mode, "simulation");
assert.deepEqual(
  runtimeRequest.context.distilledProfileIds,
  ["profile-alice"],
);

console.log("PASS simulation mode");
console.log("PASS active distilled persona seat");
console.log("PASS P15 provenance retained");
console.log("PASS Agency expert Council retained");
console.log("PASS bounded cross-review");
console.log("PASS runtime simulation handoff");
