import {
  buildPersonPersonaSkill,
  getPersonProfile,
  listPersonProfiles,
} from "./personProfiles";
import {
  distilledPersonaToCouncilContext,
} from "./councilMemberContext";
import {
  createCouncilChiefOfStaff,
  type CouncilAssemblyRequest,
} from "./councilChiefOfStaff";
import { CouncilRuntime } from "./councilRuntime";
import type {
  CouncilAssemblyPlan,
  CouncilModelAssignment,
  CouncilSeat,
} from "../types/councilAssembly";
import type {
  CouncilMember,
} from "../types/council";
import type {
  CouncilRunRequest,
  CouncilRunResult,
} from "../types/councilRuntime";
import type {
  PersonProfileView,
  ProfileSummary,
  RunnablePersonaSkill,
} from "../types/personProfile";

export type SimulationCouncilRequest = {
  objective: string;
  profileId: string;
  members?: CouncilMember[];
  maxSeats?: number;
  callbacks?: CouncilRunRequest["callbacks"];
};

export type SimulationCouncilPrepared = {
  plan: CouncilAssemblyPlan;
  profile: PersonProfileView;
};

export type SimulationCouncilDependencies = {
  listProfiles: typeof listPersonProfiles;
  getProfile: typeof getPersonProfile;
  buildPersonaSkill: typeof buildPersonPersonaSkill;
  chiefOfStaff: ReturnType<typeof createCouncilChiefOfStaff>;
  runtime: CouncilRuntime;
};

function averageConfidence(source: ReturnType<typeof distilledPersonaToCouncilContext>): number {
  return typeof source.confidence === "number" ? source.confidence : 0;
}

function selectPersonaModel(plan: CouncilAssemblyPlan): CouncilModelAssignment {
  const expert = plan.seats.find(
    (seat) => seat.id !== plan.synthesizerSeatId,
  );
  const fallback = expert ?? plan.seats[0];
  if (!fallback) {
    throw new Error("Simulation Council has no available AI Center model assignment.");
  }

  return {
    ...fallback.assignedModel,
    rationale:
      "Assigned by the existing Dynamic Council model allocator for the simulated-person seat. " +
      fallback.assignedModel.rationale,
  };
}

function personaSeatContext(
  profileId: string,
  sourceContent: string,
): string {
  return [
    `You are the simulated-person seat for validated distilled profile ${profileId}.`,
    "",
    "This is a bounded behavioral simulation, not factual access to the person's mind or future.",
    "Reason only from the validated human-reviewed profile below.",
    "Distinguish evidence from inference.",
    "State uncertainty explicitly.",
    "When evidence supports multiple plausible responses, preserve the alternatives.",
    "Do not invent motives, memories, facts, preferences or capabilities not grounded in the profile.",
    "",
    sourceContent,
  ].join("\n");
}

export class SimulationCouncilService {
  constructor(
    private readonly dependencies: SimulationCouncilDependencies = {
      listProfiles: listPersonProfiles,
      getProfile: getPersonProfile,
      buildPersonaSkill: buildPersonPersonaSkill,
      chiefOfStaff: createCouncilChiefOfStaff(),
      runtime: new CouncilRuntime(),
    },
  ) {}

  listAvailableProfiles(): Promise<ProfileSummary[]> {
    return this.dependencies.listProfiles();
  }

  cancel(): Promise<void> {
    return this.dependencies.runtime.cancel();
  }

