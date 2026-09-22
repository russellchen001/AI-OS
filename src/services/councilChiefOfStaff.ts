import {
  listAiCenterModels,
  type AiCenterModelChoice,
} from "./aiCenter";
import { AgencyAgentsAdapter } from "./integrations/agencyAgentsAdapter";
import {
  PAPERCLIP_SOURCE_COMMIT,
  PaperclipAdapter,
} from "./integrations/paperclipAdapter";
import type {
  CouncilAssemblyPlan,
  CouncilAssemblyProvenance,
  CouncilModelAssignment,
  CouncilSeat,
  CouncilSeatRequirement,
} from "../types/councilAssembly";
import type {
  AgencyAgentDefinition,
  AgencyAgentsCatalog,
  PaperclipCompany,
} from "../types/councilIntegrations";
import type {
  CouncilReconveneDecision,
  ExecutionBlocker,
} from "../types/councilExecution";

const MIN_DYNAMIC_SEATS = 3;
const MAX_DYNAMIC_SEATS = 7;
const MAX_PROFILE_CHARS = 12_000;

export type ChiefOfStaffDependencies = {
  paperclip: Pick<PaperclipAdapter, "probe" | "listCompanies">;
  agencyAgents: Pick<AgencyAgentsAdapter, "loadCatalog" | "loadAgent">;
  listModels: typeof listAiCenterModels;
  deriveRequirements?: (objective: string) => CouncilSeatRequirement[];
  uuid: () => string;
};

export type CouncilAssemblyRequest = {
  objective: string;
  maxSeats?: number;
};

function slugify(value: string, fallback: string): string {
  const slug = value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48);
  return slug || fallback;
}

function deriveRequirements(objective: string): CouncilSeatRequirement[] {
  const lower = objective.toLowerCase();
  const requirements: CouncilSeatRequirement[] = [];
  const add = (title: string, purpose: string, expertise: string[]): void => {
    requirements.push({
      id: slugify(title, `seat-${requirements.length + 1}`),
      title,
      purpose,
      requiredExpertise: expertise,
    });
  };

  if (/market|business|strategy|growth|revenue|customer|商业|市场|战略|增长/.test(lower)) {
    add("Market Strategist", "Assess market position, customers and strategic options.", [
      "market research",
      "business strategy",
      "customer insight",
    ]);
    add("Financial Analyst", "Test economics, investment and resource implications.", [
      "finance",
      "unit economics",
      "forecasting",
    ]);
  } else if (/security|privacy|risk|compliance|安全|隐私|合规/.test(lower)) {
    add("Security Specialist", "Identify security, privacy and compliance constraints.", [
      "security",
      "privacy",
      "compliance",
    ]);
    add("Systems Specialist", "Evaluate architecture and operational feasibility.", [
      "systems engineering",
      "reliability",
      "implementation",
    ]);
  } else if (/design|product|user|experience|产品|设计|用户/.test(lower)) {
    add("Product Strategist", "Clarify user value, scope and product outcomes.", [
      "product strategy",
      "prioritization",
      "user outcomes",
    ]);
    add("Experience Specialist", "Evaluate research, interaction and usability needs.", [
      "user research",
      "experience design",
      "accessibility",
    ]);
  } else {
    add("Domain Specialist", "Provide objective-specific professional analysis.", [
      "domain analysis",
      "decision support",
    ]);
    add("Implementation Specialist", "Turn the objective into practical options.", [
      "implementation",
      "operations",
    ]);
  }

  add("Risk Reviewer", "Challenge assumptions and surface disagreement and uncertainty.", [
    "risk analysis",
    "critical review",
  ]);
  requirements.push({
    id: "synthesizer",
    title: "Decision Synthesizer",
    purpose: "Reconcile expert work into a structured recommendation.",
    requiredExpertise: ["decision synthesis", "trade-off analysis"],
    synthesizer: true,
  });
  return requirements;
}

function tokens(values: string[]): Set<string> {
  return new Set(
    values
      .join(" ")
      .toLowerCase()
      .split(/[^a-z0-9]+/)
      .filter((token) => token.length > 2),
  );
}

function selectAgencyAgent(
  requirement: CouncilSeatRequirement,
  catalog: AgencyAgentsCatalog,
  usedPaths: Set<string>,
): AgencyAgentDefinition | undefined {
  const wanted = tokens([
    requirement.title,
    requirement.purpose,
    ...requirement.requiredExpertise,
  ]);
  return catalog.agents
    .filter((agent) => !usedPaths.has(agent.path))
    .map((agent) => {
      const available = tokens([agent.slug, agent.division]);
      const score = [...wanted].reduce(
        (total, token) => total + (available.has(token) ? 3 : agent.slug.includes(token) ? 1 : 0),
        0,
      );
      return { agent, score };
    })
    .sort(
      (left, right) =>
        right.score - left.score || left.agent.path.localeCompare(right.agent.path),
    )[0]?.agent;
}

