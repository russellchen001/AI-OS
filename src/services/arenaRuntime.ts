import {
  streamThroughAiCenter,
  type AiCenterConversationMessage,
  type AiCenterModelChoice,
  type AiCenterStream,
} from "./aiCenter";
import { ArenaContextStore } from "./arenaContextStore";
import { executionSource } from "./aiCenterObservability";
import {
  ARENA_SUPERSEDED_TAG,
  arenaEvaluationOutputViolations,
  collectArenaOutcome,
} from "./arenaOutcome";
import {
  localModelGate,
  type LocalModelGate,
} from "./localModelGate";
import { buildArenaStrategyDriver } from "./arenaStrategyDrivers";
import type {
  ArenaAssemblyPlan,
  ArenaContextEntry,
  ArenaInvocationMetrics,
  ArenaParticipantRuntime,
  ArenaRuntimeConfig,
  ArenaRuntimeEvent,
  ArenaRuntimeResult,
  ArenaSessionStatus,
  ArenaStrategyAction,
  ArenaVisibility,
} from "../types/arena";

type ArenaRuntimeDependencies = {
  stream: typeof streamThroughAiCenter;
  now: () => number;
  uuid: () => string;
  localGate?: LocalModelGate;
};

const defaultDependencies: ArenaRuntimeDependencies = {
  stream: streamThroughAiCenter,
  now: () => Date.now(),
  uuid: () => crypto.randomUUID(),
  localGate: localModelGate,
};

const EVALUATION_MARKER = "AI_OS_ARENA_EVALUATION_JSON";

/** The single re-ask an evaluator gets after breaking the JSON contract. */
function correctionAction(
  action: ArenaStrategyAction,
  violations: string[],
): ArenaStrategyAction {
  return {
    ...action,
    instruction: [
      action.instruction,
      "",
      "Your previous evaluation JSON (visible above) was rejected for these reasons:",
      ...violations.map((violation) => `- ${violation}`),
      "Return the corrected complete JSON object only. Keep your judgement; fix only what was rejected.",
    ].join("\n"),
  };
}

class ArenaCancelledError extends Error {
  constructor() {
    super("Arena execution cancelled.");
    this.name = "ArenaCancelledError";
  }
}

class ArenaStoppedError extends Error {
  constructor() {
    super("Arena execution stopped.");
    this.name = "ArenaStoppedError";
  }
}

function visibilityForAction(
  action: ArenaStrategyAction,
  participant: ArenaParticipantRuntime,
): ArenaVisibility {
  switch (action.outputVisibility) {
    case "participant-private":
      return {
        kind: "participant-private",
        participantId: participant.seatId,
      };

    case "team-private":
      if (!participant.team) {
        return {
          kind: "participant-private",
          participantId: participant.seatId,
        };
      }

      return {
        kind: "team-private",
        teamId: participant.team,
      };

    case "public":
      return { kind: "public" };
  }
}

function assignmentForSeat(
  assembly: ArenaAssemblyPlan,
  seatId: string,
): AiCenterModelChoice {
  const assignment = assembly.modelAssignments.find(
    (candidate) => candidate.seatId === seatId,
  );

  if (!assignment) {
    throw new Error(
      `Arena seat "${seatId}" has no AI Center model assignment.`,
    );
  }

  return assignment.choice;
}

function participantFromSeat(
  assembly: ArenaAssemblyPlan,
  seatId: string,
): ArenaParticipantRuntime {
  const seat = assembly.seats.find(
    (candidate) => candidate.id === seatId,
  );

  if (!seat) {
    throw new Error(`Unknown Arena seat "${seatId}".`);
  }

  return {
    seatId: seat.id,
    title: seat.title,
    role: seat.role,
    team: seat.team,
    assignedModel: assignmentForSeat(assembly, seat.id),
  };
}

