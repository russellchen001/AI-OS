import {
  ChiefOfStaffOrchestrator,
  selectChiefOfStaffModels,
  type ChiefOfStaffModelInvoker,
} from "../src/services/chiefOfStaffOrchestrator";
import { CouncilChiefOfStaff } from "../src/services/councilChiefOfStaff";
import type { AiCenterModelChoice } from "../src/services/aiCenter";
import type { AgencyAgentsCatalog } from "../src/types/councilIntegrations";

let pass = 0;
let fail = 0;

async function check(name: string, test: () => void | Promise<void>): Promise<void> {
  try {
    await test();
    pass += 1;
    console.log(`✓ ${name}`);
  } catch (error) {
    fail += 1;
    console.error(`✗ ${name}: ${error instanceof Error ? error.message : String(error)}`);
  }
}

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const models: AiCenterModelChoice[] = [
  { providerId: "cloud-a", providerInstanceId: "cloud-a-1", modelId: "reasoner-a", label: "Reasoner A" },
  { providerId: "omlx", providerInstanceId: "omlx-local", modelId: "local-reasoner", label: "Local Reasoner" },
  { providerId: "cloud-b", providerInstanceId: "cloud-b-1", modelId: "reasoner-b", label: "Reasoner B" },
];

const validDecision = JSON.stringify({
  seats: [
    {
      id: "market",
      title: "Market Strategist",
      purpose: "Assess customers and market options.",
      requiredExpertise: ["market research", "strategy"],
    },
    {
      id: "risk",
      title: "Risk Reviewer",
      purpose: "Challenge assumptions and identify uncertainty.",
      requiredExpertise: ["risk analysis", "critical review"],
    },
    {
      id: "synthesis",
      title: "Decision Synthesizer",
      purpose: "Reconcile evidence into a recommendation.",
      requiredExpertise: ["decision synthesis"],
      synthesizer: true,
    },
  ],
  rationale: "A market expert, independent reviewer, and synthesizer are complementary.",
});

const fallback = () => [
  { id: "expert", title: "Domain Expert", purpose: "Analyse the objective.", requiredExpertise: ["analysis"] },
  { id: "reviewer", title: "Risk Reviewer", purpose: "Review risk.", requiredExpertise: ["risk"] },
  { id: "synthesizer", title: "Decision Synthesizer", purpose: "Synthesize.", requiredExpertise: ["synthesis"], synthesizer: true },
];

function orchestrator(invokeModel: ChiefOfStaffModelInvoker): ChiefOfStaffOrchestrator {
  return new ChiefOfStaffOrchestrator({ invokeModel, timeoutMs: 100 });
}

function input(availableModels = models) {
  return {
    objective: "Plan a bounded market launch.",
    domain: "council" as const,
    availableModels,
    governanceContext: "Paperclip governance scope includes one company.",
    agencyContext: "Agency Agents exposes marketing, finance, and review roles.",
    constraints: { minSeats: 3, maxSeats: 5, localFirst: false },
    deterministicFallback: fallback,
  };
}

await check("LLM-backed primary path accepts a valid structured assembly", async () => {
  const result = await orchestrator(async () => ({ text: validDecision })).assemble(input());
  assert(result.provenance.mode === "llm-chief-of-staff", "LLM path was not recorded");
  assert(result.requirements[0]?.title === "Market Strategist", "LLM seats were not accepted");
});

await check("Chief-of-Staff model is selected dynamically from AI Center order", async () => {
  let selected = "";
  const reordered = [models[2], models[0], models[1]];
  const result = await orchestrator(async (_prompt, model) => {
    selected = model.modelId;
    return { text: validDecision };
  }).assemble(input(reordered));
  assert(selected === "reasoner-b", "selector ignored the available AI Center order");
  assert(result.provenance.mode === "llm-chief-of-staff" && result.provenance.model.modelId === selected, "selection provenance missing");
});

await check("Local-first selection prefers an eligible local AI Center model", () => {
  const selected = selectChiefOfStaffModels(models, true);
  assert(selected[0]?.model.providerId === "omlx", "local-first did not prefer the local model");
});

