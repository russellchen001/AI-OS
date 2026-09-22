import { RemoteAiOsGateway, type RemoteAiOsDependencies } from "../src/services/remoteAiOs";
import { LincoRemoteTransportAdapter } from "../src/services/integrations/lincoRemoteTransportAdapter";
import { resolveCouncilContext } from "../src/services/councilMemberContext";
import type { CouncilSession } from "../src/types/council";
import type { CouncilExecutionState } from "../src/types/councilExecution";
import type { CouncilRunResult } from "../src/types/councilRuntime";
import type { RemoteAiOsEvent, RemoteSessionMapping } from "../src/types/remoteInteraction";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function event(events: RemoteAiOsEvent[], type: RemoteAiOsEvent["type"]): RemoteAiOsEvent {
  const found = events.find((item) => item.type === type);
  if (!found) throw new Error(`Missing remote event: ${type}`);
  return found;
}

function councilSession(id: string, recommendationId: string): CouncilSession {
  return {
    id,
    title: "Remote Council",
    prompt: "Assess the plan",
    createdAt: 1,
    updatedAt: 2,
    favorite: false,
    steps: [],
    finalAnswer: "Use a bounded pilot.",
    recommendation: {
      id: recommendationId,
      councilSessionId: id,
      summary: "Use a bounded pilot.",
      recommendedPlan: ["Inspect safely", "Run a pilot"],
      rationale: ["Limits risk"],
      disagreements: [],
      risks: ["Evidence may change"],
      assumptions: ["Read-only inspection is available"],
      uncertainty: ["Demand remains uncertain"],
      provenance: ["paperclip:test"],
      createdAt: 2,
    },
  };
}

function councilResult(session: CouncilSession): CouncilRunResult {
  return {
    session,
    finalAnswer: session.finalAnswer,
    steps: [],
    recommendation: session.recommendation,
    metadata: {
      integrationIds: ["paperclip", "agency-agent"],
      provenanceReferences: ["paperclip:test"],
      contextSources: [],
    },
  };
}

const mappings = new Map<string, RemoteSessionMapping>();
const conversations = new Map<string, Array<{ role: string; content: string }>>();
let conversationCount = 0;
let taskCount = 0;
let councilCalls = 0;
let simulationCalls = 0;
let councilApprovalCalls = 0;
let councilExecutionCalls = 0;
let replanCalls = 0;
let exactExecutionOptions: Record<string, unknown> | undefined;
let exactExecutionTaskId: string | undefined;
let intent: "ASK" | "DO" = "ASK";
let councilExecutionMode: "approval" | "success" | "blocker" = "approval";
let confirmationCounter = 0;
let cancelCount = 0;

const baseCouncil = councilSession("council-1", "recommendation-1");
const revisedCouncil = councilSession("council-2", "recommendation-2");

function execution(
  state: "approval" | "success" | "blocker",
  taskId = "task-council-1",
): CouncilExecutionState {
  const linkage = {
    councilSessionId: "council-1",
    recommendationId: "recommendation-1",
    taskId,
  };
  if (state === "approval") {
    return {
      linkage,
      feedback: {
        ...linkage,
        kind: "progress",
        status: "awaiting-confirmation",
        approval: { capability: "nas.status", input: { protocol: "smb" } },
        occurredAt: 10,
        message: "confirmation required",
      },
    };
  }
  if (state === "blocker") {
    return {
      linkage,
      feedback: {
        ...linkage,
        kind: "blocker",
        status: "blocked",
        reason: "mount unavailable",
        observedReality: "NAS is not mounted",
        invalidatedAssumptions: ["NAS is mounted"],
        suggestedExpertise: ["storage operations"],
        occurredAt: 10,
        message: "blocked",
      },
    };
  }
  return {
    linkage,
    feedback: {
      ...linkage,
      kind: "success",
      status: "completed",
      planId: "plan-1",
      result: { mounted: true },
      occurredAt: 10,
      message: "completed",
    },
  };
}