export class ArenaRuntime {
  private status: ArenaSessionStatus = "created";
  private cancelled = false;
  private stopped = false;
  private pauseRequested = false;
  private activeStreams = new Map<string, AiCenterStream>();
  private resumeWaiters: Array<() => void> = [];

  constructor(
    private readonly dependencies: ArenaRuntimeDependencies =
      defaultDependencies,
  ) {}

  getStatus(): ArenaSessionStatus {
    return this.status;
  }

  pause(): void {
    if (this.status !== "running") {
      return;
    }

    this.pauseRequested = true;
    this.status = "paused";
  }

  resume(): void {
    if (!this.pauseRequested) {
      return;
    }

    this.pauseRequested = false;
    this.status = "running";

    const waiters = [...this.resumeWaiters];
    this.resumeWaiters = [];

    for (const resolve of waiters) {
      resolve();
    }
  }

  async stop(): Promise<void> {
    this.stopped = true;
    this.status = "stopped";

    const streams = [...this.activeStreams.values()];
    this.activeStreams.clear();

    await Promise.all(
      streams.map((stream) =>
        stream.cancel().catch(() => undefined),
      ),
    );

    this.resume();
  }

  async cancel(): Promise<void> {
    this.cancelled = true;
    this.status = "cancelled";

    const streams = [...this.activeStreams.values()];
    this.activeStreams.clear();

    await Promise.all(
      streams.map((stream) =>
        stream.cancel().catch(() => undefined),
      ),
    );

    this.resume();
  }

  private assertCanContinue(): void {
    if (this.cancelled) {
      throw new ArenaCancelledError();
    }

    if (this.stopped) {
      throw new ArenaStoppedError();
    }
  }

  private async waitIfPaused(): Promise<void> {
    this.assertCanContinue();

    if (!this.pauseRequested) {
      return;
    }

    await new Promise<void>((resolve) => {
      this.resumeWaiters.push(resolve);
    });

    this.assertCanContinue();
  }

  private emit(
    events: ArenaRuntimeEvent[],
    event: ArenaRuntimeEvent,
    callback?: (event: ArenaRuntimeEvent) => void,
  ): void {
    events.push(event);
    callback?.(event);
  }

  private seedContext(
    context: ArenaContextStore,
    assembly: ArenaAssemblyPlan,
    config: ArenaRuntimeConfig,
    events: ArenaRuntimeEvent[],
    sessionId: string,
    callback?: (event: ArenaRuntimeEvent) => void,
  ): void {
    const add = (
      round: number,
      visibility: ArenaVisibility,
      content: string,
      tags?: string[],
    ) => {
      const entry = context.add({
        round,
        visibility,
        content,
        tags,
      });

      this.emit(
        events,
        {
          type: "context-added",
          sessionId,
          round,
          entryId: entry.id,
          visibility: entry.visibility,
          at: this.dependencies.now(),
        },
        callback,
      );
    };

    for (const item of config.initialPublicContext ?? []) {
      add(0, { kind: "public" }, item, ["initial"]);
    }

    for (const [seatId, items] of Object.entries(
      config.participantPrivateContext ?? {},
    )) {
      for (const item of items) {
        add(
          0,
          {
            kind: "participant-private",
            participantId: seatId,
          },
          item,
          ["initial", "private"],
        );
      }
    }

    for (const [teamId, items] of Object.entries(
      config.teamPrivateContext ?? {},
    )) {
      for (const item of items) {
        add(
          0,
          {
            kind: "team-private",
            teamId,
          },
          item,
          ["initial", "team-private"],
        );
      }
    }

    for (const [seatId, hiddenRole] of Object.entries(
      config.hiddenRoles ?? {},
    )) {
      if (!assembly.seats.some((seat) => seat.id === seatId)) {
        throw new Error(
          `Hidden role references unknown Arena seat "${seatId}".`,
        );
      }

      add(
        0,
        {
          kind: "participant-private",
          participantId: seatId,
        },
        `Hidden role: ${hiddenRole}`,
        ["hidden-role"],
      );
    }

    if (config.worldState) {
      add(
        0,
        { kind: "runtime-private" },
        `Initial world state: ${JSON.stringify(config.worldState)}`,
        ["world-state"],
      );
    }
  }

