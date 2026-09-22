import {
  streamThroughAiCenter,
  type AiCenterStream,
} from "./aiCenter";
import { classifyAutomaticIntent } from "./intentRouting";
import {
  completeChatTaskExecution,
  executeChatWorkTask,
  failChatTaskExecution,
  parseWorkTaskApproval,
  startChatTaskExecution,
  submitChatTask,
  type ExecuteWorkTaskOptions,
  type WorkTaskApproval,
} from "./tasks";
import {
  createConversation,
  getConversation,
  saveConversation,
  buildConversationContext,
} from "./conversations";
import { createCouncilChiefOfStaff } from "./councilChiefOfStaff";
import { createCouncilRuntime } from "./councilRuntime";
import { createCouncilExecutionCoordinator } from "./councilExecution";
import { createCouncilReplanningCoordinator } from "./councilReplanning";
import { createSimulationCouncilService } from "./councilSimulation";
import { loadCouncilSessions, upsertCouncilSession } from "./council";
import type { CouncilSession } from "../types/council";
import type { CouncilRunResult } from "../types/councilRuntime";
import type {
  RemoteAiOsEvent,
  RemoteInboundEvent,
  RemoteSessionMapping,
  RemoteSessionState,
} from "../types/remoteInteraction";

const REMOTE_MAPPINGS_KEY = "ai-os.remote.linco.sessions.v1";

type AskOperation = {
  result: Promise<{ text: string; cancelled: boolean }>;
  cancel: () => Promise<void>;
};

export type RemoteSessionPersistence = {
  load: () => RemoteSessionMapping[];
  save: (mapping: RemoteSessionMapping) => void;
};

export type RemoteAiOsDependencies = {
  persistence: RemoteSessionPersistence;
  createConversation: () => string;
  hasConversation: (id: string) => boolean;
  appendConversationMessage: (
    id: string,
    role: "user" | "assistant",
    content: string,
  ) => void;
  loadCouncilSession: (id: string) => CouncilSession | undefined;
  saveCouncilSession: (session: CouncilSession) => void;
  classifyIntent: typeof classifyAutomaticIntent;
  startAsk: (
    conversationId: string,
    text: string,
    onChunk: (chunk: string) => void,
  ) => AskOperation;
  submitTask: typeof submitChatTask;
  executeTask: typeof executeChatWorkTask;
  runCouncil: (
    text: string,
    onProgress: (message: string) => void,
  ) => Promise<CouncilRunResult>;
  cancelCouncil: () => Promise<void>;
  runSimulation: (
    text: string,
    profileId: string,
    onProgress: (message: string) => void,
  ) => Promise<CouncilRunResult>;
  cancelSimulation: () => Promise<void>;
  councilExecution: Pick<
    ReturnType<typeof createCouncilExecutionCoordinator>,
    "approve" | "execute"
  >;
  replanning: Pick<
    ReturnType<typeof createCouncilReplanningCoordinator>,
    "review"
  >;
  uuid: () => string;
  now: () => number;
};

type PendingConfirmation = {
  id: string;
  taskId: string;
  approval: WorkTaskApproval;
  kind: "ordinary" | "council";
  prompt: string;
  councilSession?: CouncilSession;
};

type InternalSession = RemoteSessionState & {
  seenMessageIds: Set<string>;
  pending?: PendingConfirmation;
  activeCancel?: () => Promise<void>;
  ordinaryPrompt?: string;
  consumedRecommendationId?: string;
};

