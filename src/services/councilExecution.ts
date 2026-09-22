import {
  createCouncilRecommendationTask,
  recommendationFromCouncilSession,
} from "./councilTaskHandoff";
import {
  executeChatWorkTask,
  extractErrorKind,
  parseWorkTaskApproval,
  type ExecuteWorkTaskOptions,
  type ExecuteWorkTaskResponse,
  type SubmitChatTaskResponse,
} from "./tasks";
import type { CouncilSession } from "../types/council";
import type {
  CouncilExecutionLinkage,
  CouncilExecutionState,
  ExecutionBlocker,
  ExecutionFeedback,
} from "../types/councilExecution";

type CouncilExecutionDependencies = {
  createTask: (session: CouncilSession) => Promise<SubmitChatTaskResponse>;
  executeTask: (
    taskId: string,
    agentId: string,
    options?: ExecuteWorkTaskOptions,
  ) => Promise<ExecuteWorkTaskResponse>;
  now: () => number;
};

const BLOCKER_KINDS = new Set([
  "AuthenticationRequired",
  "ConnectionUnavailable",
  "NoViableExecutionPath",
  "PairingRequired",
]);

function detail(error: unknown): string {
  return typeof error === "string"
    ? error
    : error instanceof Error
      ? error.message
      : String(error);
}

export class CouncilExecutionCoordinator {
  constructor(
    private readonly dependencies: CouncilExecutionDependencies = {
      createTask: createCouncilRecommendationTask,
      executeTask: executeChatWorkTask,
      now: Date.now,
    },
  ) {}

  async approve(
    session: CouncilSession,
    onFeedback?: (feedback: ExecutionFeedback) => void,
  ): Promise<CouncilExecutionState> {
    const recommendation = recommendationFromCouncilSession(session);
    const task = await this.dependencies.createTask(session);
    const linkage: CouncilExecutionLinkage = {
      councilSessionId: session.id,
      recommendationId: recommendation.id,
      taskId: task.taskId,
    };
    const feedback: ExecutionFeedback = {
      ...linkage,
      kind: "progress",
      status: "planning",
      occurredAt: this.dependencies.now(),
      message: "Approved recommendation created a DO Task for Planner.",
    };
    onFeedback?.(feedback);
    return { linkage, feedback };
  }

  async execute(
    session: CouncilSession,
    state: CouncilExecutionState,
    approval?: ExecuteWorkTaskOptions,
    onFeedback?: (feedback: ExecutionFeedback) => void,
  ): Promise<CouncilExecutionState> {
    let linkage = state.linkage;
    try {
      if (approval?.userConfirmed) {
        const retry = await this.dependencies.createTask(session);
        linkage = {
          ...linkage,
          previousTaskId: linkage.taskId,
          taskId: retry.taskId,
        };
      }
      const running: ExecutionFeedback = {
        ...linkage,
        kind: "progress",
        status: "running",
        occurredAt: this.dependencies.now(),
        message: "Task Engine is executing the Planner output through OpenClaw.",
      };
      onFeedback?.(running);
      const result = await this.dependencies.executeTask(
        linkage.taskId,
        "openclaw",
        approval ?? {},
      );
      const feedback: ExecutionFeedback = {
        ...linkage,
        kind: "success",
        status: "completed",
        planId: result.planId,
        result: result.output,
        occurredAt: this.dependencies.now(),
        message: "Agent execution completed through the normal Runtime path.",
      };
      onFeedback?.(feedback);
      return { linkage, feedback };
    } catch (error) {
      const requestedApproval = parseWorkTaskApproval(error);
      if (requestedApproval) {
        const feedback: ExecutionFeedback = {
          ...linkage,
          kind: "progress",
          status: "awaiting-confirmation",
          approval: requestedApproval,
          occurredAt: this.dependencies.now(),
          message: `OpenClaw selected ${requestedApproval.capability}; exact input confirmation is required.`,
        };
        onFeedback?.(feedback);
        return { linkage, feedback };
      }
      const reason = detail(error);
      const errorKind = extractErrorKind(reason) ?? undefined;
      const feedback: ExecutionFeedback = errorKind && BLOCKER_KINDS.has(errorKind)
        ? {
            ...linkage,
            kind: "blocker",
            status: "blocked",
            reason,
            errorKind,
            observedReality: reason,
            invalidatedAssumptions: [],
            suggestedExpertise: [],
            occurredAt: this.dependencies.now(),
            message: "Execution reached a blocker; Chief of Staff review is available.",
          } satisfies ExecutionBlocker
        : {
            ...linkage,
            kind: "failure",
            status: "failed",
            reason,
            occurredAt: this.dependencies.now(),
            message: "Agent execution failed.",
          };
      onFeedback?.(feedback);
      return { linkage, feedback };
    }
  }
}

export function createCouncilExecutionCoordinator(): CouncilExecutionCoordinator {
  return new CouncilExecutionCoordinator();
}