  private buildMessages(
    assembly: ArenaAssemblyPlan,
    participant: ArenaParticipantRuntime,
    action: ArenaStrategyAction,
    context: ArenaContextStore,
    seed: number,
  ): AiCenterConversationMessage[] {
    const projected = context.renderForParticipant(participant);

    return [
      {
        role: "system",
        content: [
          "You are a participant inside AI-OS AI Arena.",
          `Arena strategy: ${action.strategy}`,
          `Seat: ${participant.title}`,
          `Seat role: ${participant.role}`,
          participant.team
            ? `Team: ${participant.team}`
            : "Team: none",
          `Deterministic session seed: ${seed}`,
          "You may use only the context projected below.",
          "Never infer or claim access to hidden participant-private, team-private, or runtime-private information that is not projected to you.",
          "Do not execute tools, Skills, OpenClaw actions, system actions, purchases, messages, files, or external side effects.",
        ].join("\n"),
      },
      {
        role: "user",
        content: [
          `Objective: ${assembly.objective}`,
          "",
          `Round ${action.round} instruction:`,
          action.instruction,
          "",
          "Visible Arena context:",
          projected || "(no prior visible Arena context)",
        ].join("\n"),
      },
    ];
  }

  private async invokeParticipant(
    assembly: ArenaAssemblyPlan,
    participant: ArenaParticipantRuntime,
    action: ArenaStrategyAction,
    context: ArenaContextStore,
    sessionId: string,
    seed: number,
    maxRetries: number,
    metrics: ArenaInvocationMetrics[],
    events: ArenaRuntimeEvent[],
    callback?: (event: ArenaRuntimeEvent) => void,
  ): Promise<ArenaContextEntry | undefined> {
    const messages = this.buildMessages(
      assembly,
      participant,
      action,
      context,
      seed,
    );

    const assignment = assembly.modelAssignments.find(
      (candidate) => candidate.seatId === participant.seatId,
    );

    if (!assignment) {
      throw new Error(
        `Arena seat "${participant.seatId}" has no model assignment.`,
      );
    }

    const candidates = [
      assignment.choice,
      ...(assignment.policy === "pinned"
        ? []
        : assignment.fallbackChoices),
    ].slice(0, Math.max(1, maxRetries + 1));

    let lastError: Error | undefined;

    for (const choice of candidates) {
      this.assertCanContinue();
      await this.waitIfPaused();

      const gate = this.dependencies.localGate;
      const release =
        gate && executionSource(choice) === "local"
          ? await gate.acquire(
              `${choice.providerInstanceId}/${choice.modelId}`,
            )
          : () => undefined;

      const startedAt = this.dependencies.now();

      this.emit(
        events,
        {
          type: "participant-started",
          sessionId,
          round: action.round,
          seatId: participant.seatId,
          at: startedAt,
        },
        callback,
      );

      try {
        this.assertCanContinue();

        const stream = this.dependencies.stream(
          messages,
          choice,
          () => undefined,
        );

        this.activeStreams.set(participant.seatId, stream);

        const result = await stream.result;
        this.activeStreams.delete(participant.seatId);

        if (result.cancelled || this.cancelled) {
          throw new ArenaCancelledError();
        }

        if (this.stopped) {
          throw new ArenaStoppedError();
        }

        const completedAt = this.dependencies.now();

        metrics.push({
          seatId: participant.seatId,
          round: action.round,
          providerId: choice.providerId,
          providerInstanceId: choice.providerInstanceId,
          modelId: choice.modelId,
          startedAt,
          completedAt,
          durationMs: Math.max(0, completedAt - startedAt),
          status: "success",
          metadata: result.response.metadata,
        });

        const entry = context.add({
          round: action.round,
          authorId: participant.seatId,
          visibility: visibilityForAction(
            action,
            participant,
          ),
          content: result.response.text,
          tags: [
            "arena-output",
            action.strategy,
            action.id,
          ],
        });

        this.emit(
          events,
          {
            type: "context-added",
            sessionId,
            round: action.round,
            entryId: entry.id,
            visibility: entry.visibility,
            at: completedAt,
          },
          callback,
        );

        this.emit(
          events,
          {
            type: "participant-completed",
            sessionId,
            round: action.round,
            seatId: participant.seatId,
            at: completedAt,
          },
          callback,
        );

        release();
        return entry;
      } catch (error) {
        release();
        this.activeStreams.delete(participant.seatId);

        if (
          error instanceof ArenaCancelledError ||
          error instanceof ArenaStoppedError
        ) {
          throw error;
        }

        const completedAt = this.dependencies.now();
        const message =
          error instanceof Error
            ? error.message
            : String(error);

        metrics.push({
          seatId: participant.seatId,
          round: action.round,
          providerId: choice.providerId,
          providerInstanceId: choice.providerInstanceId,
          modelId: choice.modelId,
          startedAt,
          completedAt,
          durationMs: Math.max(0, completedAt - startedAt),
          status: "failed",
          error: message,
        });

        lastError =
          error instanceof Error
            ? error
            : new Error(message);
      }
    }

    this.emit(
      events,
      {
        type: "participant-failed",
        sessionId,
        round: action.round,
        seatId: participant.seatId,
        error: lastError?.message ?? "Arena participant invocation failed.",
        at: this.dependencies.now(),
      },
      callback,
    );

    throw lastError ??
      new Error(
        `Arena participant "${participant.seatId}" failed.`,
      );
  }

