import {
  answerThroughAiCenter,
  type AiCenterModelChoice,
  type AiCenterResponse,
} from "./aiCenter";

const MAX_OUTPUT_CHARS = 16_000;
const MAX_RATIONALE_CHARS = 800;
const MAX_TITLE_CHARS = 80;
const MAX_PURPOSE_CHARS = 320;
const MAX_EXPERTISE_ITEMS = 8;
const MAX_EXPERTISE_CHARS = 80;
const DEFAULT_TIMEOUT_MS = 20_000;
const MAX_MODEL_ATTEMPTS = 2;

export type ChiefOfStaffDomain = "council" | "arena";

export type ChiefOfStaffSeatRequirement = {
  id: string;
  title: string;
  purpose: string;
  requiredExpertise: string[];
  synthesizer?: boolean;
  localFirst?: boolean;
};

export type ChiefOfStaffAssemblyInput = {
  objective: string;
  domain: ChiefOfStaffDomain;
  availableModels: AiCenterModelChoice[];
  governanceContext: string;
  agencyContext: string;
  distilledContext?: string;
  constraints: {
    minSeats: number;
    maxSeats: number;
    localFirst: boolean;
  };
  deterministicFallback: () => ChiefOfStaffSeatRequirement[];
};

export type ChiefOfStaffSelection = {
  model: AiCenterModelChoice;
  rationale: string;
};

export type ChiefOfStaffAssemblyResult = {
  requirements: ChiefOfStaffSeatRequirement[];
  rationale: string;
  provenance:
    | {
        mode: "llm-chief-of-staff";
        model: AiCenterModelChoice;
        selectionRationale: string;
        attempts: number;
      }
    | {
        mode: "deterministic-fallback";
        selectionRationale: string;
        fallbackReason: string;
        attempts: number;
      };
};

export type ChiefOfStaffModelInvoker = (
  prompt: string,
  selectedModel: AiCenterModelChoice,
) => Promise<Pick<AiCenterResponse, "text">>;

export type ChiefOfStaffOrchestratorDependencies = {
  invokeModel: ChiefOfStaffModelInvoker;
  timeoutMs: number;
};

function isLocalModel(model: AiCenterModelChoice): boolean {
  return model.providerId === "ollama" || model.providerId === "omlx";
}

function modelKey(model: AiCenterModelChoice): string {
  return [model.providerId, model.providerInstanceId, model.modelId].join(":");
}