function assignModels(
  requirements: CouncilSeatRequirement[],
  models: AiCenterModelChoice[],
  objective: string,
): CouncilModelAssignment[] {
  if (!models.length) throw new Error("No AI Center model is connected.");
  const localFirstObjective = /private|privacy|confidential|local|隐私|机密|本地/.test(
    objective.toLowerCase(),
  );
  const providerRepresentatives = [
    ...new Map(models.map((model) => [model.providerId, model])).values(),
  ];

  return requirements.map((requirement, index) => {
    const local = models.find((model) => model.providerId === "ollama");
    const preferred =
      (requirement.localFirst || localFirstObjective) && local
        ? local
        : providerRepresentatives[index % providerRepresentatives.length] ??
          models[index % models.length];
    return {
      ...preferred,
      rationale:
        providerRepresentatives.length === 1
          ? "Only one connected AI Center provider is available."
          : preferred.providerId === "ollama" && (requirement.localFirst || localFirstObjective)
            ? "Local-first assignment for privacy-sensitive work."
            : "Assigned dynamically to preserve useful provider diversity.",
      fallbackModelIds: models
        .filter((model) => model.modelId !== preferred.modelId)
        .map((model) => model.modelId),
    };
  });
}

function paperclipSummary(companies: PaperclipCompany[], available: boolean): string {
  if (!available) return "Paperclip unavailable; built-in governance bounds applied.";
  if (!companies.length) {
    return "Paperclip is healthy with no configured companies; an independent bounded council was assembled.";
  }
  return `Paperclip governance scope includes ${companies.length} configured company context(s).`;
}

export class CouncilChiefOfStaff {
  constructor(
    private readonly dependencies: ChiefOfStaffDependencies = {
      paperclip: new PaperclipAdapter(),
      agencyAgents: new AgencyAgentsAdapter(),
      listModels: listAiCenterModels,
      uuid: () => crypto.randomUUID(),
    },
  ) {}

  evaluateExecutionBlocker(
    blocker: ExecutionBlocker,
    assemblyPlan?: CouncilAssemblyPlan,
  ): CouncilReconveneDecision {
    const requested = [...new Set(blocker.suggestedExpertise.map((item) => item.trim()).filter(Boolean))]
      .slice(0, 4);
    const existingSeatIds = (assemblyPlan?.seats ?? [])
      .filter((seat) =>
        requested.some((expertise) =>
          [seat.title, ...seat.requiredExpertise]
            .join(" ")
            .toLowerCase()
            .includes(expertise.toLowerCase()),
        ),
      )
      .map((seat) => seat.id)
      .slice(0, 4);
    const newSeatRequirements = requested.filter((expertise) =>
      !existingSeatIds.some((seatId) =>
        assemblyPlan?.seats.find((seat) => seat.id === seatId)?.requiredExpertise
          .some((item) => item.toLowerCase().includes(expertise.toLowerCase())),
      ),
    );
    return {
      action: requested.length || blocker.invalidatedAssumptions.length
        ? "reconvene-experts"
        : "require-user-decision",
      reason: blocker.reason,
      existingSeatIds,
      newSeatRequirements,
      executionContext: {
        councilSessionId: blocker.councilSessionId,
        recommendationId: blocker.recommendationId,
        taskId: blocker.taskId,
        reason: blocker.reason,
        observedReality: blocker.observedReality,
        invalidatedAssumptions: blocker.invalidatedAssumptions.slice(0, 8),
      },
      requiresUserApproval: true,
    };
  }

