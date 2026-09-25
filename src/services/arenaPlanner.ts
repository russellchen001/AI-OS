import { listAiCenterModels } from "./aiCenter";
import { createArenaChiefOfStaff } from "./arenaChiefOfStaff";
import { extractUserModelConstraints } from "./arenaStrategyRouter";
import type {
  ArenaAssemblyPlan,
  ArenaModelConstraint,
} from "../types/arena";

export async function planArena(
  objective: string,
  explicitConstraints: ArenaModelConstraint[] = [],
): Promise<ArenaAssemblyPlan> {
  const availableModels = listAiCenterModels();

  const detectedConstraints =
    extractUserModelConstraints(
      objective,
      availableModels,
    );

  return createArenaChiefOfStaff().assemble({
    objective,
    availableModels,
    userModelConstraints: [
      ...explicitConstraints,
      ...detectedConstraints,
    ].slice(0, 12),
  });
}