export function selectChiefOfStaffModels(
  models: AiCenterModelChoice[],
  localFirst: boolean,
): ChiefOfStaffSelection[] {
  const seen = new Set<string>();
  const eligible = models.filter((model) => {
    if (
      !model.providerId.trim() ||
      !model.providerInstanceId.trim() ||
      !model.modelId.trim()
    ) {
      return false;
    }
    const key = modelKey(model);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
  const ranked = eligible
    .map((model, index) => ({ model, index }))
    .sort((left, right) => {
      if (!localFirst) return left.index - right.index;
      const locality = Number(isLocalModel(right.model)) - Number(isLocalModel(left.model));
      return locality || left.index - right.index;
    })
    .slice(0, MAX_MODEL_ATTEMPTS);

  return ranked.map(({ model }) => ({
    model,
    rationale: localFirst && isLocalModel(model)
      ? "Selected an available local AI Center model for the privacy-sensitive orchestration request."
      : "Selected from AI Center's connected, enabled model order; capability metadata is not yet exposed for deeper ranking.",
  }));
}

function bounded(value: unknown, maxLength: number): string | undefined {
  if (typeof value !== "string") return undefined;
  const normalized = value.trim();
  return normalized && normalized.length <= maxLength ? normalized : undefined;
}

function hasOnlyKeys(value: Record<string, unknown>, allowed: string[]): boolean {
  return Object.keys(value).every((key) => allowed.includes(key));
}

function parseStructuredDecision(
  text: string,
  input: ChiefOfStaffAssemblyInput,
): { requirements: ChiefOfStaffSeatRequirement[]; rationale: string } {
  if (text.length > MAX_OUTPUT_CHARS) throw new Error("Chief of Staff output exceeded the size limit.");
  const unwrapped = text.trim().replace(/^```(?:json)?\s*/i, "").replace(/\s*```$/, "");
  let parsed: unknown;
  try {
    parsed = JSON.parse(unwrapped);
  } catch {
    throw new Error("Chief of Staff returned malformed JSON.");
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("Chief of Staff output must be an object.");
  }
  const root = parsed as Record<string, unknown>;
  if (!hasOnlyKeys(root, ["seats", "rationale"]) || !Array.isArray(root.seats)) {
    throw new Error("Chief of Staff output contains an invalid schema.");
  }
  if (root.seats.length < input.constraints.minSeats || root.seats.length > input.constraints.maxSeats) {
    throw new Error("Chief of Staff seat count is outside policy bounds.");
  }
  const titles = new Set<string>();
  const ids = new Set<string>();
  const requirements = root.seats.map((value, index) => {
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      throw new Error("Chief of Staff seat must be an object.");
    }
    const seat = value as Record<string, unknown>;
    if (!hasOnlyKeys(seat, ["id", "title", "purpose", "requiredExpertise", "synthesizer", "localFirst"])) {
      throw new Error("Chief of Staff seat contains an unsupported field.");
    }
    const title = bounded(seat.title, MAX_TITLE_CHARS);
    const purpose = bounded(seat.purpose, MAX_PURPOSE_CHARS);
    if (!title || !purpose || !Array.isArray(seat.requiredExpertise)) {
      throw new Error("Chief of Staff seat is missing a bounded role definition.");
    }
    const expertise = seat.requiredExpertise.map((item) => bounded(item, MAX_EXPERTISE_CHARS));
    if (
      expertise.length === 0 ||
      expertise.length > MAX_EXPERTISE_ITEMS ||
      expertise.some((item) => !item)
    ) {
      throw new Error("Chief of Staff expertise is outside policy bounds.");
    }
    const normalizedTitle = title.toLowerCase();
    if (titles.has(normalizedTitle)) throw new Error("Chief of Staff roles must be unique.");
    titles.add(normalizedTitle);
    const requestedId = bounded(seat.id, MAX_TITLE_CHARS) ?? `seat-${index + 1}`;
    const normalizedId = requestedId.toLowerCase();
    if (ids.has(normalizedId)) throw new Error("Chief of Staff seat ids must be unique.");
    ids.add(normalizedId);
    if (seat.synthesizer !== undefined && typeof seat.synthesizer !== "boolean") {
      throw new Error("Chief of Staff synthesizer flag must be boolean.");
    }
    if (seat.localFirst !== undefined && typeof seat.localFirst !== "boolean") {
      throw new Error("Chief of Staff local-first flag must be boolean.");
    }
    return {
      id: requestedId,
      title,
      purpose,
      requiredExpertise: expertise as string[],
      synthesizer: seat.synthesizer === true,
      localFirst: seat.localFirst === true,
    };
  });
  const synthesizers = requirements.filter((requirement) => requirement.synthesizer);
  if (synthesizers.length !== 1 || requirements.length - synthesizers.length < 1) {
    throw new Error("Chief of Staff output requires experts and exactly one synthesizer.");
  }
  const rationale = bounded(root.rationale, MAX_RATIONALE_CHARS);
  if (!rationale) throw new Error("Chief of Staff rationale is missing or too long.");
  return { requirements, rationale };
}

function buildPrompt(input: ChiefOfStaffAssemblyInput): string {
  const distilled = input.distilledContext?.trim();
  return [
    "You are the AI-OS shared Chief of Staff. Assemble an advisory team only.",
    "You cannot execute tasks, tools, Skills, OpenClaw actions, or grant authorization.",
    "Do not assign provider ids or model ids to participant seats.",
    `Domain: ${input.domain}`,
    `Objective: ${input.objective.slice(0, 4_000)}`,
    `Policy: ${input.constraints.minSeats}-${input.constraints.maxSeats} total seats, exactly one synthesizer, at least one expert.`,
    `Local-first required: ${input.constraints.localFirst}`,
    `Governance context: ${input.governanceContext.slice(0, 2_000)}`,
    `Agency role context: ${input.agencyContext.slice(0, 4_000)}`,
    distilled ? `Optional distilled context: ${distilled.slice(0, 2_000)}` : "",
    "Return JSON only with exactly these root fields: seats, rationale.",
    "Each seat may contain only: id, title, purpose, requiredExpertise, synthesizer, localFirst.",
  ].filter(Boolean).join("\n");
}

async function withTimeout<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<T>((_, reject) => {
        timer = setTimeout(() => reject(new Error("Chief of Staff invocation timed out.")), timeoutMs);
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

export class ChiefOfStaffOrchestrator {
  constructor(
    private readonly dependencies: ChiefOfStaffOrchestratorDependencies = {
      invokeModel: answerThroughAiCenter,
      timeoutMs: DEFAULT_TIMEOUT_MS,
    },
  ) {}

  async assemble(input: ChiefOfStaffAssemblyInput): Promise<ChiefOfStaffAssemblyResult> {
    const candidates = selectChiefOfStaffModels(
      input.availableModels,
      input.constraints.localFirst,
    );
    const errors: string[] = [];
    for (const [index, candidate] of candidates.entries()) {
      try {
        const response = await withTimeout(
          this.dependencies.invokeModel(buildPrompt(input), candidate.model),
          this.dependencies.timeoutMs,
        );
        const decision = parseStructuredDecision(response.text, input);
        return {
          ...decision,
          provenance: {
            mode: "llm-chief-of-staff",
            model: candidate.model,
            selectionRationale: candidate.rationale,
            attempts: index + 1,
          },
        };
      } catch (error) {
        errors.push(error instanceof Error ? error.message : String(error));
      }
    }
    return {
      requirements: input.deterministicFallback(),
      rationale: "Deterministic P16 assembly retained as the safe fallback.",
      provenance: {
        mode: "deterministic-fallback",
        selectionRationale: candidates[0]?.rationale ?? "No eligible Chief-of-Staff model was available.",
        fallbackReason: errors.join(" | ") || "No eligible Chief-of-Staff model was available.",
        attempts: candidates.length,
      },
    };
  }
}
