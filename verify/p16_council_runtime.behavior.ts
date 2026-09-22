import {
  CouncilCancelledError,
  CouncilRuntime,
} from "../src/services/councilRuntime";
import {
  agencyAgentToCouncilContext,
  distilledPersonaToCouncilContext,
  resolveCouncilContext,
} from "../src/services/councilMemberContext";
import { recommendationFromCouncilSession } from "../src/services/councilTaskHandoff";
import { loadCouncilMembers } from "../src/services/council";
import type { CouncilMember } from "../src/types/council";
import type { AiCenterResponse } from "../src/services/aiCenter";
import type { PersonProfileView, RunnablePersonaSkill } from "../src/types/personProfile";

function assertEqual(actual: unknown, expected: unknown, message: string): void {
  if (actual !== expected) {
    throw new Error(`${message}: expected ${String(expected)}, got ${String(actual)}`);
  }
}

function assertJsonEqual(actual: unknown, expected: unknown, message: string): void {
  assertEqual(JSON.stringify(actual), JSON.stringify(expected), message);
}

const members: CouncilMember[] = [
  {
    id: "judge",
    name: "Judge",
    icon: "J",
    providerId: "chatgpt",
    enabled: true,
    systemPrompt: "judge",
  },
  {
    id: "researcher",
    name: "Researcher",
    icon: "R",
    providerId: "chatgpt",
    enabled: true,
    systemPrompt: "researcher",
  },
  {
    id: "planner",
    name: "Planner",
    icon: "P",
    providerId: "chatgpt",
    enabled: true,
    systemPrompt: "planner",
  },
  {
    id: "critic",
    name: "Critic",
    icon: "C",
    providerId: "chatgpt",
    enabled: true,
    systemPrompt: "critic",
  },
  {
    id: "engineer",
    name: "Engineer",
    icon: "E",
    providerId: "chatgpt",
    enabled: true,
    systemPrompt: "engineer",
  },
];

const models = [
  {
    providerId: "chatgpt",
    providerInstanceId: "chatgpt-1",
    modelId: "model-1",
    label: "ChatGPT",
  },
  {
    providerId: "claude",
    providerInstanceId: "claude-1",
    modelId: "model-2",
    label: "Claude",
  },
];

const metadata = {
  providerId: "claude",
  modelId: "model-2",
  providerInstanceId: "claude-1",
  invocationId: "test",
  startedAt: "2026-01-01T00:00:00.000Z",
  completedAt: "2026-01-01T00:00:01.000Z",
  latencyMs: 1,
} as AiCenterResponse["metadata"];

let streamCalls = 0;
const providerEvents: string[] = [];
const completedMembers: string[] = [];
const runtime = new CouncilRuntime({
  listModels: () => models,
  stream: (messages, choice, onChunk) => {
    streamCalls += 1;
    const isFirstPlannerAttempt = streamCalls === 1;
    const isJudge = messages[0]?.content.includes("judge");
    const text = isJudge ? "final recommendation" : "planner output";
    return {
      operationId: `operation-${streamCalls}`,
      result: isFirstPlannerAttempt
        ? Promise.reject(new Error("provider unavailable"))
        : Promise.resolve().then(() => {
            onChunk(text);
            return {
              cancelled: false,
              response: {
                providerId: choice!.providerId,
                modelId: choice!.modelId,
                text,
                metadata,
              },
            };
          }),
      cancel: async () => undefined,
    };
  },
  resolveContext: async () => [
    {
      id: "paperclip:test",
      kind: "paperclip",
      title: "Paperclip",
      content: "governance",
      provenance: {
        sourceId: "paperclip",
        provenanceReferences: ["paperclip:test"],
      },
    },
  ],
  now: (() => {
    let now = 100;
    return () => ++now;
  })(),
  uuid: () => "session-1",
});

const result = await runtime.run({
  prompt: "Make a decision",
  members,
  callbacks: {
    onProviderChanged: (memberId, providerId) => {
      providerEvents.push(`${memberId}:${providerId}`);
    },
    onMemberCompleted: (step) => completedMembers.push(step.role),
  },
});

assertJsonEqual(
  completedMembers,
  ["planner", "engineer", "researcher", "critic", "judge"],
  "canonical five-role order",
);
assertJsonEqual(
  providerEvents.slice(0, 2),
  ["planner:chatgpt", "planner:claude"],
  "provider failover",
);
assertEqual(result.finalAnswer, "final recommendation", "Judge synthesis");
assertEqual(result.session.steps.at(-1)?.role, "judge", "Judge termination");
assertJsonEqual(result.metadata.integrationIds, ["paperclip"], "integration metadata");
assertJsonEqual(
  result.metadata.provenanceReferences,
  ["paperclip:test"],
  "provenance metadata",
);

