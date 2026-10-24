import type { AiCenterModelChoice } from "../services/aiCenter";

export const ARENA_STRATEGIES = [
  "compare",
  "debate",
  "collaboration",
  "competition",
  "role-play",
  "game",
  "simulation",
] as const;

export type ArenaStrategy = (typeof ARENA_STRATEGIES)[number];

export type ArenaModelConstraint =
  | {
      mode: "auto";
    }
  | {
      mode: "prefer";
      providerId?: string;
      providerInstanceId?: string;
      modelId?: string;
      label?: string;
      targetRole?: ArenaSeatRole;
      allSeats?: boolean;
    }
  | {
      mode: "pinned";
      providerId: string;
      providerInstanceId: string;
      modelId: string;
      label?: string;
      targetRole?: ArenaSeatRole;
      allSeats?: boolean;
    }
  | {
      mode: "pinned";
      label: string;
      providerId?: never;
      providerInstanceId?: never;
      modelId?: never;
      targetRole?: ArenaSeatRole;
      allSeats?: boolean;
    };

export type ArenaSeatRole =
  | "participant"
  | "moderator"
  | "judge"
  | "evaluator"
  | "observer"
  | "rules-controller"
  | "environment"
  | "synthesizer";

export type ArenaSeatRequirement = {
  id: string;
  title: string;
  purpose: string;
  requiredExpertise: string[];
  role: ArenaSeatRole;
  team?: string;
  modelConstraint: ArenaModelConstraint;
  privateStateAccess?: string[];
};

export type ArenaModelAssignment = {
  seatId: string;
  choice: AiCenterModelChoice;
  policy: ArenaModelConstraint["mode"];
  fallbackChoices: AiCenterModelChoice[];
  rationale: string;
};

export type ArenaStrategyPlan = {
  id: string;
  objective: string;
  strategies: ArenaStrategy[];
  rationale: string;
  requiresHiddenState: boolean;
  requiresEvaluation: boolean;
  suggestedMinParticipants: number;
  suggestedMaxParticipants: number;
  userModelConstraints: ArenaModelConstraint[];
};

export type ArenaAssemblyPlan = {
  id: string;
  objective: string;
  strategyPlan: ArenaStrategyPlan;
  seats: ArenaSeatRequirement[];
  modelAssignments: ArenaModelAssignment[];
  rationale: string;
  chiefOfStaff: {
    mode: "llm-chief-of-staff" | "deterministic-fallback";
    providerId?: string;
    providerInstanceId?: string;
    modelId?: string;
    selectionRationale: string;
    fallbackReason?: string;
    attempts: number;
  };
};

export type ArenaPlanRequest = {
  objective: string;
  availableModels: AiCenterModelChoice[];
  userModelConstraints?: ArenaModelConstraint[];
  maxParticipants?: number;
};

export type ArenaSessionStatus =
  | "created"
  | "running"
  | "paused"
  | "completed"
  | "stopped"
  | "cancelled"
  | "failed";

export type ArenaRoundStatus =
  | "pending"
  | "running"
  | "completed"
  | "degraded"
  | "cancelled"
  | "failed";

export type ArenaVisibility =
  | { kind: "public" }
  | { kind: "participant-private"; participantId: string }
  | { kind: "team-private"; teamId: string }
  | { kind: "runtime-private" };

export type ArenaContextEntry = {
  id: string;
  createdAt: number;
  round: number;
  authorId?: string;
  visibility: ArenaVisibility;
  content: string;
  tags?: string[];
};

export type ArenaParticipantRuntime = {
  seatId: string;
  title: string;
  role: ArenaSeatRole;
  team?: string;
  assignedModel: ArenaModelAssignment["choice"];
};

export type ArenaInvocationMetrics = {
  seatId: string;
  round: number;
  providerId: string;
  providerInstanceId: string;
  modelId: string;
  startedAt: number;
  completedAt: number;
  durationMs: number;
  status: "success" | "failed" | "cancelled";
  metadata?: unknown;
  error?: string;
};

export type ArenaRuntimeEvent =
  | {
      type: "session-started";
      sessionId: string;
      at: number;
    }
  | {
      type: "session-status";
      sessionId: string;
      status: ArenaSessionStatus;
      at: number;
    }
  | {
      type: "round-started";
      sessionId: string;
      round: number;
      strategy: ArenaStrategy;
      at: number;
    }
  | {
      type: "round-completed";
      sessionId: string;
      round: number;
      strategy: ArenaStrategy;
      status: ArenaRoundStatus;
      at: number;
    }
  | {
      type: "participant-started";
      sessionId: string;
      round: number;
      seatId: string;
      at: number;
    }
  | {
      type: "participant-completed";
      sessionId: string;
      round: number;
      seatId: string;
      at: number;
    }
  | {
      type: "participant-failed";
      sessionId: string;
      round: number;
      seatId: string;
      error: string;
      at: number;
    }
  | {
      type: "context-added";
      sessionId: string;
      round: number;
      entryId: string;
      visibility: ArenaVisibility;
      at: number;
    };

export type ArenaStrategyAction = {
  id: string;
  round: number;
  strategy: ArenaStrategy;
  actorSeatIds: string[];
  instruction: string;
  outputVisibility:
    | "public"
    | "participant-private"
    | "team-private";
  allowFailure: boolean;
};

export type ArenaStrategyDriverPlan = {
  strategy: ArenaStrategy;
  maxRounds: number;
  actions: ArenaStrategyAction[];
  requiresHiddenState: boolean;
  requiresWorldState: boolean;
};

export type ArenaRuntimeConfig = {
  maxRetriesPerInvocation?: number;
  seed?: number;
  initialPublicContext?: string[];
  participantPrivateContext?: Record<string, string[]>;
  teamPrivateContext?: Record<string, string[]>;
  hiddenRoles?: Record<string, string>;
  worldState?: Record<string, unknown>;
};

export type ArenaRuntimeResult = {
  sessionId: string;
  status: ArenaSessionStatus;
  strategy: ArenaStrategy;
  startedAt: number;
  completedAt: number;
  roundsCompleted: number;
  contexts: ArenaContextEntry[];
  metrics: ArenaInvocationMetrics[];
  events: ArenaRuntimeEvent[];
  degraded: boolean;
  errors: string[];
  outcome: import("./arenaGovernance").ArenaOutcome;
};


export function arenaModelKey(
  model: Pick<AiCenterModelChoice, "providerId" | "providerInstanceId" | "modelId">,
): string {
  return `${model.providerId}:${model.providerInstanceId}:${model.modelId}`;
}
