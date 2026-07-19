import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type CSSProperties,
} from "react";
import type {
  Metrics,
} from "../types/index";
import type {
  RuntimeServiceView,
} from "../components/ServiceList";
import type {
  AnalyticsEvent,
  AnalyticsSnapshot,
} from "../types/analytics";
import {
  createAnalyticsSnapshot,
} from "../services/analytics";
import MetricCard from "../components/MetricCard";
import ServiceList from "../components/ServiceList";
import ServiceToggle from "../components/ServiceToggle";
import StatCard from "../components/StatCard";

type DashboardPageProps = {
  services: RuntimeServiceView[];
  metrics: Metrics;
  cardStyle: CSSProperties;
  runningCount: number;
  stoppedCount: number;
  unknownCount: number;
  allRunning: boolean;
  hasCanonicalActivity: boolean;
  isChecking: boolean;
  globalAction:
    | "start"
    | "stop"
    | null;
  onGlobalToggle: () => void;
  onStartService: (
    runtimeId: string,
  ) => void;
  onStopService: (
    runtimeId: string,
  ) => void;
  onOpenService: (
    runtimeId: string,
  ) => void;
  onRefreshMetrics: () => void;
  onHealthCheck: () => void;
  onBackup: () => void;
};

function formatNumber(
  value: number,
): string {
  return new Intl.NumberFormat(
    undefined,
    {
      notation:
        value >= 10000
          ? "compact"
          : "standard",
      maximumFractionDigits: 1,
    },
  ).format(value);
}

function formatCost(
  value: number,
): string {
  if (
    !Number.isFinite(value) ||
    value <= 0
  ) {
    return "US$0.00";
  }

  return new Intl.NumberFormat(
    undefined,
    {
      style:
        "currency",
      currency:
        "USD",
      minimumFractionDigits:
        value < 0.01
          ? 4
          : 2,
      maximumFractionDigits:
        value < 0.01
          ? 6
          : 2,
    },
  ).format(value);
}

function formatLatency(
  value: number,
): string {
  if (value <= 0) {
    return "—";
  }

  if (value < 1000) {
    return `${Math.round(
      value,
    )} ms`;
  }

  return `${(
    value / 1000
  ).toFixed(1)} s`;
}

function eventIcon(
  event: AnalyticsEvent,
): string {
  switch (event.module) {
    case "artifact":
      return "🧱";
    case "prompt":
      return "📚";
    case "council":
      return "🏛";
    case "provider":
      return "🔌";
    case "multillm":
      return "🧩";
    case "openclaw":
      return "🦞";
    case "arena":
      return "🎭";
    default:
      return "●";
  }
}

