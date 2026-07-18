export type AnalyticsModule =
  | "system"
  | "provider"
  | "multillm"
  | "prompt"
  | "council"
  | "artifact"
  | "openclaw"
  | "arena";

export type AnalyticsEventType =
  | "request"
  | "success"
  | "failure"
  | "created"
  | "updated"
  | "deleted"
  | "imported"
  | "exported"
  | "moved"
  | "used"
  | "started"
  | "completed";

export type AnalyticsEvent = {
  id: string;
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

  createdAt: number;
};

export type AnalyticsCount = {
  label: string;
  value: number;
};

export type AnalyticsSnapshot = {
  generatedAt: number;

  projects: number;
  artifacts: number;
  prompts: number;
  councilSessions: number;

  totalEvents: number;
  requests: number;
  successes: number;
  failures: number;

  inputTokens: number;
  outputTokens: number;
  estimatedCost: number;
  averageLatencyMs: number;

  providers: AnalyticsCount[];
  models: AnalyticsCount[];
  modules: AnalyticsCount[];
  artifactLanguages: AnalyticsCount[];

  recentEvents: AnalyticsEvent[];
};
