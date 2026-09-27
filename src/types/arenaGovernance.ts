import type {
  ArenaSessionStatus,
  ArenaStrategy,
} from "./arena";

export type ArenaCompletionReason =
  | "winner"
  | "consensus"
  | "rule-complete"
  | "max-rounds"
  | "stopped"
  | "cancelled"
  | "failed";

export type ArenaTiePolicy =
  | "preserve"
  | "seeded";

export type ArenaScore = {
  scope: "participant" | "team" | "strategy";
  evaluatorSeatId: string;
  subjectId: string;
  criterionId: string;
  value: number;
  rationale?: string;
};

export type ArenaVote = {
  scope: "participant" | "team" | "strategy";
  evaluatorSeatId: string;
  targetId: string;
  weight: number;
};

export type ArenaEvaluationProvenance = {
  evaluatorSeatId: string;
  evaluationContextId: string;
  sourceContextIds: string[];
  providerId?: string;
  providerInstanceId?: string;
  modelId?: string;
};

export type ArenaGovernanceInput = {
  strategy: ArenaStrategy;
  seed: number;
  runtimeStatus: ArenaSessionStatus;
  round: number;
  maxRounds: number;
  degraded: boolean;
  explicitWinnerIds?: string[];
  explicitWinnerScope?: "participant" | "team";
  consensusReached?: boolean;
  ruleComplete?: boolean;
  tiePolicy?: ArenaTiePolicy;
  scores?: ArenaScore[];
  votes?: ArenaVote[];
  provenance?: ArenaEvaluationProvenance[];
  rationale?: string[];
};

export type ArenaOutcome = {
  strategy: ArenaStrategy;
  seed: number;
  status:
    | "completed"
    | "degraded"
    | "no-consensus"
    | "stopped"
    | "cancelled"
    | "failed";
  completionReason: ArenaCompletionReason;
  winnerIds: string[];
  winnerScope?: "participant" | "team";
  scores: ArenaScore[];
  votes: ArenaVote[];
  provenance: ArenaEvaluationProvenance[];
  rationale: string[];
};