await check("Unavailable malformed model choices are rejected", () => {
  const selected = selectChiefOfStaffModels([
    { providerId: "", providerInstanceId: "missing", modelId: "bad", label: "Bad" },
    models[0],
  ], false);
  assert(selected.length === 1 && selected[0]?.model.modelId === "reasoner-a", "invalid model remained eligible");
});

await check("Malformed LLM output falls back deterministically", async () => {
  const result = await orchestrator(async () => ({ text: "not-json" })).assemble(input([models[0]]));
  assert(result.provenance.mode === "deterministic-fallback", "malformed output did not fall back");
  assert(result.requirements[0]?.id === "expert", "deterministic requirements were not used");
});

await check("AI Center invocation failure falls back safely", async () => {
  const result = await orchestrator(async () => { throw new Error("provider offline"); }).assemble(input([models[0]]));
  assert(result.provenance.mode === "deterministic-fallback", "invocation failure did not fall back");
  assert(result.provenance.fallbackReason.includes("provider offline"), "failure provenance missing");
});

await check("Chief-of-Staff invocation timeout falls back safely", async () => {
  const timed = new ChiefOfStaffOrchestrator({
    invokeModel: async () => new Promise(() => undefined),
    timeoutMs: 5,
  });
  const result = await timed.assemble(input([models[0]]));
  assert(result.provenance.mode === "deterministic-fallback", "timeout did not fall back");
  assert(result.provenance.fallbackReason.includes("timed out"), "timeout provenance missing");
});

await check("No eligible Chief-of-Staff model falls back without invocation", async () => {
  let invoked = false;
  const result = await orchestrator(async () => { invoked = true; return { text: validDecision }; }).assemble(input([]));
  assert(!invoked, "model invocation occurred without an eligible model");
  assert(result.provenance.mode === "deterministic-fallback", "missing model did not fall back");
});

await check("Seat-count and synthesizer policy violations are rejected", async () => {
  const invalid = JSON.stringify({
    seats: [
      { id: "one", title: "One", purpose: "One role", requiredExpertise: ["one"], synthesizer: true },
      { id: "two", title: "Two", purpose: "Two role", requiredExpertise: ["two"], synthesizer: true },
    ],
    rationale: "Invalid.",
  });
  const result = await orchestrator(async () => ({ text: invalid })).assemble(input([models[0]]));
  assert(result.provenance.mode === "deterministic-fallback", "invalid policy output was accepted");
});

await check("Duplicate or empty roles are rejected", async () => {
  const parsed = JSON.parse(validDecision);
  parsed.seats[1].title = parsed.seats[0].title;
  const duplicate = await orchestrator(async () => ({ text: JSON.stringify(parsed) })).assemble(input([models[0]]));
  assert(duplicate.provenance.mode === "deterministic-fallback", "duplicate role was accepted");
  parsed.seats[1].title = "";
  const empty = await orchestrator(async () => ({ text: JSON.stringify(parsed) })).assemble(input([models[0]]));
  assert(empty.provenance.mode === "deterministic-fallback", "empty role was accepted");
});

await check("Unsupported authorization or execution fields are rejected", async () => {
  const parsed = JSON.parse(validDecision);
  parsed.seats[0].executionAuthorization = true;
  const result = await orchestrator(async () => ({ text: JSON.stringify(parsed) })).assemble(input([models[0]]));
  assert(result.provenance.mode === "deterministic-fallback", "trusted authorization field was accepted");
});

const catalog: AgencyAgentsCatalog = {
  commit: "agency-commit",
  divisions: [{ id: "advisory", name: "Advisory", raw: {} }],
  runbooks: [],
  agents: [
    { division: "advisory", slug: "market-strategist", path: "advisory/market-strategist.md" },
    { division: "advisory", slug: "risk-reviewer", path: "advisory/risk-reviewer.md" },
    { division: "advisory", slug: "decision-synthesizer", path: "advisory/decision-synthesizer.md" },
  ],
};

