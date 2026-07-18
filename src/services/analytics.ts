import {
  loadArtifactProjects,
  loadArtifacts,
} from "./artifacts";
import {
  loadPrompts,
} from "./promptLibrary";
import {
  loadCouncilSessions,
} from "./council";
import {
  calculateCost,
} from "./pricing";
import type {
  AnalyticsCount,
  AnalyticsEvent,
  AnalyticsEventType,
  AnalyticsModule,
  AnalyticsSnapshot,
} from "../types/analytics";

const STORAGE_KEY =
  "ai-os.analytics.events.v1";

const MAX_EVENTS = 2500;

type RecordAnalyticsInput = {
  module: AnalyticsModule;
  type: AnalyticsEventType;
  title: string;
  description?: string;

  provider?: string;
  model?: string;

  inputTokens?: number;
  outputTokens?: number;
  estimatedCost?: number;
  latencyMs?: number;

  metadata?: Record<
    string,
    string | number | boolean
  >;
};

function finiteNumber(
  value: unknown,
): number {
  return typeof value === "number" &&
    Number.isFinite(value)
    ? value
    : 0;
}

function normalizeEvent(
  value: Partial<AnalyticsEvent>,
): AnalyticsEvent {
  return {
    id:
      typeof value.id === "string" &&
      value.id
        ? value.id
        : crypto.randomUUID(),

    module:
      value.module ??
      "system",

    type:
      value.type ??
      "completed",

    title:
      typeof value.title === "string" &&
      value.title.trim()
        ? value.title.trim()
        : "Analytics event",

    description:
      typeof value.description ===
      "string"
        ? value.description
        : undefined,

    provider:
      typeof value.provider ===
      "string"
        ? value.provider
        : undefined,

    model:
      typeof value.model === "string"
        ? value.model
        : undefined,

    inputTokens:
      finiteNumber(
        value.inputTokens,
      ),

    outputTokens:
      finiteNumber(
        value.outputTokens,
      ),

    estimatedCost:
      finiteNumber(
        value.estimatedCost,
      ),

    latencyMs:
      finiteNumber(
        value.latencyMs,
      ),

    metadata:
      value.metadata &&
      typeof value.metadata ===
        "object"
        ? value.metadata
        : undefined,

    createdAt:
      typeof value.createdAt ===
        "number"
        ? value.createdAt
        : Date.now(),
  };
}

export function loadAnalyticsEvents():
  AnalyticsEvent[] {
  try {
    const raw =
      localStorage.getItem(
        STORAGE_KEY,
      );

    if (!raw) {
      return [];
    }

    const parsed: unknown =
      JSON.parse(raw);

    if (!Array.isArray(parsed)) {
      return [];
    }

    return parsed
      .map((item) =>
        normalizeEvent(
          item as Partial<AnalyticsEvent>,
        ),
      )
      .sort(
        (left, right) =>
          right.createdAt -
          left.createdAt,
      );
  } catch {
    return [];
  }
}

export function saveAnalyticsEvents(
  events: AnalyticsEvent[],
): void {
  localStorage.setItem(
    STORAGE_KEY,
    JSON.stringify(
      events
        .sort(
          (left, right) =>
            right.createdAt -
            left.createdAt,
        )
        .slice(0, MAX_EVENTS),
    ),
  );
}

export function recordAnalyticsEvent(
  input: RecordAnalyticsInput,
): AnalyticsEvent {
  const pricing =
    calculateCost({
      provider:
        input.provider,
      model:
        input.model,
      inputTokens:
        input.inputTokens,
      outputTokens:
        input.outputTokens,
    });

  const event =
    normalizeEvent({
      ...input,
      estimatedCost:
        input.estimatedCost ??
        pricing.cost,
      metadata: {
        ...input.metadata,
        pricingMatched:
          pricing.matched,
      },
      createdAt: Date.now(),
    });

  saveAnalyticsEvents([
    event,
    ...loadAnalyticsEvents(),
  ]);

  window.dispatchEvent(
    new CustomEvent(
      "ai-os:analytics-updated",
      {
        detail: event,
      },
    ),
  );

  return event;
}

export function clearAnalyticsEvents():
  void {
  localStorage.removeItem(
    STORAGE_KEY,
  );

  window.dispatchEvent(
    new CustomEvent(
      "ai-os:analytics-updated",
    ),
  );
}

function countValues(
  values: Array<
    string | undefined
  >,
): AnalyticsCount[] {
  const counts =
    new Map<string, number>();

  for (const value of values) {
    const normalized =
      value?.trim();

    if (!normalized) {
      continue;
    }

    counts.set(
      normalized,
      (counts.get(
        normalized,
      ) ?? 0) + 1,
    );
  }

  return Array.from(
    counts.entries(),
  )
    .map(
      ([label, value]) => ({
        label,
        value,
      }),
    )
    .sort(
      (left, right) =>
        right.value -
          left.value ||
        left.label.localeCompare(
          right.label,
        ),
    );
}

export function createAnalyticsSnapshot():
  AnalyticsSnapshot {
  const events =
    loadAnalyticsEvents();

  const artifacts =
    loadArtifacts();

  const projects =
    loadArtifactProjects();

  const prompts =
    loadPrompts();

  const councilSessions =
    loadCouncilSessions();

  const latencyEvents =
    events.filter(
      (event) =>
        finiteNumber(
          event.latencyMs,
        ) > 0,
    );

  const totalLatency =
    latencyEvents.reduce(
      (sum, event) =>
        sum +
        finiteNumber(
          event.latencyMs,
        ),
      0,
    );

  return {
    generatedAt:
      Date.now(),

    projects:
      projects.length,

    artifacts:
      artifacts.length,

    prompts:
      prompts.length,

    councilSessions:
      councilSessions.length,

    totalEvents:
      events.length,

    requests:
      events.filter(
        (event) =>
          event.type ===
          "request" ||
          event.type ===
          "started",
      ).length,

    successes:
      events.filter(
        (event) =>
          event.type ===
            "success" ||
          event.type ===
            "completed",
      ).length,

    failures:
      events.filter(
        (event) =>
          event.type ===
          "failure",
      ).length,

    inputTokens:
      events.reduce(
        (sum, event) =>
          sum +
          finiteNumber(
            event.inputTokens,
          ),
        0,
      ),

    outputTokens:
      events.reduce(
        (sum, event) =>
          sum +
          finiteNumber(
            event.outputTokens,
          ),
        0,
      ),

    estimatedCost:
      events.reduce(
        (sum, event) =>
          sum +
          finiteNumber(
            event.estimatedCost,
          ),
        0,
      ),

    averageLatencyMs:
      latencyEvents.length > 0
        ? totalLatency /
          latencyEvents.length
        : 0,

    providers:
      countValues(
        events.map(
          (event) =>
            event.provider,
        ),
      ),

    models:
      countValues(
        events.map(
          (event) =>
            event.model,
        ),
      ),

    modules:
      countValues(
        events.map(
          (event) =>
            event.module,
        ),
      ),

    artifactLanguages:
      countValues(
        artifacts.map(
          (artifact) =>
            artifact.language,
        ),
      ),

    recentEvents:
      events.slice(0, 20),
  };
}
