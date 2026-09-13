// Mirrors the Rust contract in src-tauri/src/cognitive_distillation.
// Field names are camelCase on both sides because the Rust types are
// #[serde(rename_all = "camelCase")].

export type ProfileStatus = "draft" | "reviewed" | "active" | "archived";

export type SubjectKind =
  | "self-profile"
  | "private-person"
  | "public-person"
  | "historical-person"
  | "fictional-character";

export type NarrativeOrigin = "work" | "persona" | "combined";

export type ClaimCategory =
  | "identity"
  | "domain-expertise"
  | "knowledge-model"
  | "decision-patterns"
  | "reasoning-frameworks"
  | "preferences"
  | "constraints"
  | "behavioral-patterns"
  | "communication-style"
  | "representative-examples";

export type CognitiveClaim = {
  claimId: string;
  statement: string;
  confirmed: boolean;
  confidence: number;
  evidenceIds: string[];
  contradictoryEvidenceIds: string[];
};

/** Creator prose. Never a claim — it carries no evidence links. */
export type DraftNarrativeSection = {
  origin: NarrativeOrigin;
  adapter: string;
  text: string;
};

export type RevisionRecord = {
  revision: number;
  reason: string;
  evidenceBundleId: string;
};

export type PersonDistillationProfile = {
  profileId: string;
  revision: number;
  status: ProfileStatus;
  subjectKind: SubjectKind;
  identity: CognitiveClaim[];
  domainExpertise: CognitiveClaim[];
  knowledgeModel: CognitiveClaim[];
  decisionPatterns: CognitiveClaim[];
  reasoningFrameworks: CognitiveClaim[];
  preferences: CognitiveClaim[];
  constraints: CognitiveClaim[];
  behavioralPatterns: CognitiveClaim[];
  communicationStyle: CognitiveClaim[];
  representativeExamples: CognitiveClaim[];
  unclassifiedClaims: CognitiveClaim[];
  draftNarrative: DraftNarrativeSection[];
  evidenceBundleId: string;
  contradictions: string[];
  revisionHistory: RevisionRecord[];
};

export type ProfileSummary = {
  profileId: string;
  latestRevision: number;
  activeRevision: number | null;
  status: ProfileStatus;
};

export type PersonProfileView = {
  profile: PersonDistillationProfile;
  /** Not always the newest revision: a new draft leaves the previous one live. */
  activeRevision: number | null;
  revisionCount: number;
};

/** `category: null` rejects the claim. Rejection is a legitimate outcome. */
export type ClaimDecision = {
  claimId: string;
  category: ClaimCategory | null;
  correctedStatement?: string | null;
};

export type ReviewDecisions = {
  reviewer: string;
  decisions: ClaimDecision[];
};

export type RunnablePersonaSkill = {
  skillId: string;
  profileId: string;
  profileRevision: number;
  instructions: string;
  examples: string[];
  rawMediaAssets: string[];
};

export const CLAIM_CATEGORIES: Array<{ value: ClaimCategory; label: string }> = [
  { value: "identity", label: "Identity" },
  { value: "domain-expertise", label: "Domain expertise" },
  { value: "knowledge-model", label: "Knowledge model" },
  { value: "decision-patterns", label: "Decision patterns" },
  { value: "reasoning-frameworks", label: "Reasoning frameworks" },
  { value: "preferences", label: "Preferences" },
  { value: "constraints", label: "Constraints" },
  { value: "behavioral-patterns", label: "Behavioral patterns" },
  { value: "communication-style", label: "Communication style" },
  { value: "representative-examples", label: "Representative examples" },
];

export const CATEGORISED_FIELDS: Array<{
  key: keyof PersonDistillationProfile;
  label: string;
}> = [
  { key: "identity", label: "Identity" },
  { key: "domainExpertise", label: "Domain expertise" },
  { key: "knowledgeModel", label: "Knowledge model" },
  { key: "decisionPatterns", label: "Decision patterns" },
  { key: "reasoningFrameworks", label: "Reasoning frameworks" },
  { key: "preferences", label: "Preferences" },
  { key: "constraints", label: "Constraints" },
  { key: "behavioralPatterns", label: "Behavioral patterns" },
  { key: "communicationStyle", label: "Communication style" },
  { key: "representativeExamples", label: "Representative examples" },
];
