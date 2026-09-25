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
    }
  | {
      mode: "pinned";
      providerId: string;
      providerInstanceId: string;
      modelId: string;
      label?: string;
      targetRole?: ArenaSeatRole;
    }
  | {
      mode: "pinned";
      label: string;
      providerId?: never;
      providerInstanceId?: never;
      modelId?: never;
      targetRole?: ArenaSeatRole;
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

export function arenaModelKey(
  model: Pick<AiCenterModelChoice, "providerId" | "providerInstanceId" | "modelId">,
): string {
  return `${model.providerId}:${model.providerInstanceId}:${model.modelId}`;
}
