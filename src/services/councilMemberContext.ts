import {
  AgencyAgentsAdapter,
  PaperclipAdapter,
} from "./councilIntegrations";
import { AGENCY_AGENTS_SOURCE_COMMIT } from "./integrations/agencyAgentsAdapter";
import {
  buildPersonPersonaSkill,
  getPersonProfile,
} from "./personProfiles";
import type { AgencyAgentDefinition } from "../types/councilIntegrations";
import type { CouncilRunContext, CouncilContextSource } from "../types/councilRuntime";
import type { CognitiveClaim, PersonProfileView, RunnablePersonaSkill } from "../types/personProfile";

export type CouncilContextDependencies = {
  paperclip: Pick<PaperclipAdapter, "probe" | "listCompanies">;
  agencyAgents: Pick<AgencyAgentsAdapter, "loadAgent">;
  getProfile: typeof getPersonProfile;
  buildPersonaSkill: typeof buildPersonPersonaSkill;
};

const defaultDependencies = (): CouncilContextDependencies => ({
  paperclip: new PaperclipAdapter(),
  agencyAgents: new AgencyAgentsAdapter(),
  getProfile: getPersonProfile,
  buildPersonaSkill: buildPersonPersonaSkill,
});

function boundedJson(value: unknown, limit = 12_000): string {
  const text = JSON.stringify(value, null, 2);
  return text.length > limit ? `${text.slice(0, limit)}\n…` : text;
}

async function withLocalIntegrationTimeout<T>(work: Promise<T>): Promise<T> {
  let timeoutId: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timeoutId = setTimeout(
      () => reject(new Error("Council integration context timed out.")),
      2_500,
    );
  });
  try {
    return await Promise.race([work, timeout]);
  } finally {
    if (timeoutId !== undefined) clearTimeout(timeoutId);
  }
}

export async function agencyAgentToCouncilContext(
  definition: AgencyAgentDefinition,
  adapter: Pick<AgencyAgentsAdapter, "loadAgent"> = new AgencyAgentsAdapter(),
): Promise<CouncilContextSource> {
  const profile = await adapter.loadAgent(definition);
  return {
    id: `agency-agent:${definition.slug}`,
    kind: "agency-agent",
    title: `Agency Agent: ${definition.slug}`,
    content: profile,
    provenance: {
      sourceId: definition.slug,
      sourceCommit: AGENCY_AGENTS_SOURCE_COMMIT,
      sourcePath: definition.path,
      provenanceReferences: [
        `agency-agents:${definition.division}/${definition.slug}`,
      ],
    },
  };
}

const claimFields = [
  "identity",
  "domainExpertise",
  "knowledgeModel",
  "decisionPatterns",
  "reasoningFrameworks",
  "preferences",
  "constraints",
  "behavioralPatterns",
  "communicationStyle",
  "representativeExamples",
] as const;

function activeClaims(view: PersonProfileView): CognitiveClaim[] {
  return claimFields.flatMap((field) => view.profile[field]);
}

export function distilledPersonaToCouncilContext(
  view: PersonProfileView,
  skill: RunnablePersonaSkill,
): CouncilContextSource {
  if (
    view.profile.status !== "active" ||
    view.activeRevision !== view.profile.revision ||
    skill.profileId !== view.profile.profileId ||
    skill.profileRevision !== view.profile.revision
  ) {
    throw new Error("Council can only consume the current human-reviewed active profile revision.");
  }

  const claims = activeClaims(view);
  const evidenceIds = [...new Set(claims.flatMap((claim) => claim.evidenceIds))];
  const confidence = claims.length
    ? claims.reduce((sum, claim) => sum + claim.confidence, 0) / claims.length
    : 0;

  return {
    id: `distilled-persona:${view.profile.profileId}@${view.profile.revision}`,
    kind: "distilled-persona",
    title: `Validated distilled profile: ${view.profile.profileId}`,
    content: boundedJson({
      profileId: view.profile.profileId,
      revision: view.profile.revision,
      subjectKind: view.profile.subjectKind,
      instructions: skill.instructions,
      examples: skill.examples,
      claims: claimFields.reduce<Record<string, CognitiveClaim[]>>((result, field) => {
        result[field] = view.profile[field];
        return result;
      }, {}),
      contradictions: view.profile.contradictions,
    }),
    confidence,
    humanReviewed: true,
    provenance: {
      sourceId: view.profile.profileId,
      provenanceReferences: [
        `person-profile:${view.profile.profileId}@${view.profile.revision}`,
        `evidence-bundle:${view.profile.evidenceBundleId}`,
        ...evidenceIds.map((id) => `evidence:${id}`),
      ],
    },
  };
}

async function loadPaperclipContext(
  adapter: CouncilContextDependencies["paperclip"],
): Promise<CouncilContextSource | undefined> {
  const probe = await adapter.probe();
  if (probe.status !== "available") return undefined;
  const companies = await adapter.listCompanies();
  return {
    id: "paperclip:governance",
    kind: "paperclip",
    title: "Paperclip orchestration and governance context",
    content: boundedJson({ probe, companies }),
    provenance: {
      sourceId: "paperclip",
      sourceCommit:
        typeof probe.metadata?.sourceCommit === "string"
          ? probe.metadata.sourceCommit
          : undefined,
      provenanceReferences: ["paperclip:api"],
    },
  };
}

export async function resolveCouncilContext(
  context: CouncilRunContext = {},
  dependencies: CouncilContextDependencies = defaultDependencies(),
): Promise<CouncilContextSource[]> {
  const sources = (context.sources ?? []).filter(
    (source) => source.kind !== "linco-bridge",
  );
  const optionalLoads: Array<Promise<CouncilContextSource | undefined>> = [];

  if (context.includePaperclip !== false) {
    optionalLoads.push(
      withLocalIntegrationTimeout(loadPaperclipContext(dependencies.paperclip)).catch(
        () => undefined,
      ),
    );
  }
  sources.push(
    ...(await Promise.all(optionalLoads)).filter(
      (source): source is CouncilContextSource => Boolean(source),
    ),
  );

  for (const definition of context.agencyAgents ?? []) {
    sources.push(await agencyAgentToCouncilContext(definition, dependencies.agencyAgents));
  }

  for (const profileId of context.distilledProfileIds ?? []) {
    const [view, skill] = await Promise.all([
      dependencies.getProfile(profileId),
      dependencies.buildPersonaSkill(profileId),
    ]);
    sources.push(distilledPersonaToCouncilContext(view, skill));
  }

  return sources;
}

export function formatCouncilContext(sources: CouncilContextSource[]): string {
  if (!sources.length) return "No configured integration context was available.";
  return sources
    .map((source) =>
      [
        `## ${source.title}`,
        `Source: ${source.kind} (${source.provenance.sourceId})`,
        source.humanReviewed === undefined
          ? undefined
          : `Human reviewed: ${source.humanReviewed ? "yes" : "no"}`,
        source.confidence === undefined
          ? undefined
          : `Confidence: ${source.confidence.toFixed(3)}`,
        "",
        source.content,
      ]
        .filter((line): line is string => line !== undefined)
        .join("\n"),
    )
    .join("\n\n---\n\n");
}