function browserPersistence(): RemoteSessionPersistence {
  const read = (): RemoteSessionMapping[] => {
    if (typeof localStorage === "undefined") return [];
    try {
      const value: unknown = JSON.parse(localStorage.getItem(REMOTE_MAPPINGS_KEY) ?? "[]");
      return Array.isArray(value)
        ? value.filter((item): item is RemoteSessionMapping =>
            Boolean(
              item &&
              typeof item === "object" &&
              typeof (item as RemoteSessionMapping).sessionKey === "string" &&
              typeof (item as RemoteSessionMapping).conversationId === "string",
            ),
          )
        : [];
    } catch {
      return [];
    }
  };
  return {
    load: read,
    save: (mapping) => {
      if (typeof localStorage === "undefined") return;
      const next = [
        mapping,
        ...read().filter((item) => item.sessionKey !== mapping.sessionKey),
      ].slice(0, 100);
      localStorage.setItem(REMOTE_MAPPINGS_KEY, JSON.stringify(next));
    },
  };
}

function defaultAsk(
  conversationId: string,
  text: string,
  onChunk: (chunk: string) => void,
): AskOperation {
  let taskId: string | undefined;
  let stream: AiCenterStream | undefined;
  let resolveCancellation: (() => void) | undefined;
  const cancellation = new Promise<void>((resolve) => {
    resolveCancellation = resolve;
  });
  const result = (async () => {
    const task = await submitChatTask(text, "ASK", {
      source: "linco-remote",
      remoteConversationId: conversationId,
    });
    taskId = task.taskId;
    await startChatTaskExecution(task.taskId);
    const conversation = getConversation(conversationId);
    const messages = conversation
      ? buildConversationContext(conversation).messages
      : [{ role: "user" as const, content: text }];
    stream = streamThroughAiCenter(messages, undefined, onChunk);
    const outcome = await Promise.race([
      stream.result.then((answer) => ({ type: "answer" as const, answer })),
      cancellation.then(() => ({ type: "cancelled" as const })),
    ]);
    if (outcome.type === "cancelled") {
      await failChatTaskExecution(task.taskId, "Cancelled by remote user");
      return { text: "", cancelled: true };
    }
    const answer = outcome.answer;
    if (answer.cancelled) {
      await failChatTaskExecution(task.taskId, "Cancelled by remote user");
      return { text: answer.response.text, cancelled: true };
    }
    await completeChatTaskExecution(task.taskId, {
      providerId: answer.response.providerId,
      modelId: answer.response.modelId,
      text: answer.response.text,
    });
    return { text: answer.response.text, cancelled: false };
  })();
  return {
    result,
    cancel: async () => {
      resolveCancellation?.();
      await stream?.cancel().catch(() => undefined);
      if (taskId && !stream) {
        await failChatTaskExecution(taskId, "Cancelled by remote user");
      }
    },
  };
}

function defaultDependencies(): RemoteAiOsDependencies {
  const councilRuntime = createCouncilRuntime();
  const chiefOfStaff = createCouncilChiefOfStaff();
  const simulation = createSimulationCouncilService();
  return {
    persistence: browserPersistence(),
    createConversation: () => createConversation().id,
    hasConversation: (id) => Boolean(getConversation(id)),
    appendConversationMessage: (id, role, content) => {
      const conversation = getConversation(id);
      if (!conversation) return;
      saveConversation({
        ...conversation,
        title:
          conversation.messages.length === 0
            ? content.slice(0, 48)
            : conversation.title,
        messages: [
          ...conversation.messages,
          {
            id: crypto.randomUUID(),
            role,
            content,
            createdAt: new Date().toISOString(),
          },
        ],
      });
    },
    loadCouncilSession: (id) =>
      loadCouncilSessions().find((session) => session.id === id),
    saveCouncilSession: (session) => {
      upsertCouncilSession(session);
    },
    classifyIntent: classifyAutomaticIntent,
    startAsk: defaultAsk,
    submitTask: submitChatTask,
    executeTask: executeChatWorkTask,
    runCouncil: async (text, onProgress) => {
      const plan = await chiefOfStaff.assemble({ objective: text });
      return councilRuntime.run({
        prompt: text,
        members: [],
        assemblyPlan: plan,
        callbacks: {
          onCouncilStarted: () => onProgress("Council assembled."),
          onMemberStarted: (step) => onProgress(`${step.memberName} started.`),
          onMemberCompleted: (step) => onProgress(`${step.memberName} completed.`),
        },
      });
    },
    cancelCouncil: () => councilRuntime.cancel(),
    runSimulation: async (text, profileId, onProgress) =>
      simulation.run({
        objective: text,
        profileId,
        callbacks: {
          onCouncilStarted: () => onProgress("Simulation Council assembled."),
          onMemberStarted: (step) => onProgress(`${step.memberName} started.`),
        },
      }),
    cancelSimulation: () => simulation.cancel(),
    councilExecution: createCouncilExecutionCoordinator(),
    replanning: createCouncilReplanningCoordinator(),
    uuid: () => crypto.randomUUID(),
    now: Date.now,
  };
}

