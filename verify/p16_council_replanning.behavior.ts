import type { CouncilSession } from "../src/types/council";
import type {
  CouncilAssemblyPlan,
  CouncilRecommendation,
} from "../src/types/councilAssembly";
import type {
  CouncilExecutionState,
} from "../src/types/councilExecution";
import {
  CouncilReplanningCoordinator,
} from "../src/services/councilReplanning";

function assert(
  value: unknown,
  message: string,
): void {
  if (!value) throw new Error(message);
}

const recommendation: CouncilRecommendation = {
  id: "recommendation-1",
  councilSessionId: "council-1",
  summary: "Original plan",
  recommendedPlan: ["Use mounted NAS"],
  rationale: [],
  disagreements: [],
  risks: [],
  assumptions: ["NAS is mounted"],
  uncertainty: [],
  provenance: [],
  createdAt: 1,
};

const basePlan: CouncilAssemblyPlan = {
  id: "assembly-1",
  objective: "Solve storage issue",
  mode: "dynamic",
  rationale: "Original Council",
  seats: [
    {
      id: "storage-seat",
      title: "Storage Engineer",
      purpose: "Storage operations",
      requiredExpertise: [
        "storage operations",
      ],
      kind: "agency-agent",
      source: {
        kind: "agency-agent",
        sourceId: "storage-engineer",
        provenanceReferences: [
          "agency-agents:engineering/storage-engineer",
        ],
      },
      assignedModel: {
        providerId: "test",
        providerInstanceId: "test",
        modelId: "test-model",
        label: "Test",
        rationale: "test",
        fallbackModelIds: [],
      },
      memberContext:
        "Storage specialist",
    },
    {
      id: "synth",
      title: "Decision Synthesizer",
      purpose: "Synthesize",
      requiredExpertise: [
        "decision synthesis",
      ],
      kind: "builtin",
      source: {
        kind: "builtin",
        sourceId: "synth",
        provenanceReferences: [],
      },
      assignedModel: {
        providerId: "test",
        providerInstanceId: "test",
        modelId: "test-model",
        label: "Test",
        rationale: "test",
        fallbackModelIds: [],
      },
      memberContext: "Synthesizer",
    },
  ],
  deliberation: {
    maxRounds: 2,
    stages: [
      {
        id: "independent-analysis",
        title: "Independent",
        participantSeatIds: [
          "storage-seat",
        ],
      },
      {
        id: "cross-review",
        title: "Review",
        participantSeatIds: [
          "storage-seat",
        ],
      },
      {
        id: "final-synthesis",
        title: "Synthesis",
        participantSeatIds: [
          "synth",
        ],
      },
    ],
  },
  synthesizerSeatId: "synth",
  provenance: {
    paperclip: {
      status: "consumed",
      detail: "test",
      companyIds: [],
    },
    agencyAgents: {
      status: "consumed",
      selectedAgentPaths: [],
      detail: "test",
    },
    references: [],
  },
};

const session: CouncilSession = {
  id: "council-1",
  title: "Storage",
  prompt: "Fix storage workflow",
  createdAt: 1,
  updatedAt: 1,
  favorite: false,
  steps: [],
  finalAnswer: "Original plan",
  assemblyPlan: basePlan,
  recommendation,
  metadata: {
    integrationIds: [],
    provenanceReferences: [],
    assemblyPlanId: basePlan.id,
  },
};

const blocked: CouncilExecutionState = {
  linkage: {
    councilSessionId: "council-1",
    recommendationId:
      "recommendation-1",
    taskId: "task-1",
  },
  feedback: {
    kind: "blocker",
    status: "blocked",
    councilSessionId: "council-1",
    recommendationId:
      "recommendation-1",
    taskId: "task-1",
    occurredAt: 10,
    message: "blocked",
    reason:
      "NAS mount is unavailable",
    observedReality:
      "Expected NAS path does not exist",
    invalidatedAssumptions: [
      "NAS is mounted",
    ],
    suggestedExpertise: [
      "storage operations",
    ],
  },
};

let assembleCalled = 0;
let runtimeCalled = 0;

const coordinator =
  new CouncilReplanningCoordinator({
    chiefOfStaff: {
      evaluateExecutionBlocker:
        () => ({
          action:
            "reconvene-experts",
          reason:
            "Storage assumption invalidated",
          existingSeatIds: [
            "storage-seat",
          ],
          newSeatRequirements: [
            "storage operations",
          ],
          executionContext: {
            councilSessionId:
              "council-1",
            recommendationId:
              "recommendation-1",
            taskId: "task-1",
            reason:
              "NAS mount unavailable",
            observedReality:
              "Expected NAS path does not exist",
            invalidatedAssumptions: [
              "NAS is mounted",
            ],
          },
          requiresUserApproval: true,
        }),
      assemble: async () => {
        assembleCalled += 1;
        return basePlan;
      },
    } as any,
    runtime: {
      run: async (request: any) => {
        runtimeCalled += 1;

        assert(
          request.assemblyPlan
            .deliberation.maxRounds <= 2,
          "replanning exceeded bounded rounds",
        );

        assert(
          request.assemblyPlan
            .provenance.references
            .includes(
              "execution-feedback:blocker",
            ),
          "blocker provenance lost",
        );

        const revised: CouncilRecommendation = {
          ...recommendation,
          id: "recommendation-2",
          summary:
            "Revised storage plan",
          recommendedPlan: [
            "Reconnect or remount NAS before continuing",
          ],
          assumptions: [],
          createdAt: 20,
        };

        return {
          session: {
            ...session,
            id: "council-replan-1",
            recommendation: revised,
            assemblyPlan:
              request.assemblyPlan,
            finalAnswer:
              "Revised storage plan",
          },
          recommendation: revised,
          finalAnswer:
            "Revised storage plan",
          steps: [],
          metadata: {
            integrationIds: [],
            provenanceReferences: [],
          },
        };
      },
    } as any,
  });

const result =
  await coordinator.review(
    session,
    blocked,
  );

assert(
  assembleCalled === 1,
  "Chief of Staff did not assemble targeted Council",
);

assert(
  runtimeCalled === 1,
  "targeted Council did not run",
);

assert(
  result.status ===
    "revised-recommendation-ready",
  "revised recommendation was not produced",
);

assert(
  result.requiresUserApproval === true,
  "revised recommendation received execution authority",
);

assert(
  result.sourceTaskId === "task-1",
  "original execution linkage was lost",
);

assert(
  result.revisedRecommendation?.id ===
    "recommendation-2",
  "revised recommendation missing",
);

const noBlockerCoordinator =
  new CouncilReplanningCoordinator({
    chiefOfStaff: {} as any,
    runtime: {} as any,
  });

const noBlocker =
  await noBlockerCoordinator.review(
    session,
    {
      ...blocked,
      feedback: {
        kind: "success",
        status: "completed",
        councilSessionId:
          "council-1",
        recommendationId:
          "recommendation-1",
        taskId: "task-1",
        occurredAt: 11,
        message: "done",
        planId: "plan-1",
      },
    },
  );

assert(
  noBlocker.status ===
    "not-required",
  "successful execution incorrectly triggered replanning",
);

console.log(
  "PASS P16 Council Replanning behavior",
);