function DashboardPage({
  services,
  metrics,
  cardStyle,
  runningCount,
  stoppedCount,
  unknownCount,
  allRunning,
  hasCanonicalActivity,
  isChecking,
  globalAction,
  onGlobalToggle,
  onStartService,
  onStopService,
  onOpenService,
  onRefreshMetrics,
  onHealthCheck,
  onBackup,
}: DashboardPageProps) {
  const [
    analytics,
    setAnalytics,
  ] = useState<
    AnalyticsSnapshot
  >(createAnalyticsSnapshot);

  const refreshAnalytics =
    useCallback(() => {
      setAnalytics(
        createAnalyticsSnapshot(),
      );
    }, []);

  useEffect(() => {
    window.addEventListener(
      "ai-os:analytics-updated",
      refreshAnalytics,
    );

    window.addEventListener(
      "ai-os:artifact-created",
      refreshAnalytics,
    );

    return () => {
      window.removeEventListener(
        "ai-os:analytics-updated",
        refreshAnalytics,
      );

      window.removeEventListener(
        "ai-os:artifact-created",
        refreshAnalytics,
      );
    };
  }, [
    refreshAnalytics,
  ]);

  const maxLanguageCount =
    useMemo(
      () =>
        Math.max(
          ...analytics
            .artifactLanguages
            .map(
              (item) =>
                item.value,
            ),
          1,
        ),
      [
        analytics
          .artifactLanguages,
      ],
    );

  return (
    <>
      <section className="dashboard-analytics-heading">
        <div>
          <span className="dashboard-eyebrow">
            AI OS Analytics
          </span>

          <h1>
            Dashboard Overview
          </h1>

          <p>
            System health, AI activity and Workspace usage in one place.
          </p>
        </div>

        <button
          type="button"
          className="secondary-button"
          onClick={() => {
            onRefreshMetrics();
            refreshAnalytics();
          }}
        >
          ↻ Refresh Dashboard
        </button>
      </section>

      <section className="stats-grid dashboard-analytics-stats">
        <StatCard
          title="AI Requests"
          value={
            analytics.requests
          }
          icon="⚡"
          accent="#60a5fa"
          cardStyle={cardStyle}
        />

        <StatCard
          title="Artifacts"
          value={
            analytics.artifacts
          }
          icon="🧱"
          accent="#8b5cf6"
          cardStyle={cardStyle}
        />

        <StatCard
          title="Prompts"
          value={
            analytics.prompts
          }
          icon="📚"
          accent="#22c55e"
          cardStyle={cardStyle}
        />

        <StatCard
          title="Council Sessions"
          value={
            analytics
              .councilSessions
          }
          icon="🏛"
          accent="#f59e0b"
          cardStyle={cardStyle}
        />
      </section>

      <section className="dashboard-ai-metrics-grid">
        <article
          className="settings-card dashboard-ai-metric"
          style={cardStyle}
        >
          <span>
            Total Tokens
          </span>
          <strong>
            {formatNumber(
              analytics.inputTokens +
                analytics.outputTokens,
            )}
          </strong>
          <small>
            {formatNumber(
              analytics.inputTokens,
            )}{" "}
            input ·{" "}
            {formatNumber(
              analytics.outputTokens,
            )}{" "}
            output
          </small>
        </article>

        <article
          className="settings-card dashboard-ai-metric"
          style={cardStyle}
        >
          <span>
            Estimated Cost
          </span>
          <strong>
            {formatCost(
              analytics.estimatedCost,
            )}
          </strong>
          <small>
            Accumulated tracked usage
          </small>
        </article>

        <article
          className="settings-card dashboard-ai-metric"
          style={cardStyle}
        >
          <span>
            Average Latency
          </span>
          <strong>
            {formatLatency(
              analytics.averageLatencyMs,
            )}
          </strong>
          <small>
            Across tracked AI requests
          </small>
        </article>

        <article
          className="settings-card dashboard-ai-metric"
          style={cardStyle}
        >
          <span>
            Success / Failure
          </span>
          <strong>
            {analytics.successes}
            {" / "}
            {analytics.failures}
          </strong>
          <small>
            Completed tracked operations
          </small>
        </article>
      </section>

      <section className="dashboard-analytics-layout">
        <article
          className="settings-card dashboard-analytics-panel"
          style={cardStyle}
        >
          <div className="section-header">
            <div>
              <h2>
                Artifact Languages
              </h2>
              <p>
                Current Workspace distribution
              </p>
            </div>

            <span className="dashboard-panel-count">
              {analytics.artifacts}
            </span>
          </div>

          <div className="dashboard-bar-list">
            {analytics
              .artifactLanguages
              .length === 0 ? (
              <p className="dashboard-analytics-empty">
                Save an Artifact to begin tracking Workspace usage.
              </p>
            ) : (
              analytics
                .artifactLanguages
                .slice(0, 8)
                .map((item) => (
                  <div
                    key={
                      item.label
                    }
                    className="dashboard-bar-row"
                  >
                    <div>
                      <span>
                        {
                          item.label
                        }
                      </span>
                      <strong>
                        {
                          item.value
                        }
                      </strong>
                    </div>

                    <div className="dashboard-bar-track">
                      <span
                        style={{
                          width:
                            `${
                              (
                                item.value /
                                maxLanguageCount
                              ) * 100
                            }%`,
                        }}
                      />
                    </div>
                  </div>
                ))
            )}
          </div>
        </article>

        <article
          className="settings-card dashboard-analytics-panel dashboard-timeline-panel"
          style={cardStyle}
        >
          <div className="section-header">
            <div>
              <h2>
                Recent Activity
              </h2>
              <p>
                Unified AI OS timeline
              </p>
            </div>

            <span className="dashboard-panel-count">
              {
                analytics.totalEvents
              }
            </span>
          </div>

          <div className="dashboard-timeline">
            {analytics
              .recentEvents
              .length === 0 ? (
              <p className="dashboard-analytics-empty">
                New AI and Workspace actions will appear here.
              </p>
            ) : (
              analytics
                .recentEvents
                .slice(0, 10)
                .map((event) => (
                  <div
                    key={event.id}
                    className="dashboard-timeline-item"
                  >
                    <span className="dashboard-timeline-icon">
                      {
                        eventIcon(
                          event,
                        )
                      }
                    </span>

                    <div>
                      <strong>
                        {
                          event.title
                        }
                      </strong>

                      <small>
                        {event.description ??
                          event.module}
                      </small>
                    </div>

                    <time>
                      {new Date(
                        event.createdAt,
                      ).toLocaleTimeString(
                        [],
                        {
                          hour:
                            "2-digit",
                          minute:
                            "2-digit",
                        },
                      )}
                    </time>
                  </div>
                ))
            )}
          </div>
        </article>
      </section>

      <section className="dashboard-ranking-grid">
        {[
          {
            title:
              "Top Providers",
            icon:
              "🔌",
            items:
              analytics.providers,
          },
          {
            title:
              "Top Models",
            icon:
              "🧠",
            items:
              analytics.models,
          },
          {
            title:
              "Activity by Module",
            icon:
              "📊",
            items:
              analytics.modules,
          },
        ].map((group) => (
          <article
            key={
              group.title
            }
            className="settings-card dashboard-ranking-panel"
            style={cardStyle}
          >
            <div className="section-header">
              <div>
                <h2>
                  {group.icon}{" "}
                  {group.title}
                </h2>
              </div>
            </div>

            <div className="dashboard-ranking-list">
              {group.items.length ===
              0 ? (
                <p className="dashboard-ranking-empty">
                  No tracked data yet.
                </p>
              ) : (
                group.items
                  .slice(0, 6)
                  .map(
                    (
                      item,
                      index,
                    ) => (
                      <div
                        key={
                          item.label
                        }
                        className="dashboard-ranking-item"
                      >
                        <span>
                          {index +
                            1}
                        </span>

                        <strong>
                          {
                            item.label
                          }
                        </strong>

                        <small>
                          {
                            item.value
                          }
                        </small>
                      </div>
                    ),
                  )
              )}
            </div>
          </article>
        ))}
      </section>

      <section className="section-block">
        <div className="section-header">
          <div>
            <h2>
              System Performance
            </h2>
            <p>
              Live macOS resource usage
            </p>
          </div>

          <button
            type="button"
            className="secondary-button"
            onClick={
              onRefreshMetrics
            }
          >
            ↻ Refresh
          </button>
        </div>

        <div className="metrics-grid">
          <MetricCard
            title="CPU Usage"
            icon="🧠"
            value={`${metrics.cpu.toFixed(
              1,
            )}%`}
            progress={
              metrics.cpu
            }
            accent="#3b82f6"
            cardStyle={cardStyle}
          />

          <MetricCard
            title="Memory"
            icon="💾"
            value={`${metrics.memoryUsed.toFixed(
              1,
            )} / ${metrics.memoryTotal.toFixed(
              1,
            )} GB`}
            progress={
              metrics.memoryTotal > 0
                ? (metrics.memoryUsed /
                    metrics.memoryTotal) *
                  100
                : 0
            }
            accent="#8b5cf6"
            cardStyle={cardStyle}
          />

          <MetricCard
            title="Disk"
            icon="🗄️"
            value={`${metrics.diskUsed.toFixed(
              1,
            )} / ${metrics.diskTotal.toFixed(
              1,
            )} GB`}
            progress={
              metrics.diskTotal > 0
                ? (metrics.diskUsed /
                    metrics.diskTotal) *
                  100
                : 0
            }
            accent="#f59e0b"
            cardStyle={cardStyle}
          />
        </div>
      </section>

      <section className="section-block">
        <div className="section-header">
          <div>
            <h2>
              System Status
            </h2>
            <p>
              Control your local AI services
            </p>
          </div>

          <span className="online-count">
            {runningCount}/
            {services.length} services online
          </span>
        </div>

        <ServiceList
          services={services}
          cardStyle={cardStyle}
          bulkActive={
            globalAction !== null
          }
          onStart={
            onStartService
          }
          onStop={
            onStopService
          }
          onOpen={
            onOpenService
          }
        />
      </section>

      <section className="bottom-actions">
        <ServiceToggle
          checked={allRunning}
          disabled={
            globalAction !== null ||
            hasCanonicalActivity
          }
          loading={
            globalAction !== null
          }
          large
          label={
            globalAction === "start"
              ? "Starting All..."
              : globalAction === "stop"
                ? "Stopping All..."
                : allRunning
                  ? "All Services Running"
                  : "Start All Services"
          }
          onChange={
            onGlobalToggle
          }
        />

        <button
          type="button"
          className="action-button backup-button"
          onClick={onBackup}
        >
          💾 Backup
        </button>

        <button
          type="button"
          className="action-button health-button"
          disabled={
            isChecking
          }
          onClick={
            onHealthCheck
          }
        >
          {isChecking
            ? "⏳ Checking..."
            : "🩺 Health Check"}
        </button>
      </section>
    </>
  );
}

export default DashboardPage;
