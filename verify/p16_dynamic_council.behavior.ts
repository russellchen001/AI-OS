import { CouncilChiefOfStaff } from "../src/services/councilChiefOfStaff";
import {
  CouncilCancelledError,
  CouncilRuntime,
} from "../src/services/councilRuntime";
import { loadCouncilMembers } from "../src/services/council";
import { validateCouncilAnswer } from "../src/services/answerRequirements";
import type { CouncilMember } from "../src/types/council";
import type { AgencyAgentsCatalog } from "../src/types/councilIntegrations";
import type { AiCenterResponse } from "../src/services/aiCenter";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function equal(actual: unknown, expected: unknown, message: string): void {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${message}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

const models = [
  {
    providerId: "chatgpt",
    providerInstanceId: "chatgpt-1",
    modelId: "gpt-business",
    label: "ChatGPT Business",
  },
  {
    providerId: "claude",
    providerInstanceId: "claude-1",
    modelId: "claude-review",
    label: "Claude Review",
  },
];

const qualityObjective = "不修改任何文件，最终不超过180个汉字，包含2条理由、1条可逆的本地备份建议。";
const qualityReport = { summary: "建议保留本地，并防范单机丢失。", recommendedPlan: ["建议将笔记复制到已有外接盘并只读核对，保留原件；此处不执行。"], rationale: ["避免上传造成隐私暴露。", "单机故障仍可能导致丢失。"], disagreements: [], risks: [], assumptions: [], uncertainty: [] };
validateCouncilAnswer(qualityObjective, qualityReport);
validateCouncilAnswer(qualityObjective, { ...qualityReport, recommendedPlan: ["复制到已有外盘，不删除或移动原文件。"] });
for (const bad of [
  { ...qualityReport, summary: "长".repeat(181) },
  { ...qualityReport, rationale: ["只有一条"] },
  { ...qualityReport, recommendedPlan: ["将原文件移动到外接盘"] },
  { ...qualityReport, recommendedPlan: ["不删除，但将原文件移动到外接盘"] },
]) {
  let rejected = false;
  try { validateCouncilAnswer(qualityObjective, bad); } catch { rejected = true; }
  assert(rejected, "Council accepted an explicit length/count/original-preservation violation");
}

// "汉字" limits count Han characters only; "字" limits count every character.
const hanOnlyReport = { ...qualityReport, summary: "长".repeat(100) + "!".repeat(100), recommendedPlan: ["复制"], rationale: ["a", "b"] };
validateCouncilAnswer(qualityObjective, hanOnlyReport);
let allCharactersCounted = false;
try { validateCouncilAnswer(qualityObjective.replace("个汉字", "个字"), hanOnlyReport); } catch (error) {
  allCharactersCounted = String(error).includes("(it has 204)");
}
assert(allCharactersCounted, "a 字 limit must count every character and report the measured length");

const catalog: AgencyAgentsCatalog = {
  commit: "agency-commit",
  divisions: [
    { id: "marketing", name: "Marketing", raw: {} },
    { id: "finance", name: "Finance", raw: {} },
    { id: "testing", name: "Testing", raw: {} },
    { id: "specialized", name: "Specialized", raw: {} },
  ],
  runbooks: [],
  agents: [
    { division: "marketing", slug: "marketing-market-researcher", path: "marketing/marketing-market-researcher.md" },
    { division: "finance", slug: "finance-financial-analyst", path: "finance/finance-financial-analyst.md" },
    { division: "testing", slug: "testing-reality-checker", path: "testing/testing-reality-checker.md" },
    { division: "specialized", slug: "decision-synthesizer", path: "specialized/decision-synthesizer.md" },
  ],
};

const chief = new CouncilChiefOfStaff({
  paperclip: {
    probe: async () => ({
      id: "paperclip",
      name: "Paperclip",
      status: "available",
      detail: "ready",
      checkedAt: "now",
      metadata: { sourceCommit: "paperclip-commit" },
    }),
    listCompanies: async () => [
      { id: "company-1", name: "Example Company", raw: { governance: "bounded" } },
    ],
  },
  agencyAgents: {
    loadCatalog: async () => catalog,
    loadAgent: async (definition) =>
      `# ${definition.slug}\nMethodology: evidence first.\nWorkflow: analyse, test, report.`,
  },
  listModels: () => models,
  uuid: () => "assembly-1",
});

const plan = await chief.assemble({
  objective: "Create a market entry and growth strategy with financial discipline.",
});

assert(plan.mode === "dynamic", "business prompt did not create a dynamic assembly");
assert(plan.seats.length === 4, "dynamic assembly should use objective-specific bounded seats");
assert(
  plan.seats.every((seat) => !["planner", "engineer", "researcher", "critic", "judge"].includes(seat.id)),
  "dynamic assembly is constrained to legacy five-role identities",
);
assert(plan.provenance.paperclip.status === "consumed", "Paperclip context was not consumed");
equal(plan.provenance.paperclip.companyIds, ["company-1"], "Paperclip company provenance");
assert(
  plan.rationale.includes("1 configured company"),
  "Paperclip governance context did not affect the assembly rationale",
);
assert(
  plan.seats.some((seat) => seat.memberContext.includes("Methodology: evidence first")),
  "Relevant Agency Agent definitions were not consumed",
);
assert(
  plan.seats.filter((seat) => seat.kind === "agency-agent").every(
    (seat) =>
      seat.source.sourceCommit === "agency-commit" &&
      Boolean(seat.source.sourcePath) &&
      seat.source.provenanceReferences.length > 0,
  ),
  "Agency Agent provenance was not retained",
);
let irrelevantLoads = 0;
const irrelevantPlan = await new CouncilChiefOfStaff({
  paperclip: { probe: async () => ({ id: "paperclip", name: "Paperclip", status: "unavailable", detail: "offline", checkedAt: "now" }), listCompanies: async () => [] },
  agencyAgents: {
    loadCatalog: async () => ({ ...catalog, agents: [
      { division: "database", slug: "gaussdb-expert", path: "database/gaussdb.md" },
      { division: "marketing", slug: "marketing-private-domain-operator", path: "marketing/private-domain.md" },
      { division: "project-management", slug: "project-management-studio-operations", path: "project-management/operations.md" },
      { division: "legal", slug: "legal-document-review", path: "legal/review.md" },
      { division: "engineering", slug: "engineering-china-network-engineer", path: "engineering/network.md" },
    ] }),
    loadAgent: async () => { irrelevantLoads += 1; return "Unrelated database profile must never enter note advice."; },
  },
  listModels: () => models,
  invokeChiefOfStaff: async () => { throw new Error("fallback"); },
  uuid: () => "irrelevant-profile",
}).assemble({ objective: "Decide whether two personal notes should stay on this Mac." });
assert(irrelevantLoads === 0 && irrelevantPlan.seats.every((seat) => seat.kind === "builtin"), "zero-relevance profile loaded or polluted Council seats");
assert(
  new Set(plan.seats.map((seat) => seat.assignedModel.providerId)).size === 2,
  "dynamic model assignment did not preserve provider diversity",
);

// Named participants: each named model gets its own pinned seat, and model
// labels never count as task-domain evidence for professional profiles.
const namedModels = [
  ...models,
  { providerId: "acme", providerInstanceId: "acme-1", modelId: "acme-account-default", label: "Acme account default" },
  { providerId: "omlx", providerInstanceId: "omlx-local", modelId: "local-1", label: "Local 1" },
  { providerId: "omlx", providerInstanceId: "omlx-local", modelId: "local-2", label: "Local 2" },
];
let accountProfileLoads = 0;
const namedChief = new CouncilChiefOfStaff({
  paperclip: { probe: async () => ({ id: "paperclip", name: "Paperclip", status: "unavailable", detail: "offline", checkedAt: "now" }), listCompanies: async () => [] },
  agencyAgents: {
    loadCatalog: async () => ({ ...catalog, agents: [
      { division: "marketing", slug: "marketing-official-account", path: "marketing/official-account.md" },
    ] }),
    loadAgent: async () => { accountProfileLoads += 1; return "Official account marketing profile"; },
  },
  listModels: () => namedModels,
  invokeChiefOfStaff: async () => { throw new Error("use fallback"); },
  uuid: () => "named-assembly",
});
const namedPlan = await namedChief.assemble({
  objective: "让 gpt-business、claude-review、acme-account-default、local-1 和 local-2 都参与讨论：家庭照片如何长期备份？",
});
equal(
  namedPlan.seats.map((seat) => seat.assignedModel.modelId).sort(),
  ["acme-account-default", "claude-review", "gpt-business", "local-1", "local-2"],
  "every named model must hold exactly one Council seat",
);
assert(namedPlan.seats.every((seat) => seat.assignedModel.fallbackModelIds.length === 0), "a named seat may silently fall back");
assert(accountProfileLoads === 0, "a model label was used as task-domain evidence for a profile");
// Without a stated synthesizer the strongest named model synthesizes (cloud
// above local, then the one named last); "由 X 汇总" overrides that.
const synthOf = (assembly: typeof namedPlan) =>
  assembly.seats.find((seat) => seat.id === assembly.synthesizerSeatId)?.assignedModel.modelId;
equal(synthOf(namedPlan), "acme-account-default", "the last-named cloud model should synthesize by default");
const statedPlan = await namedChief.assemble({
  objective: "让 gpt-business、claude-review 和 local-1 都参与讨论，由 local-1 汇总：家庭照片如何长期备份？",
});
equal(synthOf(statedPlan), "local-1", "a stated synthesizer must be honoured");
equal(
  statedPlan.seats.map((seat) => seat.assignedModel.modelId).sort(),
  ["claude-review", "gpt-business", "local-1"],
  "a stated synthesizer must still seat every named model once",
);
let namedCloudRejected = false;
try {
  await namedChief.assemble({ objective: "仅使用本机本地模型，让 local-1、gpt-business 都参与讨论笔记是否保留在本地。" });
} catch (error) {
  namedCloudRejected = String(error).includes("gpt-business") || String(error).includes("ChatGPT Business");
}
assert(namedCloudRejected, "a local-only Council silently dropped a named cloud model");

const localPlan = await new CouncilChiefOfStaff({
  paperclip: {
    probe: async () => ({ id: "paperclip", name: "Paperclip", status: "unavailable", detail: "offline", checkedAt: "now" }),
    listCompanies: async () => [],
  },
  agencyAgents: { loadCatalog: async () => catalog, loadAgent: async () => "Local role" },
  listModels: () => [
    ...models,
    { providerId: "omlx", providerInstanceId: "omlx-local", modelId: "local-1", label: "Local 1" },
    { providerId: "omlx", providerInstanceId: "omlx-local", modelId: "local-2", label: "Local 2" },
  ],
  invokeChiefOfStaff: async () => { throw new Error("use fallback"); },
  uuid: () => "local-assembly",
}).assemble({ objective: "Keep this private local 本地 and never use cloud providers." });
assert(
  localPlan.seats.every((seat) => seat.assignedModel.providerId === "omlx" && seat.assignedModel.fallbackModelIds.every((id) => id === "local-1" || id === "local-2")),
  "local-only Council could fall back to a cloud model",
);

const localRuntimeChoices: string[] = [];
let invalidLocalSynthesis = false;
let missingLocalRejected = false;
try {
  await chief.assemble({ objective: "仅使用本地模型，禁止云端调用" });
} catch (error) {
  missingLocalRejected = String(error).includes("No local AI Center model");
}
assert(missingLocalRejected, "local-only Council silently assigned cloud when local models were absent");
const localRuntime = new CouncilRuntime({
  listModels: () => [
    ...models,
    { ...models[0], modelId: "local-2" },
    { providerId: "omlx", providerInstanceId: "omlx-local", modelId: "local-1", label: "Local 1" },
    { providerId: "omlx", providerInstanceId: "omlx-local", modelId: "local-2", label: "Local 2" },
  ],
  stream: (messages, choice, onChunk) => {
    localRuntimeChoices.push(`${choice?.providerId}:${choice?.modelId}`);
    const text = messages[1]?.content.includes("Return JSON only")
      ? invalidLocalSynthesis ? '{"summary":"Invalid","rationale":["bad":"value"]}' : JSON.stringify({ summary: "Local", recommendedPlan: [], rationale: [], disagreements: [], risks: [], assumptions: [], uncertainty: [] })
      : "Local contribution";
    return {
      operationId: `local-${localRuntimeChoices.length}`,
      result: Promise.resolve().then(() => {
        onChunk(text);
        return { cancelled: false, response: { providerId: choice!.providerId, modelId: choice!.modelId, text, metadata: {} as AiCenterResponse["metadata"] } };
      }),
      cancel: async () => undefined,
    };
  },
  resolveContext: async () => [],
  now: () => 1,
  uuid: () => "local-runtime",
});
await localRuntime.run({ prompt: localPlan.objective, members: [], assemblyPlan: localPlan });
assert(
  localRuntimeChoices.every((choice) => choice.startsWith("omlx:local-")),
  "local-only Council runtime attempted a cloud model",
);
invalidLocalSynthesis = true;
let invalidRecommendationRejected = false;
let rejectedSynthesisOutput = "";
try {
  await localRuntime.run({ prompt: localPlan.objective, members: [], assemblyPlan: localPlan, callbacks: {
    onMemberFailed: (step) => { if (step.stage === "final-synthesis") rejectedSynthesisOutput = step.output; },
  } });
} catch (error) {
  invalidRecommendationRejected = String(error).includes("invalid JSON");
}
assert(invalidRecommendationRejected, "malformed synthesis was accepted as a completed recommendation");
assert(rejectedSynthesisOutput.startsWith('{"summary":"Invalid","rationale":["bad":"value"]}'), "rejected raw synthesis output was lost");

// An over-limit synthesis gets one correction from the same model first.
const correctionCalls: string[] = [];
const correctionRuntime = new CouncilRuntime({
  listModels: () => [{ providerId: "omlx", providerInstanceId: "omlx-local", modelId: "local-1", label: "Local 1" }],
  stream: (messages, choice, onChunk) => {
    const synthesis = messages[1]?.content.includes("Return JSON only");
    const correcting = messages.some((message) => message.content.startsWith("Your answer was rejected"));
    if (synthesis) correctionCalls.push(correcting ? "correction" : "synthesis");
    const text = !synthesis
      ? "Local contribution"
      : JSON.stringify({ ...qualityReport, summary: correcting ? qualityReport.summary : "长".repeat(181) });
    return {
      operationId: `correction-${correctionCalls.length}`,
      result: Promise.resolve().then(() => {
        onChunk(text);
        return { cancelled: false, response: { providerId: choice!.providerId, modelId: choice!.modelId, text, metadata: {} as AiCenterResponse["metadata"] } };
      }),
      cancel: async () => undefined,
    };
  },
  resolveContext: async () => [],
  now: () => 1,
  uuid: () => "correction-runtime",
});
const corrected = await correctionRuntime.run({ prompt: `${localPlan.objective}${qualityObjective}`, members: [], assemblyPlan: localPlan });
equal(correctionCalls, ["synthesis", "correction"], "over-limit synthesis must get exactly one same-model correction");
assert(corrected.recommendation?.summary === qualityReport.summary, "corrected synthesis was not used");

const metadata = {
  providerId: "chatgpt",
  modelId: "gpt-business",
  providerInstanceId: "chatgpt-1",
  invocationId: "dynamic-test",
  startedAt: "2026-01-01T00:00:00.000Z",
  completedAt: "2026-01-01T00:00:01.000Z",
  latencyMs: 1,
} as AiCenterResponse["metadata"];

const stageCalls: string[] = [];
const runtimeMessages: string[][] = [];
const dynamicRuntime = new CouncilRuntime({
  listModels: () => models,
  stream: (messages, choice, onChunk) => {
    const system = messages[0]?.content ?? "";
    const task = messages[1]?.content ?? "";
    runtimeMessages.push([system, task]);
    const stage = task.includes("Return JSON only")
      ? "synthesis"
      : task.includes("Cross-review")
        ? "critique"
        : "analysis";
    stageCalls.push(stage);
    const shouldFail = system.includes("Financial Analyst");
    const text =
      stage === "synthesis"
        ? JSON.stringify({
            summary: "Enter the market through a bounded pilot.",
            recommendedPlan: ["Validate demand", "Run a measured pilot"],
            rationale: ["Evidence supports staged commitment"],
            disagreements: ["Experts disagree on launch speed"],
            risks: ["Demand may be overstated"],
            assumptions: ["Pilot access is available"],
            uncertainty: ["Conversion rate remains uncertain"],
          })
        : `${stage} contribution from ${system.split("\n")[0]}`;
    return {
      operationId: `${stage}-${stageCalls.length}`,
      result: shouldFail
        ? Promise.reject(new Error("isolated provider failure"))
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
  resolveContext: async () => [],
  now: (() => {
    let value = 100;
    return () => ++value;
  })(),
  uuid: (() => {
    let value = 0;
    return () => `dynamic-${++value}`;
  })(),
});

plan.deliberation.maxRounds = 99;
const result = await dynamicRuntime.run({ prompt: plan.objective, members: [], assemblyPlan: plan });
const expertCount = plan.seats.length - 1;
assert(
  result.steps.filter((step) => step.stage === "independent-analysis").length === expertCount,
  "independent expert round is missing",
);
assert(
  result.steps.filter((step) => step.stage === "cross-review").length === expertCount,
  "cross-review round is missing",
);
assert(
  result.steps.filter((step) => step.stage === "final-synthesis").length === 1,
  "final synthesis is missing",
);
assert(result.steps.length === expertCount * 2 + 1, "max rounds were not bounded");
assert(result.steps.some((step) => step.status === "error"), "provider failure was not isolated");
assert(result.steps.at(-1)?.status === "done", "isolated failure prevented final synthesis");
assert(result.steps.some((step) => step.output.includes("contribution")), "individual contributions were not retained");
assert(result.recommendation?.disagreements.length === 1, "disagreement was not preserved");
assert(result.recommendation?.uncertainty.length === 1, "uncertainty was not preserved");
assert(result.session.assemblyPlan?.id === "assembly-1", "assembly plan was not stored with the session");
assert(result.session.recommendation?.risks.length === 1, "structured recommendation was not stored");
assert(
  runtimeMessages.every(([system]) => system.includes("explicit user constraints") && system.includes("destructive or irreversible")),
  "Council seats did not receive binding user and safety constraints",
);
assert(
  runtimeMessages.at(-1)?.[1].includes("unrequested cloud services") && runtimeMessages.at(-1)?.[1].includes("across all fields combined"),
  "final synthesis did not enforce scope and response-length constraints",
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
const legacyMembers = loadCouncilMembers();
assert(legacyMembers[0]?.name === "Legacy Planner", "old Council localStorage is incompatible");

const legacyFive: CouncilMember[] = [
  "planner",
  "engineer",
  "researcher",
  "critic",
  "judge",
].map((role) => ({
  id: role,
  role: role as CouncilMember["role"],
  name: role,
  icon: role,
  providerId: "chatgpt",
  enabled: true,
  systemPrompt: role,
}));
const legacyRuntime = new CouncilRuntime({
  listModels: () => models.slice(0, 1),
  stream: (messages, choice) => {
    const text = messages[0]?.content.includes("judge") ? "legacy final" : "legacy contribution";
    return {
      operationId: "legacy",
      result: Promise.resolve({
        cancelled: false,
        response: { providerId: choice!.providerId, modelId: choice!.modelId, text, metadata },
      }),
      cancel: async () => undefined,
    };
  },
  resolveContext: async () => [],
  now: () => 1,
  uuid: () => "legacy-session",
});
const legacyResult = await legacyRuntime.run({ prompt: "fallback", members: legacyFive });
assert(legacyResult.finalAnswer === "legacy final", "legacy five-role fallback no longer works");

let releaseCancellation: ((cancelled: boolean) => void) | undefined;
const cancellationRuntime = new CouncilRuntime({
  listModels: () => models.slice(0, 1),
  stream: () => ({
    operationId: "cancel-dynamic",
    result: new Promise((resolve) => {
      releaseCancellation = (cancelled) =>
        resolve({
          cancelled,
          response: {
            providerId: "chatgpt",
            modelId: "gpt-business",
            text: "",
            metadata,
          },
        });
    }),
    cancel: async () => releaseCancellation?.(true),
  }),
  resolveContext: async () => [],
  now: () => 1,
  uuid: () => "cancel-session",
});
const cancellation = cancellationRuntime.run({ prompt: plan.objective, members: [], assemblyPlan: plan });
await Promise.resolve();
await cancellationRuntime.cancel();
let cancelled = false;
try {
  await cancellation;
} catch (error) {
  cancelled = error instanceof CouncilCancelledError;
}
assert(cancelled, "dynamic Council cancellation did not stop the active stream");

console.log("PASS: P16 Dynamic Council Core behavior");
