export type CouncilExecutionLinkage = {
  councilSessionId: string;
  recommendationId: string;
  taskId: string;
  previousTaskId?: string;
};

type ExecutionFeedbackBase = CouncilExecutionLinkage & {
  occurredAt: number;
  message: string;
};

export type ExecutionProgress = ExecutionFeedbackBase & {
  kind: "progress";
  status: "task-created" | "planning" | "awaiting-confirmation" | "running";
  planId?: string;
  approval?: {
    capability: string;
    input: Record<string, unknown>;
  };
};

export type ExecutionSuccess = ExecutionFeedbackBase & {
  kind: "success";
  status: "completed";
  planId: string;
  result?: unknown;
};

export type ExecutionFailure = ExecutionFeedbackBase & {
  kind: "failure";
  status: "failed";
  reason: string;
};

export type ExecutionBlocker = ExecutionFeedbackBase & {
  kind: "blocker";
  status: "blocked";
  reason: string;
  errorKind?: string;
  blockedStep?: string;
  observedReality?: string;
  invalidatedAssumptions: string[];
  suggestedExpertise: string[];
};

export type ExecutionFeedback =
  | ExecutionProgress
  | ExecutionSuccess
  | ExecutionFailure
  | ExecutionBlocker;

export type CouncilExecutionState = {
  linkage: CouncilExecutionLinkage;
  feedback: ExecutionFeedback;
  reconveneDecision?: CouncilReconveneDecision;
};

export type CouncilReconveneDecision = {
  action: "reconvene-experts" | "require-user-decision";
  reason: string;
  existingSeatIds: string[];
  newSeatRequirements: string[];
  executionContext: Pick<
    ExecutionBlocker,
    "councilSessionId" | "recommendationId" | "taskId" | "reason" | "observedReality" | "invalidatedAssumptions"
  >;
  requiresUserApproval: true;
};
