import { CouncilExecutionCoordinator } from "../src/services/councilExecution";
import { CouncilChiefOfStaff } from "../src/services/councilChiefOfStaff";
import type { CouncilSession } from "../src/types/council";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const session: CouncilSession = {
  id: "council-1",
  title: "Safe inventory",
  prompt: "List mounted network storage targets without changing anything.",
  createdAt: 1,
  updatedAt: 2,
  favorite: false,
  steps: [],
  finalAnswer: "List mounted network storage targets.",
  recommendation: {
    id: "recommendation-1",
    councilSessionId: "council-1",
    summary: "Inspect mounted network storage.",
    recommendedPlan: ["List mounted network storage targets"],
    rationale: ["This is read-only"],
    disagreements: [],
    risks: [],
    assumptions: [],
    uncertainty: [],
    provenance: ["council:test"],
    createdAt: 2,
  },
};

let createCount = 0;
const calls: Array<{ taskId: string; agentId: string; options: Record<string, unknown> }> = [];
const coordinator = new CouncilExecutionCoordinator({
  createTask: async () => ({
    taskId: `task-${++createCount}`,
    taskType: "DO",
    status: "PLANNING",
  }),
  executeTask: async (taskId, agentId, options = {}) => {
    calls.push({ taskId, agentId, options });
    if (!options.userConfirmed) {
      throw new Error(
        '[PermissionRequired] AI_OS_APPROVAL_REQUIRED_BEGIN{"capability":"nas.list","input":{}}AI_OS_APPROVAL_REQUIRED_END',
      );
    }
    return {
      taskId,
      planId: "plan-1",
      agentId,
      status: "VERIFYING",
      output: { targets: [] },
    };
  },
  now: () => 10,
});

if (createCount !== 0) throw new Error("Council recommendation executed without user approval");
const approved = await coordinator.approve(session);
assert(Number(createCount) === 1, "approved recommendation did not create one DO Task");
assert(approved.linkage.councilSessionId === "council-1", "Council session id was lost");
assert(approved.linkage.recommendationId === "recommendation-1", "recommendation id was lost");
assert(approved.linkage.taskId === "task-1", "Task id was lost");

const awaiting = await coordinator.execute(session, approved);
assert(calls[0]?.agentId === "openclaw", "selected Agent was not OpenClaw");
assert(Object.keys(calls[0]?.options ?? {}).length === 0, "Council bypassed Agent Skill selection");
assert(
  awaiting.feedback.kind === "progress" && awaiting.feedback.status === "awaiting-confirmation",
  "Agent-selected Skill did not reach exact confirmation",
);
assert(Number(createCount) === 1, "approval request unexpectedly created another Task");

const confirmed = await coordinator.execute(session, awaiting, {
  capability: "nas.list",
  input: {},
  userConfirmed: true,
});
assert(Number(createCount) === 2, "confirmed retry did not create a fresh planning Task");
assert(confirmed.linkage.previousTaskId === "task-1", "original Task linkage was lost");
assert(confirmed.linkage.taskId === "task-2", "active confirmed Task id was not retained");
assert(calls[1]?.options.capability === "nas.list", "exact capability was not retained");
assert(calls[1]?.options.userConfirmed === true, "exact confirmation was not retained");
assert(confirmed.feedback.kind === "success", "successful execution feedback was not emitted");

const blockerCoordinator = new CouncilExecutionCoordinator({
  createTask: async () => ({ taskId: "blocked-task", taskType: "DO", status: "PLANNING" }),
  executeTask: async () => {
    throw new Error("[NoViableExecutionPath] missing specialist evidence");
  },
  now: () => 20,
});
const blockerStart = await blockerCoordinator.approve(session);
const blocked = await blockerCoordinator.execute(session, blockerStart);
assert(blocked.feedback.kind === "blocker", "bounded execution blocker was not classified");
if (blocked.feedback.kind !== "blocker") throw new Error("expected blocker");
blocked.feedback.invalidatedAssumptions = ["NAS is mounted"];
blocked.feedback.suggestedExpertise = ["storage operations"];
const chief = new CouncilChiefOfStaff({
  paperclip: {
    probe: async () => { throw new Error("unused"); },
    listCompanies: async () => [],
  },
  agencyAgents: {
    loadCatalog: async () => { throw new Error("unused"); },
    loadAgent: async () => "",
  },
  listModels: () => [],
  uuid: () => "unused",
});
const decision = chief.evaluateExecutionBlocker(blocked.feedback);
assert(decision.action === "reconvene-experts", "Chief of Staff did not target expertise");
assert(decision.newSeatRequirements[0] === "storage operations", "new expertise was not bounded");
assert(decision.requiresUserApproval, "reconvening could bypass user approval");

const failureCoordinator = new CouncilExecutionCoordinator({
  createTask: async () => ({ taskId: "failed-task", taskType: "DO", status: "PLANNING" }),
  executeTask: async () => { throw new Error("unexpected failure"); },
  now: () => 30,
});
const failureStart = await failureCoordinator.approve(session);
const failed = await failureCoordinator.execute(session, failureStart);
assert(failed.feedback.kind === "failure", "ordinary failure feedback was not emitted");

const legacy = { ...session, recommendation: undefined };
const legacyApproved = await coordinator.approve(legacy);
assert(
  legacyApproved.linkage.recommendationId === "legacy-council-1",
  "legacy Council history lost execution compatibility",
);

console.log("PASS P16 Council Execution behavior");
