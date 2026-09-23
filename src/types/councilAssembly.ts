export type CouncilSeatKind =
  | "builtin"
  | "agency-agent"
  | "distilled-persona";

export type CouncilMemberSource = {
  kind: CouncilSeatKind;
  sourceId: string;
  sourceCommit?: string;
  sourcePath?: string;
  fallback?: boolean;
  provenanceReferences: string[];
};

export type CouncilModelAssignment = {
  providerId: string;
  providerInstanceId: string;
  modelId: string;
  label: string;
  rationale: string;
  fallbackModelIds: string[];
};

export type CouncilSeatRequirement = {
  id: string;
  title: string;
  purpose: string;
  requiredExpertise: string[];
  localFirst?: boolean;
  synthesizer?: boolean;
};

export type CouncilSeat = {
  id: string;
  title: string;
  purpose: string;
  requiredExpertise: string[];
  kind: CouncilSeatKind;
  source: CouncilMemberSource;
  assignedModel: CouncilModelAssignment;
  memberContext: string;
};

export type CouncilDeliberationStage = {
  id: "independent-analysis" | "cross-review" | "final-synthesis";
  title: string;
  participantSeatIds: string[];
};

export type CouncilDeliberationPlan = {
  maxRounds: number;
  stages: CouncilDeliberationStage[];
};

export type CouncilAssemblyProvenance = {
  chiefOfStaff?: {
    mode: "llm-chief-of-staff" | "deterministic-fallback";
    providerId?: string;
    providerInstanceId?: string;
    modelId?: string;
    selectionRationale: string;
    fallbackReason?: string;
    attempts: number;
  };
  paperclip: {
    status: "consumed" | "fallback";
    sourceCommit?: string;
    companyIds: string[];
    detail: string;
  };
  agencyAgents: {
    status: "consumed" | "fallback";
    sourceCommit?: string;
    selectedAgentPaths: string[];
    detail: string;
  };
  references: string[];
};

export type CouncilSimulationMetadata = {
  profileId: string;
  profileRevision: number;
  evidenceBundleId: string;
  personaSeatId: string;
  humanReviewed: true;
  confidence: number;
  provenanceReferences: string[];
};

export type CouncilSimulationReport = {
  evidence: string[];
  assumptions: string[];
  likelyResponses: string[];
  alternativeScenarios: string[];
  triggerConditions: string[];
  counterarguments: string[];
  confidence: string[];
  uncertainty: string[];
  evidenceThatWouldChangeForecast: string[];
  recommendedResponse: string[];
};

export type CouncilAssemblyPlan = {
  id: string;
  objective: string;
  mode: "dynamic" | "simulation" | "legacy-fallback";
  rationale: string;
  seats: CouncilSeat[];
  deliberation: CouncilDeliberationPlan;
  synthesizerSeatId: string;
  provenance: CouncilAssemblyProvenance;
  simulation?: CouncilSimulationMetadata;
};

export type CouncilRecommendation = {
  id: string;
  councilSessionId: string;
  summary: string;
  recommendedPlan: string[];
  rationale: string[];
  disagreements: string[];
  risks: string[];
  assumptions: string[];
  uncertainty: string[];
  provenance: string[];
  createdAt: number;
  simulationReport?: CouncilSimulationReport;
};
