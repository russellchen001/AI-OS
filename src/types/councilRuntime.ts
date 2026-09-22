import type { AgencyAgentDefinition } from "./councilIntegrations";
import type {
  CouncilMember,
  CouncilSession,
  CouncilSourceMetadata,
  CouncilStepResult,
} from "./council";
import type { ProviderId } from "./provider";
import type {
  CouncilAssemblyPlan,
  CouncilRecommendation,
} from "./councilAssembly";

export type CouncilContextSourceKind =
  | "paperclip"
  | "agency-agent"
  | "linco-bridge"
  | "distilled-persona"
  | "provided";

export type CouncilContextSource = {
  id: string;
  kind: CouncilContextSourceKind;
  title: string;
  content: string;
  provenance: CouncilSourceMetadata;
  confidence?: number;
  humanReviewed?: boolean;
};

export type CouncilRunContext = {
  includePaperclip?: boolean;
  includeLincoBridge?: boolean;
  agencyAgents?: AgencyAgentDefinition[];
  distilledProfileIds?: string[];
  sources?: CouncilContextSource[];
};

export type CouncilRuntimeCallbacks = {
  onCouncilStarted?: (sessionId: string, steps: CouncilStepResult[]) => void;
  onMemberStarted?: (step: CouncilStepResult) => void;
  onProviderChanged?: (
    memberId: string,
    providerId: ProviderId,
    attempt: number,
    total: number,
  ) => void;
  onChunk?: (memberId: string, providerId: ProviderId, text: string) => void;
  onMemberCompleted?: (step: CouncilStepResult) => void;
  onMemberFailed?: (step: CouncilStepResult) => void;
  onCouncilCompleted?: (result: CouncilRunResult) => void;
};

export type CouncilRunRequest = {
  prompt: string;
  members: CouncilMember[];
  assemblyPlan?: CouncilAssemblyPlan;
  context?: CouncilRunContext;
  callbacks?: CouncilRuntimeCallbacks;
};

export type CouncilRunResult = {
  session: CouncilSession;
  finalAnswer: string;
  steps: CouncilStepResult[];
  recommendation?: CouncilRecommendation;
  metadata: {
    integrationIds: string[];
    provenanceReferences: string[];
    contextSources: CouncilContextSource[];
  };
};