function errorDetail(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function safeTargetSummary(input: Record<string, unknown>): string | undefined {
  const allowed = ["path", "source", "destination", "model", "protocol"];
  const parts = allowed.flatMap((key) => {
    const value = input[key];
    if (typeof value !== "string" || !value.trim()) return [];
    const clean = value.replace(/([?&](?:token|key|secret|password)=)[^&]+/gi, "$1[redacted]");
    return [`${key}: ${clean.slice(0, 160)}`];
  });
  return parts.length ? parts.join("; ") : undefined;
}

export class RemoteAiOsGateway {
  private readonly sessions = new Map<string, InternalSession>();
  private readonly dependencies: RemoteAiOsDependencies;

  constructor(dependencies?: RemoteAiOsDependencies) {
    this.dependencies = dependencies ?? defaultDependencies();
  }

  private event(
    session: InternalSession,
    type: RemoteAiOsEvent["type"],
    fields: Omit<RemoteAiOsEvent, "type" | "sessionKey" | "conversationId" | "occurredAt"> = {},
  ): RemoteAiOsEvent {
    return {
      type,
      sessionKey: session.mapping.sessionKey,
      conversationId: session.mapping.conversationId,
      occurredAt: this.dependencies.now(),
      ...fields,
    };
  }

  private persist(session: InternalSession): void {
    session.mapping.updatedAt = this.dependencies.now();
    this.dependencies.persistence.save({ ...session.mapping });
  }

  private resolveSession(sessionKey: string): InternalSession {
    const key = sessionKey.trim();
    if (!key) throw new Error("Remote sessionKey is required.");
    const current = this.sessions.get(key);
    if (current) return current;
    const saved = this.dependencies.persistence.load().find((item) => item.sessionKey === key);
    const conversationId =
      saved && this.dependencies.hasConversation(saved.conversationId)
        ? saved.conversationId
        : this.dependencies.createConversation();
    const session: InternalSession = {
      mapping: {
        sessionKey: key,
        conversationId,
        councilSessionId: saved?.councilSessionId,
        recommendationId: saved?.recommendationId,
        taskId: saved?.taskId,
        updatedAt: this.dependencies.now(),
      },
      seenMessageIds: new Set(),
      councilSession: saved?.councilSessionId
        ? this.dependencies.loadCouncilSession(saved.councilSessionId)
        : undefined,
    };
    this.sessions.set(key, session);
    this.persist(session);
    return session;
  }

  getSession(sessionKey: string): RemoteSessionState {
    const session = this.resolveSession(sessionKey);
    return {
      mapping: { ...session.mapping },
      councilSession: session.councilSession,
      execution: session.execution,
    };
  }

  private recommendationEvents(
    session: InternalSession,
    council: CouncilSession,
  ): RemoteAiOsEvent[] {
    const recommendation = council.recommendation;
    if (!recommendation) {
      return [this.event(session, "failed", { message: "Council produced no structured recommendation." })];
    }
    session.councilSession = council;
    session.consumedRecommendationId = undefined;
    session.mapping.councilSessionId = council.id;
    session.mapping.recommendationId = recommendation.id;
    this.dependencies.saveCouncilSession(council);
    this.persist(session);
    const state = {
      summary: recommendation.summary,
      proposedActions: recommendation.recommendedPlan,
      assumptions: recommendation.assumptions,
      risks: recommendation.risks,
      requiresApproval: true as const,
    };
    return [
      this.event(session, "council_recommendation", {
        councilSessionId: council.id,
        recommendationId: recommendation.id,
        recommendation: state,
      }),
      this.event(session, "recommendation_approval_required", {
        councilSessionId: council.id,
        recommendationId: recommendation.id,
        recommendation: state,
      }),
    ];
  }

  private confirmationEvent(
    session: InternalSession,
    taskId: string,
    approval: WorkTaskApproval,
    pending: Omit<PendingConfirmation, "id" | "taskId" | "approval">,
  ): RemoteAiOsEvent {
    const id = this.dependencies.uuid();
    session.pending = { id, taskId, approval, ...pending };
    session.mapping.taskId = taskId;
    this.persist(session);
    return this.event(session, "execution_confirmation_required", {
      taskId,
      confirmationId: id,
      councilSessionId: session.mapping.councilSessionId,
      recommendationId: session.mapping.recommendationId,
      confirmation: {
        action: approval.capability,
        targetSummary: safeTargetSummary(approval.input),
        reason: "AI-OS Runtime requires confirmation for this exact Agent-selected action.",
        options: ["approve", "reject"],
      },
    });
  }

  private async runOrdinaryDo(
    session: InternalSession,
    text: string,
  ): Promise<RemoteAiOsEvent[]> {
    const task = await this.dependencies.submitTask(text, "DO", {
      source: "linco-remote",
      remoteConversationId: session.mapping.conversationId,
    });
    session.mapping.taskId = task.taskId;
    session.ordinaryPrompt = text;
    this.persist(session);
    const events = [
      this.event(session, "task_created", { taskId: task.taskId }),
      this.event(session, "planning", { taskId: task.taskId }),
    ];
    try {
      events.push(this.event(session, "running", { taskId: task.taskId }));
      const result = await this.dependencies.executeTask(task.taskId);
      events.push(this.event(session, "completed", {
        taskId: result.taskId,
        result: result.output,
      }));
    } catch (error) {
      const approval = parseWorkTaskApproval(error);
      if (approval) {
        events.push(this.confirmationEvent(session, task.taskId, approval, {
          kind: "ordinary",
          prompt: text,
        }));
      } else {
        events.push(this.event(session, "failed", {
          taskId: task.taskId,
          message: errorDetail(error),
        }));
      }
    }
    return events;
  }

  private async runCouncilExecution(
    session: InternalSession,
    council: CouncilSession,
  ): Promise<RemoteAiOsEvent[]> {
    const approved = await this.dependencies.councilExecution.approve(council);
    session.execution = approved;
    session.mapping.taskId = approved.linkage.taskId;
    this.persist(session);
    const events = [
      this.event(session, "task_created", {
        councilSessionId: approved.linkage.councilSessionId,
        recommendationId: approved.linkage.recommendationId,
        taskId: approved.linkage.taskId,
      }),
      this.event(session, "planning", { taskId: approved.linkage.taskId }),
    ];
    const execution = await this.dependencies.councilExecution.execute(council, approved);
    session.execution = execution;
    session.mapping.taskId = execution.linkage.taskId;
    this.persist(session);
    if (
      execution.feedback.kind === "progress" &&
      execution.feedback.status === "awaiting-confirmation" &&
      execution.feedback.approval
    ) {
      events.push(this.confirmationEvent(
        session,
        execution.linkage.taskId,
        execution.feedback.approval,
        { kind: "council", prompt: council.prompt, councilSession: council },
      ));
      return events;
    }
    events.push(...await this.executionTerminalEvents(session, council, execution));
    return events;
  }

  private async executionTerminalEvents(
    session: InternalSession,
    council: CouncilSession,
    execution: NonNullable<RemoteSessionState["execution"]>,
  ): Promise<RemoteAiOsEvent[]> {
    const feedback = execution.feedback;
    if (feedback.kind === "success") {
      return [this.event(session, "completed", {
        councilSessionId: feedback.councilSessionId,
        recommendationId: feedback.recommendationId,
        taskId: feedback.taskId,
        result: feedback.result,
      })];
    }
    if (feedback.kind === "blocker") {
      const events = [this.event(session, "blocked", {
        councilSessionId: feedback.councilSessionId,
        recommendationId: feedback.recommendationId,
        taskId: feedback.taskId,
        message: feedback.reason,
      })];
      const replan = await this.dependencies.replanning.review(council, execution);
      if (replan.revisedSession?.recommendation) {
        session.councilSession = replan.revisedSession;
        session.consumedRecommendationId = undefined;
        session.execution = undefined;
        session.mapping.councilSessionId = replan.revisedSession.id;
        session.mapping.recommendationId = replan.revisedSession.recommendation.id;
        this.dependencies.saveCouncilSession(replan.revisedSession);
        this.persist(session);
        events.push(this.event(session, "revised_recommendation", {
          councilSessionId: replan.revisedSession.id,
          recommendationId: replan.revisedSession.recommendation.id,
          recommendation: {
            summary: replan.revisedSession.recommendation.summary,
            proposedActions: replan.revisedSession.recommendation.recommendedPlan,
            assumptions: replan.revisedSession.recommendation.assumptions,
            risks: replan.revisedSession.recommendation.risks,
            requiresApproval: true,
          },
        }));
      }
      return events;
    }
    return [this.event(session, "failed", {
      taskId: feedback.taskId,
      message: feedback.kind === "failure" ? feedback.reason : feedback.message,
    })];
  }

  private async handleMessage(
    session: InternalSession,
    event: Extract<RemoteInboundEvent, { type: "inbound_message" }>,
  ): Promise<RemoteAiOsEvent[]> {
    if (session.seenMessageIds.has(event.messageId)) {
      return [this.event(session, "error", { message: "Remote message was already consumed." })];
    }
    session.seenMessageIds.add(event.messageId);
    this.dependencies.appendConversationMessage(
      session.mapping.conversationId,
      "user",
      event.text,
    );
    const events = [this.event(session, "turn_start")];

    if (event.mode === "council") {
      events.push(this.event(session, "council_assembling"));
      session.activeCancel = this.dependencies.cancelCouncil;
      try {
        const result = await this.dependencies.runCouncil(event.text, (message) => {
          events.push(this.event(session, "progress", { message }));
        });
        events.push(...this.recommendationEvents(session, result.session));
        this.dependencies.appendConversationMessage(
          session.mapping.conversationId,
          "assistant",
          result.finalAnswer,
        );
      } finally {
        session.activeCancel = undefined;
      }
    } else if (event.mode === "simulation") {
      if (!event.profileId) {
        events.push(this.event(session, "error", { message: "Simulation profileId is required." }));
      } else {
        events.push(this.event(session, "council_assembling"));
        session.activeCancel = this.dependencies.cancelSimulation;
        try {
          const result = await this.dependencies.runSimulation(
            event.text,
            event.profileId,
            (message) => events.push(this.event(session, "progress", { message })),
          );
          this.dependencies.saveCouncilSession(result.session);
          events.push(this.event(session, "completed", {
            councilSessionId: result.session.id,
            recommendationId: result.recommendation?.id,
            result: {
              report: result.finalAnswer,
              recommendation: result.recommendation,
              simulation: result.session.assemblyPlan?.simulation,
            },
          }));
          this.dependencies.appendConversationMessage(
            session.mapping.conversationId,
            "assistant",
            result.finalAnswer,
          );
        } finally {
          session.activeCancel = undefined;
        }
      }
    } else {
      const intent = await this.dependencies.classifyIntent(event.text);
      if (intent.taskType === "DO") {
        events.push(...await this.runOrdinaryDo(session, event.text));
      } else {
        let assistantText = "";
        const operation = this.dependencies.startAsk(
          session.mapping.conversationId,
          event.text,
          (chunk) => {
            assistantText += chunk;
            events.push(this.event(session, "assistant_chunk", { message: chunk }));
          },
        );
        session.activeCancel = operation.cancel;
        try {
          const result = await operation.result;
          assistantText = result.text || assistantText;
          events.push(this.event(session, result.cancelled ? "cancelled" : "completed", {
            message: assistantText,
          }));
          if (assistantText) {
            this.dependencies.appendConversationMessage(
              session.mapping.conversationId,
              "assistant",
              assistantText,
            );
          }
        } finally {
          session.activeCancel = undefined;
        }
      }
    }
    events.push(this.event(session, "turn_end"));
    return events;
  }

  private async handleRecommendationResponse(
    session: InternalSession,
    event: Extract<RemoteInboundEvent, { type: "recommendation_response" }>,
  ): Promise<RemoteAiOsEvent[]> {
    const council = session.councilSession;
    if (
      !council?.recommendation ||
      council.id !== event.councilSessionId ||
      council.recommendation.id !== event.recommendationId ||
      session.consumedRecommendationId === event.recommendationId
    ) {
      return [this.event(session, "error", { message: "No matching pending Council recommendation." })];
    }
    if (!event.approved) {
      session.consumedRecommendationId = event.recommendationId;
      return [this.event(session, "recommendation_rejected", {
        councilSessionId: council.id,
        recommendationId: council.recommendation.id,
      })];
    }
    session.consumedRecommendationId = event.recommendationId;
    return this.runCouncilExecution(session, council);
  }

  private async handleConfirmation(
    session: InternalSession,
    event: Extract<RemoteInboundEvent, { type: "permission_response" | "danger_confirm" }>,
  ): Promise<RemoteAiOsEvent[]> {
    const pending = session.pending;
    if (
      !pending ||
      pending.id !== event.confirmationId ||
      pending.taskId !== event.taskId
    ) {
      return [this.event(session, "error", { message: "No matching pending execution confirmation." })];
    }
    session.pending = undefined;
    if (!event.approved) {
      return [this.event(session, "cancelled", {
        taskId: pending.taskId,
        message: "Execution confirmation rejected.",
      })];
    }
    const exact: ExecuteWorkTaskOptions = {
      capability: pending.approval.capability,
      input: pending.approval.input,
      userConfirmed: true,
    };
    if (pending.kind === "council" && pending.councilSession && session.execution) {
      const execution = await this.dependencies.councilExecution.execute(
        pending.councilSession,
        session.execution,
        exact,
      );
      session.execution = execution;
      session.mapping.taskId = execution.linkage.taskId;
      this.persist(session);
      return this.executionTerminalEvents(session, pending.councilSession, execution);
    }
    try {
      const result = await this.dependencies.executeTask(pending.taskId, "openclaw", exact);
      return [this.event(session, "completed", {
        taskId: result.taskId,
        result: result.output,
      })];
    } catch (error) {
      return [this.event(session, "failed", {
        taskId: pending.taskId,
        message: errorDetail(error),
      })];
    }
  }

  async receive(event: RemoteInboundEvent): Promise<RemoteAiOsEvent[]> {
    const session = this.resolveSession(event.sessionKey);
    const ready = this.event(session, "session_ready");
    if (event.type === "inbound_message") {
      return [ready, ...await this.handleMessage(session, event)];
    }
    if (event.type === "recommendation_response") {
      return [ready, ...await this.handleRecommendationResponse(session, event)];
    }
    if (event.type === "permission_response" || event.type === "danger_confirm") {
      return [ready, ...await this.handleConfirmation(session, event)];
    }
    if (!session.activeCancel) {
      return [ready, this.event(session, "cancelled", { message: "Nothing to cancel." })];
    }
    await session.activeCancel();
    return [ready, this.event(session, "cancelled", { message: "Cancellation requested." })];
  }
}

export function createRemoteAiOsGateway(): RemoteAiOsGateway {
  return new RemoteAiOsGateway();
}