  async prepare(
    request: Omit<SimulationCouncilRequest, "members" | "callbacks">,
  ): Promise<SimulationCouncilPrepared> {
    const objective = request.objective.trim();
    const profileId = request.profileId.trim();

    if (!objective) {
      throw new Error("Simulation objective is required.");
    }
    if (!profileId) {
      throw new Error("A distilled person profile is required.");
    }

    const [view, skill] = await Promise.all([
      this.dependencies.getProfile(profileId),
      this.dependencies.buildPersonaSkill(profileId),
    ]);

    // This adapter is intentionally the policy gate:
    // it rejects draft/non-active/non-current/non-human-reviewed profiles.
    const personaSource = distilledPersonaToCouncilContext(view, skill);

    const assemblyRequest: CouncilAssemblyRequest = {
      objective: [
        objective,
        "",
        "Simulation requirement:",
        `Evaluate plausible responses/decisions of distilled profile ${profileId}.`,
        "Build supporting expert seats needed to challenge the simulation, inspect assumptions,",
        "surface alternative scenarios, triggers, uncertainty and evidence that would change the forecast.",
      ].join("\n"),
      maxSeats: request.maxSeats,
    };

    const basePlan = await this.dependencies.chiefOfStaff.assemble(
      assemblyRequest,
    );

    if (!basePlan.seats.length) {
      throw new Error("Chief of Staff produced no supporting Council seats.");
    }

    const personaSeatId = `simulated-person-${profileId}`;
    const personaSeat: CouncilSeat = {
      id: personaSeatId,
      title: `Simulated Person — ${profileId}`,
      purpose:
        "Model plausible responses using only the current human-reviewed P15 distilled profile.",
      requiredExpertise: [
        "behavioral simulation",
        "decision-pattern reconstruction",
        "evidence-grounded scenario reasoning",
      ],
      kind: "distilled-persona",
      source: {
        kind: "distilled-persona",
        sourceId: profileId,
        provenanceReferences:
          personaSource.provenance.provenanceReferences ?? [],
      },
      assignedModel: selectPersonaModel(basePlan),
      memberContext: personaSeatContext(
        profileId,
        personaSource.content,
      ),
    };

    const synthesizerIndex = basePlan.seats.findIndex(
      (seat) => seat.id === basePlan.synthesizerSeatId,
    );

    const seats =
      synthesizerIndex >= 0
        ? [
            ...basePlan.seats.slice(0, synthesizerIndex),
            personaSeat,
            ...basePlan.seats.slice(synthesizerIndex),
          ]
        : [...basePlan.seats, personaSeat];

    const expertSeatIds = seats
      .filter((seat) => seat.id !== basePlan.synthesizerSeatId)
      .map((seat) => seat.id);

    const provenanceReferences = [
      ...new Set([
        ...basePlan.provenance.references,
        ...(personaSource.provenance.provenanceReferences ?? []),
      ]),
    ];

    const plan: CouncilAssemblyPlan = {
      ...basePlan,
      mode: "simulation",
      objective,
      rationale: [
        basePlan.rationale,
        `P15 distilled profile ${profileId} is participating as a first-class simulated-person seat.`,
        "Supporting experts challenge the simulated response and preserve alternative scenarios and uncertainty.",
      ].join(" "),
      seats,
      deliberation: {
        maxRounds: Math.min(2, Math.max(1, basePlan.deliberation.maxRounds)),
        stages: [
          {
            id: "independent-analysis",
            title: "Independent expert and persona analysis",
            participantSeatIds: expertSeatIds,
          },
          {
            id: "cross-review",
            title: "Expert cross-examination of the simulation",
            participantSeatIds: expertSeatIds,
          },
          {
            id: "final-synthesis",
            title: "Scenario and forecast synthesis",
            participantSeatIds: [basePlan.synthesizerSeatId],
          },
        ],
      },
      provenance: {
        ...basePlan.provenance,
        references: provenanceReferences,
      },
      simulation: {
        profileId,
        profileRevision: view.profile.revision,
        evidenceBundleId: view.profile.evidenceBundleId,
        personaSeatId,
        humanReviewed: true,
        confidence: averageConfidence(personaSource),
        provenanceReferences:
          personaSource.provenance.provenanceReferences ?? [],
      },
    };

    return {
      plan,
      profile: view,
    };
  }

  async run(request: SimulationCouncilRequest): Promise<CouncilRunResult> {
    const prepared = await this.prepare(request);

    return this.dependencies.runtime.run({
      prompt: request.objective,
      members: request.members ?? [],
      assemblyPlan: prepared.plan,
      context: {
        distilledProfileIds: [request.profileId],
      },
      callbacks: request.callbacks,
    });
  }
}

export function createSimulationCouncilService(): SimulationCouncilService {
  return new SimulationCouncilService();
}
