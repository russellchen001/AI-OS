import type {
  ArenaModelConstraint,
  ArenaSeatRole,
} from "../types/arena";

function constraintTargetsRole(
  constraint: ArenaModelConstraint,
  role: ArenaSeatRole,
): boolean {
  if (constraint.mode === "auto") {
    return false;
  }

  return !constraint.targetRole || constraint.targetRole === role;
}

export function resolveUserConstraintsForSeats(
  roles: ArenaSeatRole[],
  constraints: ArenaModelConstraint[],
): ArenaModelConstraint[] {
  const resolved: ArenaModelConstraint[] =
    roles.map(() => ({ mode: "auto" }));

  const remaining = constraints.map((constraint, index) => ({
    constraint,
    index,
    used: false,
  }));

  // Pass 1:
  // Explicit role-bound user requests have highest priority.
  // A model explicitly requested as judge/moderator/evaluator/etc. must
  // bind only to a compatible seat and must never be positionally assigned.
  for (let seatIndex = 0; seatIndex < roles.length; seatIndex += 1) {
    const role = roles[seatIndex];

    const match = remaining.find(
      (entry) =>
        !entry.used &&
        entry.constraint.mode !== "auto" &&
        Boolean(entry.constraint.targetRole) &&
        constraintTargetsRole(entry.constraint, role),
    );

    if (!match) continue;

    resolved[seatIndex] = match.constraint;
    match.used = true;
  }

  // Pass 2:
  // User-selected models without an explicit role may fill ordinary
  // participant seats first. This lets the Chief of Staff decide topology
  // while preserving the user's selected models.
  for (let seatIndex = 0; seatIndex < roles.length; seatIndex += 1) {
    if (resolved[seatIndex].mode !== "auto") continue;
    if (roles[seatIndex] !== "participant") continue;

    const match = remaining.find(
      (entry) =>
        !entry.used &&
        entry.constraint.mode !== "auto" &&
        !entry.constraint.targetRole,
    );

    if (!match) continue;

    resolved[seatIndex] = match.constraint;
    match.used = true;
  }

  // Pass 3:
  // If there are still unbound user-selected models, place them into any
  // remaining compatible non-specialized seats. We deliberately do not
  // force an unscoped model into judge/moderator/evaluator/rules-controller
  // roles merely because of array order.
  for (let seatIndex = 0; seatIndex < roles.length; seatIndex += 1) {
    if (resolved[seatIndex].mode !== "auto") continue;

    const role = roles[seatIndex];

    if (
      role === "judge" ||
      role === "moderator" ||
      role === "evaluator" ||
      role === "rules-controller" ||
      role === "environment"
    ) {
      continue;
    }

    const match = remaining.find(
      (entry) =>
        !entry.used &&
        entry.constraint.mode !== "auto" &&
        !entry.constraint.targetRole,
    );

    if (!match) continue;

    resolved[seatIndex] = match.constraint;
    match.used = true;
  }

  // Hard user constraints must not silently disappear.
  const unresolvedRequired = remaining.filter(
    (entry) =>
      !entry.used &&
      entry.constraint.mode === "pinned",
  );

  if (unresolvedRequired.length > 0) {
    const labels = unresolvedRequired
      .map((entry) =>
        entry.constraint.mode === "pinned"
          ? entry.constraint.label ?? "explicit pinned model"
          : "explicit pinned model",
      )
      .join(", ");

    throw new Error(
      `Arena topology cannot satisfy the user's required model constraint(s): ${labels}. ` +
      "AI-OS will not silently drop, replace, or re-role an explicitly pinned model.",
    );
  }

  return resolved;
}
