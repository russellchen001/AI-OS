import { planArena } from "./arenaPlanner";
import {
  createArenaRuntime,
  type ArenaRuntime,
} from "./arenaRuntime";
import type {
  ArenaAssemblyPlan,
  ArenaModelConstraint,
  ArenaRuntimeConfig,
  ArenaRuntimeEvent,
  ArenaRuntimeResult,
  ArenaSessionStatus,
} from "../types/arena";

type ProductSessionDependencies = {
  plan: (
    objective: string,
    constraints: ArenaModelConstraint[],
  ) => Promise<ArenaAssemblyPlan>;
  createRuntime: () => ArenaRuntime;
};

const defaults: ProductSessionDependencies = {
  plan: planArena,
  createRuntime: createArenaRuntime,
};

export class ArenaProductSession {
  private runtime: ArenaRuntime | null = null;

  constructor(
    private readonly dependencies = defaults,
  ) {}

  plan(
    objective: string,
    constraints: ArenaModelConstraint[] = [],
  ): Promise<ArenaAssemblyPlan> {
    return this.dependencies.plan(objective, constraints);
  }

  run(
    plan: ArenaAssemblyPlan,
    config: ArenaRuntimeConfig = {},
    onEvent?: (event: ArenaRuntimeEvent) => void,
  ): Promise<ArenaRuntimeResult> {
    this.runtime = this.dependencies.createRuntime();
    return this.runtime.run(plan, config, onEvent);
  }

  getStatus(): ArenaSessionStatus {
    return this.runtime?.getStatus() ?? "created";
  }

  pause(): void { this.runtime?.pause(); }
  resume(): void { this.runtime?.resume(); }
  async stop(): Promise<void> { await this.runtime?.stop(); }
  async cancel(): Promise<void> { await this.runtime?.cancel(); }
}

export function createArenaProductSession(): ArenaProductSession {
  return new ArenaProductSession();
}