const agencyContext = await agencyAgentToCouncilContext(
  { slug: "security-reviewer", division: "engineering", path: "engineering/security-reviewer.md" },
  { loadAgent: async () => "# Security Reviewer\nReview risks." },
);
assertEqual(agencyContext.kind, "agency-agent", "Agency Agent conversion");
assertEqual(
  agencyContext.provenance.sourcePath,
  "engineering/security-reviewer.md",
  "Agency Agent source path",
);

const claim = {
  claimId: "claim-1",
  statement: "Uses evidence before deciding.",
  confirmed: true,
  confidence: 0.8,
  evidenceIds: ["evidence-1"],
  contradictoryEvidenceIds: [],
};
const profileView: PersonProfileView = {
  activeRevision: 2,
  revisionCount: 2,
  profile: {
    profileId: "profile-alice",
    revision: 2,
    status: "active",
    subjectKind: "private-person",
    identity: [claim],
    domainExpertise: [],
    knowledgeModel: [],
    decisionPatterns: [],
    reasoningFrameworks: [],
    preferences: [],
    constraints: [],
    behavioralPatterns: [],
    communicationStyle: [],
    representativeExamples: [],
    unclassifiedClaims: [],
    pendingCognitiveCandidates: [],
    draftNarrative: [],
    evidenceBundleId: "bundle-1",
    contradictions: [],
    revisionHistory: [],
  },
};
const personaSkill: RunnablePersonaSkill = {
  skillId: "persona-profile-alice",
  profileId: "profile-alice",
  profileRevision: 2,
  instructions: "Use the reviewed profile.",
  examples: [],
  rawMediaAssets: [],
};
const personaContext = distilledPersonaToCouncilContext(profileView, personaSkill);
assertEqual(personaContext.humanReviewed, true, "distilled persona review state");
assertEqual(personaContext.confidence, 0.8, "distilled persona confidence");
assertJsonEqual(
  personaContext.provenance.provenanceReferences,
  [
    "person-profile:profile-alice@2",
    "evidence-bundle:bundle-1",
    "evidence:evidence-1",
  ],
  "distilled persona provenance",
);

const integrationContext = await resolveCouncilContext(
  { includePaperclip: true, includeLincoBridge: true },
  {
    paperclip: {
      probe: async () => ({
        id: "paperclip",
        name: "Paperclip",
        status: "available",
        detail: "ready",
        checkedAt: "now",
        metadata: { sourceCommit: "paperclip-commit" },
      }),
      listCompanies: async () => [{ id: "company-1", name: "One", raw: {} }],
    },
    agencyAgents: { loadAgent: async () => "" },
    getProfile: async () => profileView,
    buildPersonaSkill: async () => personaSkill,
  },
);
assertJsonEqual(
  integrationContext.map((source) => source.kind),
  ["paperclip"],
  "Paperclip context consumption without Linco transport leakage",
);

const recommendation = recommendationFromCouncilSession(result.session);
assertEqual(recommendation.councilSessionId, "session-1", "Task handoff session id");
assertJsonEqual(
  recommendation.provenance,
  ["paperclip:test"],
  "Task handoff provenance",
);

const storage = new Map<string, string>();
Object.defineProperty(globalThis, "localStorage", {
  value: {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
  },
  configurable: true,
});
storage.set(
  "ai-os.council.members.v1",
  JSON.stringify([{ id: "planner", name: "Legacy Planner", enabled: true }]),
);
const normalizedLegacyMembers = loadCouncilMembers();
assertEqual(normalizedLegacyMembers[0]?.name, "Legacy Planner", "legacy member data");
assertEqual(normalizedLegacyMembers[0]?.kind, "builtin", "legacy member kind");
assertEqual(normalizedLegacyMembers[0]?.role, "planner", "legacy member role");

let releaseCancellation: ((cancelled: boolean) => void) | undefined;
const cancellationRuntime = new CouncilRuntime({
  listModels: () => models.slice(0, 1),
  stream: () => ({
    operationId: "cancel-me",
    result: new Promise((resolve) => {
      releaseCancellation = (cancelled) =>
        resolve({
          cancelled,
          response: {
            providerId: "chatgpt",
            modelId: "model-1",
            text: "",
            metadata,
          },
        });
    }),
    cancel: async () => releaseCancellation?.(true),
  }),
  resolveContext: async () => [],
  now: () => 1,
  uuid: () => "session-2",
});

const cancelledRun = cancellationRuntime.run({ prompt: "Stop", members });
await Promise.resolve();
await cancellationRuntime.cancel();
let cancellationError: unknown;
try {
  await cancelledRun;
} catch (error) {
  cancellationError = error;
}
if (!(cancellationError instanceof CouncilCancelledError)) {
  throw new Error("Cancellation did not stop the active Council run.");
}

console.log("PASS: P16 Council Runtime behavior");
