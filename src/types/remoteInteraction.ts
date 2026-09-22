import type { CouncilSession } from "./council";
import type { CouncilExecutionState } from "./councilExecution";

export type RemoteMode = "auto" | "council" | "simulation";

export type RemoteInboundEvent =
  | {
      type: "inbound_message";
      sessionKey: string;
      messageId: string;
      text: string;
      mode?: RemoteMode;
      profileId?: string;
    }
  | {
      type: "recommendation_response";
      sessionKey: string;
      councilSessionId: string;
      recommendationId: string;
      approved: boolean;
    }
  | {
      type: "permission_response" | "danger_confirm";
      sessionKey: string;
      confirmationId: string;
      taskId: string;
      approved: boolean;
    }
  | {
      type: "stop_turn";
      sessionKey: string;
    };

export type RemoteEventType =
  | "session_ready"
  | "turn_start"
  | "assistant_chunk"
  | "council_assembling"
  | "council_recommendation"
  | "recommendation_approval_required"
  | "recommendation_rejected"
  | "task_created"
  | "planning"
  | "execution_confirmation_required"
  | "running"
  | "progress"
  | "blocked"
  | "revised_recommendation"
  | "completed"
  | "failed"
  | "cancelled"
  | "error"
  | "turn_end";

export type RemoteAiOsEvent = {
  type: RemoteEventType;
  sessionKey: string;
  conversationId: string;
  occurredAt: number;
  message?: string;
  councilSessionId?: string;
  recommendationId?: string;
  taskId?: string;
  confirmationId?: string;
  recommendation?: {
    summary: string;
    proposedActions: string[];
    assumptions: string[];
    risks: string[];
    requiresApproval: true;
  };
  confirmation?: {
    action: string;
    targetSummary?: string;
    reason: string;
    options: ["approve", "reject"];
  };
  result?: unknown;
};

export type RemoteSessionMapping = {
  sessionKey: string;
  conversationId: string;
  councilSessionId?: string;
  recommendationId?: string;
  taskId?: string;
  updatedAt: number;
};

export type RemoteSessionState = {
  mapping: RemoteSessionMapping;
  councilSession?: CouncilSession;
  execution?: CouncilExecutionState;
};