let llmCompleted = false;
let capturedPrompt = "";
const council = new CouncilChiefOfStaff({
  paperclip: {
    probe: async () => ({
      id: "paperclip", name: "Paperclip", status: "available", detail: "ready", checkedAt: "now",
      metadata: { sourceCommit: "paperclip-commit" },
    }),
    listCompanies: async () => [{ id: "company-1", name: "Company", raw: {} }],
  },
  agencyAgents: {
    loadCatalog: async () => catalog,
    loadAgent: async (definition) => {
      assert(llmCompleted, "Agency mapping ran before assembly requirements were produced");
      return `Professional profile: ${definition.slug}`;
    },
  },
  listModels: () => models,
  invokeChiefOfStaff: async (prompt, selectedModel) => {
    capturedPrompt = prompt;
    assert(selectedModel.modelId === "reasoner-a", "unexpected Chief-of-Staff selection");
    llmCompleted = true;
    return { text: validDecision };
  },
  uuid: () => "shared-chief-plan",
});
const councilPlan = await council.assemble({ objective: "Plan a bounded market launch." });

await check("Agency Agents mapping remains downstream of LLM assembly", () => {
  assert(councilPlan.seats.every((seat) => seat.kind === "agency-agent"), "Agency roles were not mapped");
  assert(councilPlan.seats.every((seat) => seat.memberContext.startsWith("Professional profile")), "Agency profiles missing");
});

await check("Paperclip governance remains bounded Chief-of-Staff input and provenance", () => {
  assert(capturedPrompt.includes("Paperclip governance scope includes 1 configured company"), "Paperclip context missing from prompt");
  assert(councilPlan.provenance.paperclip.status === "consumed", "Paperclip provenance missing");
});

await check("Participant model assignment remains separate from Chief-of-Staff selection", () => {
  assert(councilPlan.provenance.chiefOfStaff?.modelId === "reasoner-a", "Chief-of-Staff model provenance missing");
  assert(new Set(councilPlan.seats.map((seat) => seat.assignedModel.modelId)).size > 1, "participant assignment reused only the Chief-of-Staff model");
});

await check("Council plan retains the existing bounded Runtime contract", () => {
  assert(councilPlan.seats.length === 3, "Council seat plan changed unexpectedly");
  assert(councilPlan.deliberation.maxRounds === 2, "Council round bound changed");
  assert(councilPlan.deliberation.stages.at(-1)?.participantSeatIds[0] === councilPlan.synthesizerSeatId, "synthesis contract changed");
  assert(capturedPrompt.includes("cannot execute tasks, tools, Skills, OpenClaw actions"), "non-execution boundary missing");
});

const fallbackCouncil = new CouncilChiefOfStaff({
  paperclip: {
    probe: async () => ({ id: "paperclip", name: "Paperclip", status: "unavailable", detail: "offline", checkedAt: "now" }),
    listCompanies: async () => [],
  },
  agencyAgents: { loadCatalog: async () => { throw new Error("offline"); }, loadAgent: async () => "" },
  listModels: () => models,
  invokeChiefOfStaff: async () => ({ text: "malformed" }),
  uuid: () => "fallback-plan",
});
const fallbackPlan = await fallbackCouncil.assemble({ objective: "Review a security and privacy architecture." });

await check("Existing deterministic P16 fallback still produces a valid bounded Council", () => {
  assert(fallbackPlan.provenance.chiefOfStaff?.mode === "deterministic-fallback", "fallback mode not recorded");
  assert(fallbackPlan.seats.length >= 3 && fallbackPlan.seats.length <= 7, "fallback seats outside bounds");
  assert(fallbackPlan.seats.filter((seat) => seat.id === fallbackPlan.synthesizerSeatId).length === 1, "fallback synthesizer invalid");
});

console.log(`\nPASS=${pass}`);
console.log(`FAIL=${fail}`);
if (fail > 0) throw new Error("Shared Chief of Staff verification failed.");
console.log("PASS: Shared Chief of Staff behavior");