  async run(
    assembly: ArenaAssemblyPlan,
    config: ArenaRuntimeConfig = {},
    onEvent?: (event: ArenaRuntimeEvent) => void,
  ): Promise<ArenaRuntimeResult> {
    this.cancelled = false;
    this.stopped = false;
    this.pauseRequested = false;
    this.activeStreams.clear();
    this.resumeWaiters = [];

    const strategy =
      assembly.strategyPlan.strategies[0];

    if (!strategy) {
      throw new Error("Arena assembly contains no strategy.");
    }

    const driver = buildArenaStrategyDriver(
      strategy,
      assembly,
    );

    const sessionId = this.dependencies.uuid();
    const startedAt = this.dependencies.now();
    const seed =
      config.seed ??
      Array.from(sessionId).reduce(
        (value, char) =>
          (value * 31 + char.charCodeAt(0)) >>> 0,
        2166136261,
      );

    const context = new ArenaContextStore();
    const events: ArenaRuntimeEvent[] = [];
    const metrics: ArenaInvocationMetrics[] = [];
    const errors: string[] = [];

    this.status = "running";

    this.emit(
      events,
      {
        type: "session-started",
        sessionId,
        at: startedAt,
      },
      onEvent,
    );

    this.emit(
      events,
      {
        type: "session-status",
        sessionId,
        status: "running",
        at: startedAt,
      },
      onEvent,
    );

    this.seedContext(
      context,
      assembly,
      config,
      events,
      sessionId,
      onEvent,
    );

    let roundsCompleted = 0;
    let degraded = false;

    try {
      for (let round = 1; round <= driver.maxRounds; round += 1) {
        this.assertCanContinue();
        await this.waitIfPaused();

        const actions = driver.actions.filter(
          (candidate) => candidate.round === round,
        );

        if (!actions.length) {
          continue;
        }

        this.emit(
          events,
          {
            type: "round-started",
            sessionId,
            round,
            strategy,
            at: this.dependencies.now(),
          },
          onEvent,
        );

        let roundDegraded = false;

        for (const action of actions) {
          this.assertCanContinue();
          await this.waitIfPaused();

          const actorIds = action.actorSeatIds.filter(
            (seatId) =>
              assembly.seats.some(
                (seat) => seat.id === seatId,
              ),
          );

          if (!actorIds.length) {
            if (!action.allowFailure) {
              throw new Error(
                `Arena action "${action.id}" has no compatible actor seat.`,
              );
            }

            roundDegraded = true;
            degraded = true;
            errors.push(
              `Arena action "${action.id}" had no compatible actor seat.`,
            );
            continue;
          }

          const invoke = (
            seatId: string,
            current: ArenaStrategyAction,
          ) =>
            this.invokeParticipant(
              assembly,
              participantFromSeat(assembly, seatId),
              current,
              context,
              sessionId,
              seed,
              Math.max(
                0,
                config.maxRetriesPerInvocation ?? 1,
              ),
              metrics,
              events,
              onEvent,
            );

          const results = await Promise.allSettled(
            actorIds.map(async (seatId) => {
              const entry = await invoke(seatId, action);

              if (
                !entry ||
                !action.instruction.includes(EVALUATION_MARKER)
              ) {
                return entry;
              }

              const violations = arenaEvaluationOutputViolations(
                entry.content,
                { assembly, driver },
              );

              if (!violations.length) return entry;

              // One correction; a second invalid output stays and is
              // rejected by governance.
              let corrected: ArenaContextEntry | undefined;

              try {
                corrected = await invoke(
                  seatId,
                  correctionAction(action, violations),
                );
              } catch (error) {
                if (
                  error instanceof ArenaCancelledError ||
                  error instanceof ArenaStoppedError
                ) {
                  throw error;
                }

                return entry;
              }

              if (corrected) {
                context.tag(entry.id, ARENA_SUPERSEDED_TAG);
              }

              return corrected ?? entry;
            }),
          );

          for (const result of results) {
            if (result.status === "rejected") {
              if (
                result.reason instanceof ArenaCancelledError ||
                result.reason instanceof ArenaStoppedError
              ) {
                throw result.reason;
              }

              roundDegraded = true;
              degraded = true;
              const message =
                result.reason instanceof Error
                  ? result.reason.message
                  : String(result.reason);
              errors.push(message);

              if (!action.allowFailure) {
                throw result.reason;
              }
            }
          }
        }

        roundsCompleted = round;

        this.emit(
          events,
          {
            type: "round-completed",
            sessionId,
            round,
            strategy,
            status: roundDegraded
              ? "degraded"
              : "completed",
            at: this.dependencies.now(),
          },
          onEvent,
        );
      }

      this.status = "completed";
    } catch (error) {
      if (error instanceof ArenaCancelledError) {
        this.status = "cancelled";
      } else if (error instanceof ArenaStoppedError) {
        this.status = "stopped";
      } else {
        this.status = "failed";
        errors.push(
          error instanceof Error
            ? error.message
            : String(error),
        );
      }
    }

    const completedAt = this.dependencies.now();

    this.emit(
      events,
      {
        type: "session-status",
        sessionId,
        status: this.status,
        at: completedAt,
      },
      onEvent,
    );

    const contexts = context.all();

    const governance = collectArenaOutcome({
      assembly,
      driver,
      contexts,
      runtimeStatus: this.status,
      roundsCompleted,
      seed,
      degraded,
    });

    errors.push(...governance.errors);

    const finalDegraded =
      degraded ||
      governance.outcome.status === "degraded";

    return {
      sessionId,
      status: this.status,
      strategy,
      startedAt,
      completedAt,
      roundsCompleted,
      contexts,
      metrics,
      events,
      degraded: finalDegraded,
      errors,
      outcome: governance.outcome,
    };
  }
}

export function createArenaRuntime(): ArenaRuntime {
  return new ArenaRuntime();
}
