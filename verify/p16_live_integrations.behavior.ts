import { AgencyAgentsAdapter } from "../src/services/integrations/agencyAgentsAdapter";
import { LincoBridgeAdapter } from "../src/services/integrations/lincoBridgeAdapter";
import { PaperclipAdapter } from "../src/services/integrations/paperclipAdapter";
import { CouncilChiefOfStaff } from "../src/services/councilChiefOfStaff";

declare const process: {
  argv: string[];
  exitCode?: number;
};

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function isConnectionFailure(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return /fetch failed|ECONNREFUSED|connection refused|could not connect/i.test(message);
}

async function verifyAgencyAgents(): Promise<void> {
  const catalog = await new AgencyAgentsAdapter().loadCatalog();
  assert(catalog.commit.length === 40, "Agency Agents source commit is invalid.");
  assert(catalog.divisions.length > 0, "Agency Agents divisions catalog is empty.");
  assert(catalog.runbooks.length > 0, "Agency Agents runbooks catalog is empty.");
  assert(catalog.agents.length > 0, "Agency Agents role index is empty.");
  console.log(
    `PASS: Agency Agents live catalog divisions=${catalog.divisions.length} runbooks=${catalog.runbooks.length} agents=${catalog.agents.length}`,
  );
}

async function verifyDynamicAssembly(): Promise<void> {
  const paperclip = new PaperclipAdapter("http://127.0.0.1:3100");
  const paperclipProbe = await paperclip.probe();
  if (paperclipProbe.status !== "available") {
    throw new Error(`Paperclip unavailable: ${paperclipProbe.detail}`);
  }
  const agencyAgents = new AgencyAgentsAdapter();
  const chief = new CouncilChiefOfStaff({
    paperclip,
    agencyAgents,
    listModels: () => [
      {
        providerId: "ollama",
        providerInstanceId: "ollama-local",
        modelId: "live-local-model",
        label: "Live validation model slot",
      },
    ],
    uuid: () => "live-dynamic-assembly",
  });
  const plan = await chief.assemble({
    objective: "Create a market entry strategy with financial and risk review.",
  });
  assert(plan.mode === "dynamic", "Chief of Staff did not produce a dynamic plan.");
  assert(plan.provenance.paperclip.status === "consumed", "Paperclip was not consumed.");
  assert(plan.provenance.agencyAgents.status === "consumed", "Agency roles were not consumed.");
  assert(
    plan.seats.every((seat) => seat.kind === "agency-agent" && Boolean(seat.source.sourcePath)),
    "A live dynamic seat lacks a real Agency Agent definition.",
  );
  assert(
    plan.seats.every((seat) => seat.assignedModel.modelId === "live-local-model"),
    "Single-model assignment fallback is invalid.",
  );
  console.log(
    `PASS: Dynamic assembly live paperclip=${plan.provenance.paperclip.status} agencySeats=${plan.seats.length} modelAssignments=${plan.seats.length}`,
  );
}

async function verifyPaperclip(): Promise<void> {
  const adapter = new PaperclipAdapter("http://127.0.0.1:3100");
  const probe = await adapter.probe();
  if (probe.status !== "available") {
    throw new Error(`Paperclip unavailable: ${probe.detail}`);
  }
  const companies = await adapter.listCompanies();
  assert(
    companies.every((company) => company.id.length > 0 && company.name.length > 0),
    "Paperclip company normalization produced an invalid company.",
  );
  console.log(
    `PASS: Paperclip live adapter reachable=true companies=${companies.length} zeroCompanyHandled=${companies.length === 0}`,
  );
}

async function verifyLincoBridge(): Promise<void> {
  const adapter = new LincoBridgeAdapter("http://127.0.0.1:3300");
  const probe = await adapter.probe();
  if (probe.status !== "available") {
    throw new Error(`Linco Bridge unavailable: ${probe.detail}`);
  }
  const sessions = await adapter.listSessions();
  assert(
    sessions.every((session) => session.id.length > 0),
    "Linco Bridge session normalization produced an invalid session.",
  );
  const status = await adapter.getOpenClawBridgeStatus();
  assert(
    typeof status === "object" && status !== null && "connected" in status,
    "Linco Bridge OpenClaw status response is invalid.",
  );
  console.log(
    `PASS: Linco Bridge live adapter reachable=true sessions=${sessions.length} openClawConnected=${String((status as { connected: unknown }).connected)}`,
  );
}

const mode = process.argv[2];
const run =
  mode === "agency-agents"
    ? verifyAgencyAgents
    : mode === "paperclip"
      ? verifyPaperclip
      : mode === "linco-bridge"
        ? verifyLincoBridge
        : mode === "dynamic-assembly"
          ? verifyDynamicAssembly
        : undefined;

if (!run) {
  console.error("FAIL: unknown P16 live integration mode");
  process.exitCode = 1;
} else {
  try {
    await run();
  } catch (error) {
    if (isConnectionFailure(error)) {
      console.log(`SKIP: ${mode} live service is not reachable.`);
      process.exitCode = 2;
    } else {
      console.error(
        `FAIL: ${mode}: ${error instanceof Error ? error.message : String(error)}`,
      );
      process.exitCode = 1;
    }
  }
}