function dependencies(): RemoteAiOsDependencies {
  return {
    persistence: {
      load: () => [...mappings.values()],
      save: (mapping) => mappings.set(mapping.sessionKey, { ...mapping }),
    },
    createConversation: () => {
      const id = `conversation-${++conversationCount}`;
      conversations.set(id, []);
      return id;
    },
    hasConversation: (id) => conversations.has(id),
    appendConversationMessage: (id, role, content) => {
      conversations.get(id)?.push({ role, content });
    },
    loadCouncilSession: (id) =>
      [baseCouncil, revisedCouncil].find((session) => session.id === id),
    saveCouncilSession: () => undefined,
    classifyIntent: async () => ({ taskType: intent, source: "semantic-rule" }),
    startAsk: (_conversationId, text, onChunk) => {
      onChunk("normal ASK response");
      return {
        result: Promise.resolve({ text: `answer:${text}`, cancelled: false }),
        cancel: async () => { cancelCount += 1; },
      };
    },
    submitTask: async (_prompt, taskType = "ASK") => ({
      taskId: `task-${++taskCount}`,
      taskType,
      status: taskType === "DO" ? "PLANNING" : "READY",
    }),
    executeTask: async (taskId, _agentId, options = {}) => {
      exactExecutionOptions = options;
      exactExecutionTaskId = taskId;
      if (!options.userConfirmed) {
        throw new Error(
          '[PermissionRequired] AI_OS_APPROVAL_REQUIRED_BEGIN{"capability":"nas.status","input":{"protocol":"smb"}}AI_OS_APPROVAL_REQUIRED_END',
        );
      }
      return {
        taskId,
        planId: "plan-remote",
        agentId: "openclaw",
        status: "VERIFYING",
        output: { status: "available" },
      };
    },
    runCouncil: async (_text, onProgress) => {
      councilCalls += 1;
      onProgress("existing dynamic council progress");
      return councilResult(baseCouncil);
    },
    cancelCouncil: async () => { cancelCount += 1; },
    runSimulation: async (_text, _profileId, onProgress) => {
      simulationCalls += 1;
      onProgress("existing simulation progress");
      return councilResult(baseCouncil);
    },
    cancelSimulation: async () => { cancelCount += 1; },
    councilExecution: {
      approve: async () => {
        councilApprovalCalls += 1;
        return {
          linkage: {
            councilSessionId: "council-1",
            recommendationId: "recommendation-1",
            taskId: "task-council-1",
          },
          feedback: {
            councilSessionId: "council-1",
            recommendationId: "recommendation-1",
            taskId: "task-council-1",
            kind: "progress",
            status: "planning",
            occurredAt: 10,
            message: "planning",
          },
        };
      },
      execute: async (_session, _state, approval) => {
        councilExecutionCalls += 1;
        if (approval?.userConfirmed) {
          exactExecutionOptions = approval;
          return execution("success", "task-council-2");
        }
        return execution(councilExecutionMode);
      },
    },
    replanning: {
      review: async () => {
        replanCalls += 1;
        return {
          status: "revised-recommendation-ready",
          sourceSessionId: "council-1",
          sourceRecommendationId: "recommendation-1",
          sourceTaskId: "task-council-1",
          revisedSession: revisedCouncil,
          revisedRecommendation: revisedCouncil.recommendation,
          requiresUserApproval: true,
        };
      },
    },
    uuid: () => `confirmation-${++confirmationCounter}`,
    now: (() => { let value = 100; return () => ++value; })(),
  };
}