  async assemble(request: CouncilAssemblyRequest): Promise<CouncilAssemblyPlan> {
    const objective = request.objective.trim();
    if (!objective) throw new Error("Council objective is required.");
    const models = this.dependencies.listModels();
    if (!models.length) throw new Error("No AI Center model is connected.");

    let paperclipAvailable = false;
    let paperclipCommit: string | undefined = PAPERCLIP_SOURCE_COMMIT;
    let companies: PaperclipCompany[] = [];
    try {
      const probe = await this.dependencies.paperclip.probe();
      paperclipAvailable = probe.status === "available";
      paperclipCommit =
        typeof probe.metadata?.sourceCommit === "string"
          ? probe.metadata.sourceCommit
          : paperclipCommit;
      if (paperclipAvailable) companies = await this.dependencies.paperclip.listCompanies();
    } catch {
      paperclipAvailable = false;
    }
    const governanceContext = paperclipSummary(companies, paperclipAvailable);

    let catalog: AgencyAgentsCatalog | undefined;
    try {
      catalog = await this.dependencies.agencyAgents.loadCatalog();
    } catch {
      catalog = undefined;
    }

    const seatLimit = Math.min(
      MAX_DYNAMIC_SEATS,
      Math.max(MIN_DYNAMIC_SEATS, request.maxSeats ?? MAX_DYNAMIC_SEATS),
    );
    const derived = (
      this.dependencies.deriveRequirements?.(objective) ?? deriveRequirements(objective)
    ).slice(0, seatLimit);
    let requirements = derived;
    if (!requirements.some((requirement) => requirement.synthesizer)) {
      requirements = [
        ...requirements.slice(0, seatLimit - 1),
        {
          id: "synthesizer",
          title: "Decision Synthesizer",
          purpose: "Reconcile expert work into a structured recommendation.",
          requiredExpertise: ["decision synthesis", "trade-off analysis"],
          synthesizer: true,
        },
      ];
    }
    const synthesizerIndex = requirements.findIndex((requirement) => requirement.synthesizer);
    requirements = requirements.map((requirement, index) => ({
      ...requirement,
      id: `${slugify(requirement.id || requirement.title, `seat-${index + 1}`)}-${index + 1}`,
      synthesizer: index === synthesizerIndex,
    }));
    const assignments = assignModels(requirements, models, objective);
    const usedPaths = new Set<string>();
    const seats: CouncilSeat[] = [];

    for (let index = 0; index < requirements.length; index += 1) {
      const requirement = requirements[index];
      const definition = catalog
        ? selectAgencyAgent(requirement, catalog, usedPaths)
        : undefined;
      if (definition) usedPaths.add(definition.path);
      let profile: string | undefined;
      if (definition) {
        try {
          profile = await this.dependencies.agencyAgents.loadAgent(definition);
        } catch {
          profile = undefined;
        }
      }
      const agencyBacked = Boolean(definition && profile);
      seats.push({
        id: requirement.id,
        title: requirement.title,
        purpose: requirement.purpose,
        requiredExpertise: requirement.requiredExpertise,
        kind: agencyBacked ? "agency-agent" : "builtin",
        source: agencyBacked
          ? {
              kind: "agency-agent",
              sourceId: definition!.slug,
              sourceCommit: catalog!.commit,
              sourcePath: definition!.path,
              provenanceReferences: [
                `agency-agents:${definition!.division}/${definition!.slug}`,
              ],
            }
          : {
              kind: "builtin",
              sourceId: "chief-of-staff-fallback",
              fallback: true,
              provenanceReferences: ["chief-of-staff:fallback-role-context"],
            },
        assignedModel: assignments[index],
        memberContext: agencyBacked
          ? profile!.slice(0, MAX_PROFILE_CHARS)
          : [
              `Professional role: ${requirement.title}`,
              `Purpose: ${requirement.purpose}`,
              `Required expertise: ${requirement.requiredExpertise.join(", ")}`,
              "Use evidence, state assumptions, preserve uncertainty, and stay within this role.",
            ].join("\n"),
      });
    }

    const synthesizerSeatId =
      seats[requirements.findIndex((requirement) => requirement.synthesizer)]?.id ??
      seats.at(-1)!.id;
    const expertSeatIds = seats
      .filter((seat) => seat.id !== synthesizerSeatId)
      .map((seat) => seat.id);
    const selectedAgentPaths = seats
      .map((seat) => seat.source.sourcePath)
      .filter((path): path is string => Boolean(path));
    const provenance: CouncilAssemblyProvenance = {
      paperclip: {
        status: paperclipAvailable ? "consumed" : "fallback",
        sourceCommit: paperclipCommit,
        companyIds: companies.map((company) => company.id),
        detail: governanceContext,
      },
      agencyAgents: {
        status: selectedAgentPaths.length ? "consumed" : "fallback",
        sourceCommit: catalog?.commit,
        selectedAgentPaths,
        detail: selectedAgentPaths.length
          ? `${selectedAgentPaths.length} real Agency Agents profiles instantiated.`
          : "Agency Agents unavailable; generic bounded expert contexts were used.",
      },
      references: [
        paperclipAvailable ? "paperclip:api" : "paperclip:fallback",
        ...seats.flatMap((seat) => seat.source.provenanceReferences),
      ],
    };

    return {
      id: this.dependencies.uuid(),
      objective,
      mode: "dynamic",
      rationale: `Chief of Staff selected objective-specific complementary expertise. ${governanceContext}`,
      seats,
      deliberation: {
        maxRounds: 2,
        stages: [
          {
            id: "independent-analysis",
            title: "Independent expert analysis",
            participantSeatIds: expertSeatIds,
          },
          {
            id: "cross-review",
            title: "Cross-review and critique",
            participantSeatIds: expertSeatIds,
          },
          {
            id: "final-synthesis",
            title: "Structured recommendation synthesis",
            participantSeatIds: [synthesizerSeatId],
          },
        ],
      },
      synthesizerSeatId,
      provenance,
    };
  }
}

export function createCouncilChiefOfStaff(): CouncilChiefOfStaff {
  return new CouncilChiefOfStaff();
}
