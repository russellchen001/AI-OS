import type { AiCenterModelChoice } from "./aiCenter";
import type {
  ArenaModelAssignment,
  ArenaModelConstraint,
  ArenaSeatRequirement,
} from "../types/arena";
import { arenaModelKey } from "../types/arena";

function normalize(value: string | undefined): string {
  return (value ?? "").trim().toLowerCase();
}

function matchesLabel(
  model: AiCenterModelChoice,
  label: string | undefined,
): boolean {
  const wanted = normalize(label);
  if (!wanted) return false;

  return [
    model.providerId,
    model.providerInstanceId,
    model.modelId,
    model.label,
  ]
    .join(" ")
    .toLowerCase()
    .includes(wanted);
}

function matchesConstraint(
  model: AiCenterModelChoice,
  constraint: Exclude<ArenaModelConstraint, { mode: "auto" }>,
): boolean {
  if (
    constraint.providerId &&
    model.providerId !== constraint.providerId
  ) {
    return false;
  }

  if (
    constraint.providerInstanceId &&
    model.providerInstanceId !== constraint.providerInstanceId
  ) {
    return false;
  }

  if (
    constraint.modelId &&
    model.modelId !== constraint.modelId
  ) {
    return false;
  }

  if (
    constraint.label &&
    !matchesLabel(model, constraint.label)
  ) {
    return false;
  }

  return true;
}

function isUnresolvedPinnedConstraint(
  constraint: ArenaModelConstraint,
): constraint is Extract<
  ArenaModelConstraint,
  {
    mode: "pinned";
    label: string;
    providerId?: never;
    providerInstanceId?: never;
    modelId?: never;
  }
> {
  return (
    constraint.mode === "pinned" &&
    !constraint.providerId &&
    !constraint.providerInstanceId &&
    !constraint.modelId &&
    Boolean(constraint.label?.trim())
  );
}

function resolveUnresolvedPinnedModel(
  models: AiCenterModelChoice[],
  constraint: Extract<
    ArenaModelConstraint,
    {
      mode: "pinned";
      label: string;
      providerId?: never;
      providerInstanceId?: never;
      modelId?: never;
    }
  >,
  seatTitle: string,
): AiCenterModelChoice {
  const matches = models.filter((model) =>
    matchesConstraint(model, constraint),
  );

  if (matches.length === 0) {
    throw new Error(
      `Pinned Arena model "${constraint.label}" is unavailable for ` +
      `"${seatTitle}". AI-OS will not silently replace an explicitly ` +
      "pinned model.",
    );
  }

  if (matches.length > 1) {
    throw new Error(
      `Pinned Arena model "${constraint.label}" is ambiguous for ` +
      `"${seatTitle}". ${matches.length} connected AI Center models match ` +
      "that request. Select the exact model before continuing; AI-OS will " +
      "not guess or silently substitute a model.",
    );
  }

  return matches[0];
}

function findPreferredModel(
  models: AiCenterModelChoice[],
  constraint: ArenaModelConstraint,
  used: Set<string>,
): AiCenterModelChoice | undefined {
  const unused = models.filter((model) => !used.has(arenaModelKey(model)));
  const pool = unused.length ? unused : models;

  if (constraint.mode === "auto") {
    return pool[0];
  }

  return pool.find((model) => matchesConstraint(model, constraint));
}

export function assignArenaModels(
  seats: ArenaSeatRequirement[],
  models: AiCenterModelChoice[],
): ArenaModelAssignment[] {
  if (!models.length) {
    throw new Error("No AI Center model is connected.");
  }

  const used = new Set<string>();

  return seats.map((seat) => {
    const constraint = seat.modelConstraint;
    const selected = isUnresolvedPinnedConstraint(constraint)
      ? resolveUnresolvedPinnedModel(
          models,
          constraint,
          seat.title,
        )
      : findPreferredModel(models, constraint, used);

    if (!selected) {
      if (constraint.mode === "pinned") {
        throw new Error(
          `Pinned Arena model is unavailable for "${seat.title}". ` +
          "AI-OS will not silently replace an explicitly pinned model.",
        );
      }

      const fallback = models.find(
        (model) => !used.has(arenaModelKey(model)),
      ) ?? models[0];

      used.add(arenaModelKey(fallback));

      return {
        seatId: seat.id,
        choice: fallback,
        policy: constraint.mode,
        fallbackChoices:
          constraint.mode === "prefer"
            ? models.filter(
                (model) =>
                  arenaModelKey(model) !== arenaModelKey(fallback),
              )
            : models.filter(
                (model) =>
                  arenaModelKey(model) !== arenaModelKey(fallback),
              ),
        rationale:
          constraint.mode === "prefer"
            ? "Preferred model was unavailable; an explicit legal fallback was selected."
            : "AI Center automatic Arena assignment.",
      };
    }

    used.add(arenaModelKey(selected));

    const fallbackChoices =
      constraint.mode === "pinned"
        ? []
        : models.filter(
            (model) =>
              arenaModelKey(model) !== arenaModelKey(selected),
          );

    return {
      seatId: seat.id,
      choice: selected,
      policy: constraint.mode,
      fallbackChoices,
      rationale:
        constraint.mode === "pinned"
          ? "User-pinned model. Silent fallback is prohibited."
          : constraint.mode === "prefer"
            ? "User-preferred model selected."
            : "AI Center automatic Arena assignment.",
    };
  });
}
