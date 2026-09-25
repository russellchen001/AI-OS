import {
  answerThroughAiCenter,
  listAiCenterModels,
  type AiCenterModelChoice,
} from "./aiCenter";
import {
  ChiefOfStaffOrchestrator,
} from "./chiefOfStaffOrchestrator";
import { AgencyAgentsAdapter } from "./integrations/agencyAgentsAdapter";
import {
  PaperclipAdapter,
} from "./integrations/paperclipAdapter";
import { assignArenaModels } from "./arenaModelAssignment";
import { resolveUserConstraintsForSeats } from "./arenaConstraintBinding";
import { routeArenaStrategy } from "./arenaStrategyRouter";
import type {
  ArenaAssemblyPlan,
  ArenaModelConstraint,
  ArenaPlanRequest,
  ArenaSeatRequirement,
  ArenaSeatRole,
  ArenaStrategy,
} from "../types/arena";

const MIN_ARENA_SEATS = 2;
const MAX_ARENA_SEATS = 12;

type SharedAssemblyDecision =
  Awaited<ReturnType<ChiefOfStaffOrchestrator["assemble"]>>;

function slugify(value: string, fallback: string): string {
  const slug = value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48);

  return slug || fallback;
}

function deterministicRequirements(
  strategies: ArenaStrategy[],
): Array<{
  id: string;
  title: string;
  purpose: string;
  requiredExpertise: string[];
  synthesizer?: boolean;
  localFirst?: boolean;
}> {
  if (strategies.includes("game")) {
    return [
      {
        id: "rules-controller",
        title: "Rules Controller",
        purpose: "Maintain the game rules, turn order and bounded shared state.",
        requiredExpertise: ["rules", "game state", "moderation"],
      },
      {
        id: "player-a",
        title: "Player A",
        purpose: "Participate according to the assigned role and visible information.",
        requiredExpertise: ["reasoning", "social interaction"],
      },
      {
        id: "player-b",
        title: "Player B",
        purpose: "Participate according to the assigned role and visible information.",
        requiredExpertise: ["reasoning", "social interaction"],
      },
      {
        id: "observer",
        title: "Arena Observer",
        purpose: "Evaluate rule adherence and interaction quality.",
        requiredExpertise: ["evaluation", "observation"],
        synthesizer: true,
      },
    ];
  }

  if (strategies.includes("simulation")) {
    return [
      {
        id: "scenario-actor-a",
        title: "Scenario Actor A",
        purpose: "Represent one important actor or force in the simulated scenario.",
        requiredExpertise: ["scenario reasoning", "domain analysis"],
      },
      {
        id: "scenario-actor-b",
        title: "Scenario Actor B",
        purpose: "Represent a complementary actor or force in the simulation.",
        requiredExpertise: ["scenario reasoning", "counterfactual analysis"],
      },
      {
        id: "environment",
        title: "Environment Controller",
        purpose: "Maintain explicit scenario assumptions and environment changes.",
        requiredExpertise: ["systems thinking", "simulation"],
      },
      {
        id: "evaluator",
        title: "Simulation Evaluator",
        purpose: "Evaluate outcomes, uncertainty and scenario limitations.",
        requiredExpertise: ["evaluation", "uncertainty"],
        synthesizer: true,
      },
    ];
  }

  if (
    strategies.includes("compare") ||
    strategies.includes("debate") ||
    strategies.includes("competition")
  ) {
    return [
      {
        id: "contender-a",
        title: "Contender A",
        purpose: "Develop the strongest well-supported first position.",
        requiredExpertise: ["analysis", "argumentation"],
      },
      {
        id: "contender-b",
        title: "Contender B",
        purpose: "Develop an independent competing position and challenge assumptions.",
        requiredExpertise: ["analysis", "critical reasoning"],
      },
      {
        id: "judge",
        title: "Arena Judge",
        purpose: "Evaluate the competing outputs against explicit criteria.",
        requiredExpertise: ["evaluation", "comparison", "reasoning"],
        synthesizer: true,
      },
    ];
  }

  return [
    {
      id: "specialist-a",
      title: "Specialist A",
      purpose: "Contribute one complementary expert perspective.",
      requiredExpertise: ["domain analysis", "problem solving"],
    },
    {
      id: "specialist-b",
      title: "Specialist B",
      purpose: "Contribute a distinct complementary expert perspective.",
      requiredExpertise: ["critical reasoning", "implementation"],
    },
    {
      id: "synthesizer",
      title: "Arena Synthesizer",
      purpose: "Combine useful contributions without hiding disagreement.",
      requiredExpertise: ["synthesis", "evaluation"],
      synthesizer: true,
    },
  ];
}

function inferRole(
  title: string,
  synthesizer: boolean | undefined,
): ArenaSeatRole {
  const lower = title.toLowerCase();

  if (/moderator/.test(lower)) return "moderator";
  if (/judge/.test(lower)) return "judge";
  if (/evaluator/.test(lower)) return "evaluator";
  if (/observer/.test(lower)) return "observer";
  if (/rules/.test(lower)) return "rules-controller";
  if (/environment/.test(lower)) return "environment";
  if (synthesizer) return "synthesizer";
  return "participant";
}


async function paperclipContext(): Promise<string> {
  try {
    const adapter = new PaperclipAdapter();
    const probe = await adapter.probe();

    if (probe.status !== "available") {
      return "Paperclip governance reference unavailable; use bounded Arena fallback governance.";
    }

    const companies = await adapter.listCompanies();

    return companies.length
      ? `Paperclip governance context is available with ${companies.length} configured company scope(s).`
      : "Paperclip is available; no company-specific governance scope is configured.";
  } catch {
    return "Paperclip governance reference unavailable; use bounded Arena fallback governance.";
  }
}

