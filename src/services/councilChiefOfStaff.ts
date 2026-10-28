import {
  answerThroughAiCenter,
  listAiCenterModels,
  type AiCenterModelChoice,
} from "./aiCenter";
import {
  ChiefOfStaffOrchestrator,
  type ChiefOfStaffModelInvoker,
} from "./chiefOfStaffOrchestrator";
import { AgencyAgentsAdapter } from "./integrations/agencyAgentsAdapter";
import {
  extractUserModelConstraints,
  withoutModelMentions,
} from "./arenaStrategyRouter";
import { preferredSynthesizerIndex } from "./synthesizerChoice";
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
  invokeChiefOfStaff?: ChiefOfStaffModelInvoker;
  chiefOfStaffTimeoutMs?: number;
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
  objective: string,
): AgencyAgentDefinition | undefined {
  const taskTerms = tokens([objective]);
  const wanted = tokens([
    requirement.purpose,
    ...requirement.requiredExpertise,
  ]);
  for (const generic of ["specialist", "expert", "reviewer", "synthesizer", "implementation", "analysis", "professional", "role"]) {
    wanted.delete(generic);
  }
  return catalog.agents
    .filter((agent) => !usedPaths.has(agent.path))
    .map((agent) => {
      const available = tokens([agent.slug, agent.division]);
      // Seat boilerplate (domain, review, operations...) is not task-domain evidence.
      if (![...available].some((token) => taskTerms.has(token))) {
        return { agent, score: 0 };
      }
      const score = [...wanted].reduce(
        (total, token) => total + (available.has(token) ? 3 : agent.slug.includes(token) ? 1 : 0),
        0,
      );
      return { agent, score };
    })
    .filter(({ score }) => score > 0)
    .sort(
      (left, right) =>
        right.score - left.score || left.agent.path.localeCompare(right.agent.path),
    )[0]?.agent;
}

type NamedParticipants = {
  models: AiCenterModelChoice[];
  /** The model the user asked to synthesize ("由 X 汇总"), if any. */
  synthesizer?: AiCenterModelChoice;
};

/** Connected models the user explicitly named as participants, in order. */
function namedParticipantModels(
  objective: string,
  models: AiCenterModelChoice[],
): NamedParticipants {
  const named: NamedParticipants = { models: [] };
  for (const constraint of extractUserModelConstraints(objective, models)) {
    if (constraint.mode !== "pinned" || !constraint.modelId) continue;
    const model = models.find((candidate) =>
      candidate.providerInstanceId === constraint.providerInstanceId &&
      candidate.modelId === constraint.modelId);
    if (!model) continue;
    if (!named.models.includes(model)) named.models.push(model);
    if (constraint.targetRole === "synthesizer") named.synthesizer ??= model;
  }
  return named;
}

/** Named models in seat order: experts first, the synthesizer's model last. */
function seatOrder(named: NamedParticipants): AiCenterModelChoice[] {
  const synthesizer = named.synthesizer ??
    named.models[preferredSynthesizerIndex(named.models)];
  return [
    ...named.models.filter((model) => model !== synthesizer),
    ...(synthesizer ? [synthesizer] : []),
  ];
}

/** Adds expert seats before the synthesizer until `count` seats exist. */
function withSeatCapacity(
  requirements: CouncilSeatRequirement[],
  count: number,
): CouncilSeatRequirement[] {
  const result = [...requirements];
  while (result.length < count) {
    const synthesizerAt = result.findIndex((requirement) => requirement.synthesizer);
    result.splice(synthesizerAt < 0 ? result.length : synthesizerAt, 0, {
      id: `additional-expert-${result.length + 1}`,
      title: `Additional Expert ${result.length + 1}`,
      purpose: "Contribute another independent, objective-specific perspective.",
      requiredExpertise: ["independent analysis", "critical review"],
    });
  }
  return result;
}

