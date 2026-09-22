import type {
  ProviderId,
} from "./provider";
import type {
  CouncilAssemblyPlan,
  CouncilMemberSource,
  CouncilRecommendation,
} from "./councilAssembly";
import type { CouncilExecutionState } from "./councilExecution";

export type CouncilRole =
  | "planner"
  | "engineer"
  | "researcher"
  | "critic"
  | "judge";

export type CouncilMember = {
  id: string;
  role?: CouncilRole;
  name: string;
  icon: string;
  providerId: ProviderId;
  enabled: boolean;
  systemPrompt: string;
  kind?: "builtin" | "agency-agent" | "distilled-persona";
  source?: CouncilSourceMetadata;
};

export type CouncilSourceMetadata = {
  sourceId: string;
  sourceCommit?: string;
  sourcePath?: string;
  provenanceReferences?: string[];
};

export type CouncilStepStatus =
  | "idle"
  | "running"
  | "done"
  | "error"
  | "skipped";

export type CouncilStepResult = {
  id?: string;
  role: string;
  seatId?: string;
  stage?: "independent-analysis" | "cross-review" | "final-synthesis";
  memberName: string;
  providerId: ProviderId;
  modelId?: string;
  source?: CouncilMemberSource;
  status: CouncilStepStatus;
  output: string;
  error?: string;
  startedAt?: number;
  completedAt?: number;
};

export type CouncilSession = {
  id: string;
  title: string;
  prompt: string;
  createdAt: number;
  updatedAt: number;
  favorite: boolean;
  steps: CouncilStepResult[];
  finalAnswer: string;
  assemblyPlan?: CouncilAssemblyPlan;
  recommendation?: CouncilRecommendation;
  execution?: CouncilExecutionState;
  metadata?: CouncilSessionMetadata;
};

export type CouncilSessionMetadata = {
  integrationIds: string[];
  provenanceReferences: string[];
  assemblyPlanId?: string;
};