const gateway = new RemoteAiOsGateway(dependencies());
const ask = await gateway.receive({
  type: "inbound_message",
  sessionKey: "linco-A",
  messageId: "message-ask",
  text: "Explain the current plan",
});
assert(event(ask, "assistant_chunk").message === "normal ASK response", "remote ASK bypassed normal ASK path");
assert(event(ask, "completed").message === "answer:Explain the current plan", "remote ASK result missing");
const firstMapping = gateway.getSession("linco-A").mapping;
const resumedMapping = gateway.getSession("linco-A").mapping;
assert(firstMapping.conversationId === resumedMapping.conversationId, "same sessionKey did not resume conversation");
assert(
  gateway.getSession("linco-B").mapping.conversationId !== firstMapping.conversationId,
  "different sessionKeys were not isolated",
);

const transport = new LincoRemoteTransportAdapter();
const normalized = transport.normalizeInbound({
  type: "inbound_message",
  sessionKey: "linco-A",
  messageId: "message-council",
  text: "Start a Council",
  mode: "council",
});
const normalizedRecommendation = transport.normalizeInbound({
  type: "recommendation_response",
  sessionKey: "linco-A",
  councilSessionId: "council-1",
  recommendationId: "recommendation-1",
  decision: "approve",
});
assert(
  normalizedRecommendation.type === "recommendation_response" &&
    normalizedRecommendation.approved,
  "transport did not normalize Council recommendation approval",
);
const council = await gateway.receive(normalized);
assert(councilCalls === 1, "remote Council did not reuse existing Dynamic Council runtime");
const recommendation = event(council, "recommendation_approval_required");
assert(recommendation.councilSessionId === "council-1", "Council session id missing remotely");
assert(recommendation.recommendationId === "recommendation-1", "recommendation id missing remotely");
assert(recommendation.recommendation?.risks.length === 1, "structured recommendation state missing");

const approved = await gateway.receive({
  type: "recommendation_response",
  sessionKey: "linco-A",
  councilSessionId: "council-1",
  recommendationId: "recommendation-1",
  approved: true,
});
assert(councilApprovalCalls === 1, "remote approval did not reuse Council Task handoff");
assert(councilExecutionCalls === 1, "remote approval did not enter existing execution coordinator");
event(approved, "planning");
const confirmation = event(approved, "execution_confirmation_required");
assert(confirmation.confirmation?.action === "nas.status", "exact Runtime confirmation not surfaced");
assert(!JSON.stringify(confirmation).includes("userConfirmed"), "trusted authorization leaked remotely");

const mismatch = await gateway.receive({
  type: "permission_response",
  sessionKey: "linco-A",
  confirmationId: confirmation.confirmationId!,
  taskId: "wrong-task",
  approved: true,
});
event(mismatch, "error");
assert(councilExecutionCalls === 1, "mismatched approval executed work");

const confirmed = await gateway.receive({
  type: "permission_response",
  sessionKey: "linco-A",
  confirmationId: confirmation.confirmationId!,
  taskId: confirmation.taskId!,
  approved: true,
});
const completed = event(confirmed, "completed");
assert(completed.councilSessionId === "council-1", "Council provenance missing from result");
assert(completed.recommendationId === "recommendation-1", "recommendation provenance missing from result");
assert(completed.taskId === "task-council-2", "active Task id missing from result");
assert(exactExecutionOptions?.userConfirmed === true, "matching pending confirmation was not consumed");
assert(exactExecutionOptions?.capability === "nas.status", "remote client manufactured capability context");

const recommendationReplay = await gateway.receive({
  type: "recommendation_response",
  sessionKey: "linco-A",
  councilSessionId: "council-1",
  recommendationId: "recommendation-1",
  approved: true,
});
event(recommendationReplay, "error");
assert(Number(councilApprovalCalls) === 1, "recommendation approval executed twice");

const replay = await gateway.receive({
  type: "danger_confirm",
  sessionKey: "linco-A",
  confirmationId: confirmation.confirmationId!,
  taskId: confirmation.taskId!,
  approved: true,
});
event(replay, "error");
assert(Number(councilExecutionCalls) === 2, "consumed confirmation executed twice");