async function agencyContext(): Promise<string> {
  try {
    const adapter = new AgencyAgentsAdapter();
    const catalog = await adapter.loadCatalog();

    return (
      `${catalog.agents.length} Agency Agents professional roles across ` +
      `${catalog.divisions.length} divisions are available for role-shaping context.`
    );
  } catch {
    return "Agency Agents unavailable; use bounded generic Arena role contexts.";
  }
}

function toArenaSeats(
  decision: SharedAssemblyDecision,
  constraints: ArenaModelConstraint[],
  maxSeats: number,
): ArenaSeatRequirement[] {
  const requirements = decision.requirements.slice(0, maxSeats);
  const roles = requirements.map((requirement) =>
    inferRole(requirement.title, requirement.synthesizer),
  );
  const resolvedConstraints =
    resolveUserConstraintsForSeats(roles, constraints);

  return requirements.map((requirement, index) => ({
    id: `${slugify(
      requirement.title,
      `arena-seat-${index + 1}`,
    )}-${index + 1}`,
    title: requirement.title,
    purpose: requirement.purpose,
    requiredExpertise: [...requirement.requiredExpertise],
    role: roles[index],
    modelConstraint:
      resolvedConstraints[index] ?? { mode: "auto" },
  }));
}

export class ArenaChiefOfStaff {
  async assemble(request: ArenaPlanRequest): Promise<ArenaAssemblyPlan> {
    const objective = request.objective.trim();

    if (!objective) {
      throw new Error("Arena objective is required.");
    }

    const availableModels =
      request.availableModels.length
        ? request.availableModels
        : listAiCenterModels();

    if (!availableModels.length) {
      throw new Error("No AI Center model is connected.");
    }

    const strategyPlan = routeArenaStrategy(
      objective,
      request.userModelConstraints ?? [],
    );

    const maxSeats = Math.min(
      Math.max(
        request.maxParticipants ??
          strategyPlan.suggestedMaxParticipants,
        MIN_ARENA_SEATS,
      ),
      MAX_ARENA_SEATS,
    );

    const deterministicFallback =
      deterministicRequirements(strategyPlan.strategies)
        .slice(0, maxSeats);

    const orchestrator = new ChiefOfStaffOrchestrator({
      invokeModel: answerThroughAiCenter,
      timeoutMs: 20_000,
    });

    const decision = await orchestrator.assemble({
      objective:
        `AI Arena objective: ${objective}\n` +
        `Internal strategies: ${strategyPlan.strategies.join(", ")}\n` +
        "Choose the minimum sufficient Arena lineup. " +
        "Do not execute tasks or Skills. " +
        "Do not override explicit user model constraints. " +
        "Include moderator/judge/evaluator/rules roles only when they add value.",
      domain: "arena",
      availableModels,
      governanceContext: await paperclipContext(),
      agencyContext: await agencyContext(),
      constraints: {
        minSeats: Math.max(
          MIN_ARENA_SEATS,
          Math.min(
            strategyPlan.suggestedMinParticipants,
            maxSeats,
          ),
        ),
        maxSeats,
        localFirst: /private|privacy|confidential|local|隐私|机密|本地/.test(
          objective.toLowerCase(),
        ),
      },
      deterministicFallback: () => deterministicFallback,
    });

    let seats = toArenaSeats(
      decision,
      strategyPlan.userModelConstraints,
      maxSeats,
    );

    if (seats.length < MIN_ARENA_SEATS) {
      seats = deterministicFallback
        .slice(0, maxSeats)
        .map((requirement, index) => ({
          id: `${slugify(
            requirement.id || requirement.title,
            `arena-seat-${index + 1}`,
          )}-${index + 1}`,
          title: requirement.title,
          purpose: requirement.purpose,
          requiredExpertise: [...requirement.requiredExpertise],
          role: inferRole(
            requirement.title,
            requirement.synthesizer,
          ),
          modelConstraint: { mode: "auto" },
        }));
    }

    const modelAssignments = assignArenaModels(
      seats,
      availableModels,
    );

    const fallbackRoles = seats.map((seat) => seat.role);
    const fallbackConstraints = resolveUserConstraintsForSeats(
      fallbackRoles,
      strategyPlan.userModelConstraints,
    );
    seats = seats.map((seat, index) => ({
      ...seat,
      modelConstraint:
        fallbackConstraints[index] ?? { mode: "auto" },
    }));

    const chiefOfStaff =
      decision.provenance.mode === "llm-chief-of-staff"
        ? {
            mode: decision.provenance.mode,
            providerId: decision.provenance.model.providerId,
            providerInstanceId:
              decision.provenance.model.providerInstanceId,
            modelId: decision.provenance.model.modelId,
            selectionRationale:
              decision.provenance.selectionRationale,
            attempts: decision.provenance.attempts,
          }
        : {
            mode: decision.provenance.mode,
            selectionRationale:
              decision.provenance.selectionRationale,
            fallbackReason:
              decision.provenance.fallbackReason,
            attempts: decision.provenance.attempts,
          };

    return {
      id: crypto.randomUUID(),
      objective,
      strategyPlan,
      seats,
      modelAssignments,
      rationale: decision.rationale,
      chiefOfStaff,
    };
  }
}

export function createArenaChiefOfStaff(): ArenaChiefOfStaff {
  return new ArenaChiefOfStaff();
}

export function listArenaModels(): AiCenterModelChoice[] {
  return listAiCenterModels();
}