function assignModels(
  requirements: CouncilSeatRequirement[],
  models: AiCenterModelChoice[],
  objective: string,
  named: AiCenterModelChoice[] = [],
): CouncilModelAssignment[] {
  if (!models.length) throw new Error("No generation-verified model is available. Verify a model in My AI.");
  if (named.length) {
    // Every named model takes exactly one seat and is never silently replaced;
    // the last named model in seat order goes to the synthesizer seat.
    const synthesizerAt = requirements.findIndex((requirement) => requirement.synthesizer);
    const experts = named.slice(0, -1);
    const synthesizerModel = named.at(-1);
    let expertIndex = 0;
    return requirements.map((requirement, index) => {
      const pinned = index === synthesizerAt
        ? synthesizerModel
        : experts[expertIndex++];
      if (!pinned) {
        return assignModels([requirement], models, objective)[0];
      }
      return {
        ...pinned,
        rationale: "User-named participant; no silent substitution.",
        fallbackModelIds: [],
      };
    });
  }
  const localFirstObjective = /private|privacy|confidential|local|隐私|机密|本地/.test(
    objective.toLowerCase(),
  );
  const providerRepresentatives = [
    ...new Map(models.map((model) => [model.providerId, model])).values(),
  ];

  return requirements.map((requirement, index) => {
    const localOnly = requirement.localFirst || localFirstObjective;
    const eligibleModels = localOnly
      ? models.filter((model) => model.providerId === "omlx")
      : models;
    if (!eligibleModels.length) {
      throw new Error("No local AI Center model is connected for this local-only Council.");
    }
    const eligibleRepresentatives = [
      ...new Map(eligibleModels.map((model) => [model.providerId, model])).values(),
    ];
    const local = eligibleModels.find((model) => model.providerId === "omlx");
    const preferred =
      localOnly && local
        ? eligibleModels[index % eligibleModels.length]
        : requirement.synthesizer
          ? eligibleRepresentatives[preferredSynthesizerIndex(eligibleRepresentatives)]
          : eligibleRepresentatives[index % eligibleRepresentatives.length] ??
          providerRepresentatives[index % providerRepresentatives.length] ??
          eligibleModels[index % eligibleModels.length] ?? models[index % models.length];
    return {
      ...preferred,
      rationale:
        localOnly && eligibleModels.length > 1
          ? "Local-only assignment distributed across available local models for independent perspectives."
          : providerRepresentatives.length === 1
          ? "Only one connected AI Center provider is available."
          : preferred.providerId === "omlx" && (requirement.localFirst || localFirstObjective)
            ? "Local-first assignment for privacy-sensitive work."
            : "Assigned dynamically to preserve useful provider diversity.",
      fallbackModelIds: eligibleModels
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

function agencySummary(catalog?: AgencyAgentsCatalog): string {
  if (!catalog) return "Agency Agents unavailable; built-in role contexts remain available.";
  const divisions = catalog.divisions.slice(0, 24).map((division) => division.name).join(", ");
  return `${catalog.agents.length} professional roles across ${catalog.divisions.length} divisions. Divisions: ${divisions}`;
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
    if (!models.length) throw new Error("No generation-verified model is available. Verify a model in My AI.");

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

    // Model names identify participants; they are not task-domain evidence.
    const taskText = withoutModelMentions(objective, models);
    const named = seatOrder(namedParticipantModels(objective, models));
    const localFirst = /private|privacy|confidential|local|隐私|机密|本地/.test(
      taskText.toLowerCase(),
    );
    const namedCloud = named.filter((model) => model.providerId !== "omlx");
    if (localFirst && namedCloud.length) {
      throw new Error(
        `This Council is local-only but names cloud models: ${namedCloud.map((model) => model.label).join(", ")}. ` +
        "AI-OS will not silently drop a named model; remove it or allow cloud reasoning.",
      );
    }
    if (named.length > MAX_DYNAMIC_SEATS) {
      throw new Error(
        `The Council can seat at most ${MAX_DYNAMIC_SEATS} models; ${named.length} were named.`,
      );
    }
    const seatFloor = Math.max(MIN_DYNAMIC_SEATS, named.length);
    // A Council of named models is exactly those models (at least the minimum).
    const seatLimit = named.length >= MIN_DYNAMIC_SEATS
      ? named.length
      : Math.min(
          MAX_DYNAMIC_SEATS,
          Math.max(seatFloor, request.maxSeats ?? MAX_DYNAMIC_SEATS),
        );
    const deterministicFallback = (): CouncilSeatRequirement[] => withSeatCapacity(
      this.dependencies.deriveRequirements?.(taskText) ?? deriveRequirements(taskText),
      seatFloor,
    ).slice(0, seatLimit);
    const orchestrator = new ChiefOfStaffOrchestrator({
      invokeModel: this.dependencies.invokeChiefOfStaff ?? answerThroughAiCenter,
      timeoutMs: this.dependencies.chiefOfStaffTimeoutMs ?? 20_000,
    });
    const assemblyDecision = await orchestrator.assemble({
      objective,
      domain: "council",
      availableModels: localFirst ? models.filter((model) => model.providerId === "omlx") : models,
      governanceContext,
      agencyContext: agencySummary(catalog),
      constraints: {
        minSeats: seatFloor,
        maxSeats: seatLimit,
        localFirst,
      },
      deterministicFallback,
    });
    const derived = assemblyDecision.requirements.slice(0, seatLimit);
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
    requirements = withSeatCapacity(requirements, seatFloor);
    const synthesizerIndex = requirements.findIndex((requirement) => requirement.synthesizer);
    requirements = requirements.map((requirement, index) => ({
      ...requirement,
      id: `${slugify(requirement.id || requirement.title, `seat-${index + 1}`)}-${index + 1}`,
      synthesizer: index === synthesizerIndex,
    }));
    const assignments = assignModels(requirements, models, taskText, named);
    const usedPaths = new Set<string>();
    const seats: CouncilSeat[] = [];

    for (let index = 0; index < requirements.length; index += 1) {
      const requirement = requirements[index];
      const definition = catalog
        ? selectAgencyAgent(requirement, catalog, usedPaths, taskText)
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
      chiefOfStaff: assemblyDecision.provenance.mode === "llm-chief-of-staff"
        ? {
            mode: assemblyDecision.provenance.mode,
            providerId: assemblyDecision.provenance.model.providerId,
            providerInstanceId: assemblyDecision.provenance.model.providerInstanceId,
            modelId: assemblyDecision.provenance.model.modelId,
            selectionRationale: assemblyDecision.provenance.selectionRationale,
            attempts: assemblyDecision.provenance.attempts,
          }
        : {
            mode: assemblyDecision.provenance.mode,
            selectionRationale: assemblyDecision.provenance.selectionRationale,
            fallbackReason: assemblyDecision.provenance.fallbackReason,
            attempts: assemblyDecision.provenance.attempts,
          },
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
        assemblyDecision.provenance.mode === "llm-chief-of-staff"
          ? `chief-of-staff:ai-center:${assemblyDecision.provenance.model.providerId}:${assemblyDecision.provenance.model.modelId}`
          : "chief-of-staff:deterministic-fallback",
        paperclipAvailable ? "paperclip:api" : "paperclip:fallback",
        ...seats.flatMap((seat) => seat.source.provenanceReferences),
      ],
    };

    return {
      id: this.dependencies.uuid(),
      objective,
      mode: "dynamic",
      rationale: `${assemblyDecision.rationale} ${governanceContext}`,
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
