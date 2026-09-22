import type { CouncilSession } from "../types/council";
import type {
  CouncilAssemblyPlan,
  CouncilRecommendation,
} from "../types/councilAssembly";
import type {
  CouncilExecutionState,
  CouncilReconveneDecision,
  ExecutionBlocker,
} from "../types/councilExecution";
import {
  CouncilChiefOfStaff,
  createCouncilChiefOfStaff,
} from "./councilChiefOfStaff";
import {
  CouncilRuntime,
  createCouncilRuntime,
} from "./councilRuntime";

export type CouncilReplanningStatus =
  | "not-required"
  | "user-decision-required"
  | "reconvening"
  | "revised-recommendation-ready";

export type CouncilReplanningResult = {
  status: CouncilReplanningStatus;
  decision?: CouncilReconveneDecision;
  sourceSessionId: string;
  sourceRecommendationId: string;
  sourceTaskId: string;
  revisedSession?: CouncilSession;
  revisedRecommendation?: CouncilRecommendation;
  requiresUserApproval: true;
};

export type CouncilReplanningDependencies = {
  chiefOfStaff: CouncilChiefOfStaff;
  runtime: CouncilRuntime;
};

function blockerFromState(state: CouncilExecutionState): ExecutionBlocker | undefined {
  return state.feedback.kind === "blocker"
    ? state.feedback
    : undefined;
}

function replanningObjective(
  session: CouncilSession,
  blocker: ExecutionBlocker,
  decision: CouncilReconveneDecision,
): string {
  const expertise = [
    ...decision.existingSeatIds,
    ...decision.newSeatRequirements,
  ];

  return [
    "Re-evaluate an AI Council recommendation because real execution encountered a blocker.",
    "",
    `Original objective: ${session.prompt}`,
    `Execution blocker: ${blocker.reason}`,
    blocker.observedReality
      ? `Observed reality: ${blocker.observedReality}`
      : "",
    blocker.invalidatedAssumptions.length
      ? `Invalidated assumptions: ${blocker.invalidatedAssumptions.join("; ")}`
      : "",
    expertise.length
      ? `Target expertise for reconvening: ${expertise.join("; ")}`
      : "",
    "",
    "Produce a revised recommendation only.",
    "Do not claim execution has resumed.",
    "Do not grant execution permission.",
    "Preserve unresolved uncertainty.",
    "State what changed from the previous recommendation.",
  ]
    .filter(Boolean)
    .join("\n");
}

function targetedSeatMatch(
  plan: CouncilAssemblyPlan,
  decision: CouncilReconveneDecision,
): CouncilAssemblyPlan {
  const requested = decision.newSeatRequirements
    .map((item) => item.trim().toLowerCase())
    .filter(Boolean);

  if (
    !decision.existingSeatIds.length &&
    !requested.length
  ) {
    return plan;
  }

  const synthesizer = plan.seats.find(
    (seat) => seat.id === plan.synthesizerSeatId,
  );

  const targeted = plan.seats.filter((seat) => {
    if (seat.id === plan.synthesizerSeatId) return false;

    if (decision.existingSeatIds.includes(seat.id)) {
      return true;
    }

    if (!requested.length) return false;

    const haystack = [
      seat.title,
      seat.purpose,
      ...seat.requiredExpertise,
    ]
      .join(" ")
      .toLowerCase();

    return requested.some((expertise) =>
      haystack.includes(expertise),
    );
  });

  const boundedExperts =
    targeted.length > 0
      ? targeted.slice(0, 4)
      : plan.seats
          .filter(
            (seat) =>
              seat.id !== plan.synthesizerSeatId,
          )
          .slice(0, 2);

  const seats = [
    ...boundedExperts,
    ...(synthesizer ? [synthesizer] : []),
  ];

  const synthesizerSeatId =
    synthesizer?.id ??
    seats.at(-1)?.id ??
    plan.synthesizerSeatId;

  const expertIds = seats
    .filter(
      (seat) =>
        seat.id !== synthesizerSeatId,
    )
    .map((seat) => seat.id);

  return {
    ...plan,
    id: `${plan.id}-replan`,
    rationale: [
      plan.rationale,
      "This Council was narrowed to expertise relevant to the execution blocker.",
    ].join(" "),
    seats,
    synthesizerSeatId,
    deliberation: {
      maxRounds: Math.min(
        2,
        Math.max(
          1,
          plan.deliberation.maxRounds,
        ),
      ),
      stages: [
        {
          id: "independent-analysis",
          title: "Targeted blocker analysis",
          participantSeatIds: expertIds,
        },
        {
          id: "cross-review",
          title: "Cross-review revised assumptions",
          participantSeatIds: expertIds,
        },
        {
          id: "final-synthesis",
          title: "Revised recommendation synthesis",
          participantSeatIds: [
            synthesizerSeatId,
          ],
        },
      ],
    },
    provenance: {
      ...plan.provenance,
      references: [
        ...new Set([
          ...plan.provenance.references,
          "execution-feedback:blocker",
        ]),
      ],
    },
  };
}

export class CouncilReplanningCoordinator {
  constructor(
    private readonly dependencies: CouncilReplanningDependencies = {
      chiefOfStaff:
        createCouncilChiefOfStaff(),
      runtime: createCouncilRuntime(),
    },
  ) {}

  async review(
    session: CouncilSession,
    execution: CouncilExecutionState,
  ): Promise<CouncilReplanningResult> {
    const blocker = blockerFromState(execution);

    if (!blocker) {
      return {
        status: "not-required",
        sourceSessionId:
          execution.linkage.councilSessionId,
        sourceRecommendationId:
          execution.linkage.recommendationId,
        sourceTaskId:
          execution.linkage.taskId,
        requiresUserApproval: true,
      };
    }

    const decision =
      this.dependencies.chiefOfStaff
        .evaluateExecutionBlocker(
          blocker,
          session.assemblyPlan,
        );

    if (
      decision.action ===
      "require-user-decision"
    ) {
      return {
        status: "user-decision-required",
        decision,
        sourceSessionId:
          blocker.councilSessionId,
        sourceRecommendationId:
          blocker.recommendationId,
        sourceTaskId: blocker.taskId,
        requiresUserApproval: true,
      };
    }

    const objective =
      replanningObjective(
        session,
        blocker,
        decision,
      );

    const freshPlan =
      await this.dependencies.chiefOfStaff
        .assemble({
          objective,
          maxSeats: 5,
        });

    const targetedPlan =
      targetedSeatMatch(
        freshPlan,
        decision,
      );

    const result =
      await this.dependencies.runtime.run({
        prompt: objective,
        members: [],
        assemblyPlan: targetedPlan,
      });

    return {
      status:
        "revised-recommendation-ready",
      decision,
      sourceSessionId:
        blocker.councilSessionId,
      sourceRecommendationId:
        blocker.recommendationId,
      sourceTaskId: blocker.taskId,
      revisedSession: result.session,
      revisedRecommendation:
        result.recommendation,
      requiresUserApproval: true,
    };
  }
}

export function createCouncilReplanningCoordinator(): CouncilReplanningCoordinator {
  return new CouncilReplanningCoordinator();
}