intent = "DO";
const ordinary = await gateway.receive({
  type: "inbound_message",
  sessionKey: "linco-B",
  messageId: "message-do",
  text: "Inspect NAS status",
});
const ordinaryConfirmation = event(ordinary, "execution_confirmation_required");
const ordinaryPendingTaskId = ordinaryConfirmation.taskId;
const ordinaryDone = await gateway.receive({
  type: "permission_response",
  sessionKey: "linco-B",
  confirmationId: ordinaryConfirmation.confirmationId!,
  taskId: ordinaryConfirmation.taskId!,
  approved: true,
});
event(ordinaryDone, "completed");
assert(exactExecutionOptions?.capability === "nas.status", "ordinary DO bypassed exact confirmation");
assert(exactExecutionTaskId === ordinaryPendingTaskId, "ordinary confirmation created a second Task");

const blockerGateway = new RemoteAiOsGateway(dependencies());
councilExecutionMode = "blocker";
await blockerGateway.receive({
  type: "inbound_message",
  sessionKey: "linco-blocked",
  messageId: "blocked-council",
  text: "Assess blocker",
  mode: "council",
});
const blocked = await blockerGateway.receive({
  type: "recommendation_response",
  sessionKey: "linco-blocked",
  councilSessionId: "council-1",
  recommendationId: "recommendation-1",
  approved: true,
});
event(blocked, "blocked");
const revised = event(blocked, "revised_recommendation");
assert(replanCalls === 1, "remote blocker did not reuse Council replanning coordinator");
assert(revised.recommendation?.requiresApproval, "revised recommendation lost approval boundary");
assert(revised.recommendationId === "recommendation-2", "revised recommendation identity missing");
councilExecutionMode = "approval";

const simulation = await gateway.receive({
  type: "inbound_message",
  sessionKey: "linco-simulation",
  messageId: "message-simulation",
  text: "Predict the likely response",
  mode: "simulation",
  profileId: "profile-1",
});
assert(simulationCalls === 1, "remote simulation did not reuse Simulation Council");
assert(event(simulation, "completed").result !== undefined, "simulation result missing remotely");

let releaseAsk: ((value: { text: string; cancelled: boolean }) => void) | undefined;
const cancelDeps = dependencies();
cancelDeps.classifyIntent = async () => ({ taskType: "ASK", source: "semantic-rule" });
cancelDeps.startAsk = () => ({
  result: new Promise((resolve) => { releaseAsk = resolve; }),
  cancel: async () => {
    cancelCount += 1;
    releaseAsk?.({ text: "", cancelled: true });
  },
});
const cancelGateway = new RemoteAiOsGateway(cancelDeps);
const pendingAsk = cancelGateway.receive({
  type: "inbound_message",
  sessionKey: "linco-cancel",
  messageId: "message-cancel",
  text: "Long answer",
});
await Promise.resolve();
await Promise.resolve();
const cancelled = await cancelGateway.receive({ type: "stop_turn", sessionKey: "linco-cancel" });
event(cancelled, "cancelled");
await pendingAsk;
assert(cancelCount > 0, "stop_turn did not map to existing cancellation");

const noPending = await gateway.receive({
  type: "permission_response",
  sessionKey: "linco-B",
  confirmationId: "made-up",
  taskId: "made-up",
  approved: true,
});
event(noPending, "error");

const filteredContext = await resolveCouncilContext(
  {
    includePaperclip: false,
    sources: [{
      id: "linco-transport",
      kind: "linco-bridge",
      title: "must not enter reasoning",
      content: "transport metadata",
      provenance: { sourceId: "linco", provenanceReferences: [] },
    }],
  },
  {
    paperclip: {
      probe: async () => { throw new Error("unused"); },
      listCompanies: async () => [],
    },
    agencyAgents: { loadAgent: async () => "" },
    getProfile: async () => { throw new Error("unused"); },
    buildPersonaSkill: async () => { throw new Error("unused"); },
  },
);
assert(filteredContext.length === 0, "Linco transport metadata entered Council reasoning");

console.log("PASS P16 Remote AI-OS / Linco behavior");
