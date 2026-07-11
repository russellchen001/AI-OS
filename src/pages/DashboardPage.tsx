import type {
  CSSProperties,
} from "react";

import type {
  Metrics,
  Service,
} from "../types/index";

import MetricCard from "../components/MetricCard";
import ServiceList from "../components/ServiceList";
import ServiceToggle from "../components/ServiceToggle";
import StatCard from "../components/StatCard";

type DashboardPageProps = {
  services: Service[];
  metrics: Metrics;
  cardStyle: CSSProperties;
  runningCount: number;
  stoppedCount: number;
  unknownCount: number;
  allRunning: boolean;
  isBusy: boolean;
  isChecking: boolean;
  globalAction:
    | "start"
    | "stop"
    | null;
  serviceAction: string | null;
  openAction: string | null;
  onGlobalToggle: () => void;
  onStartService: (
    service: string,
  ) => void;
  onStopService: (
    service: string,
  ) => void;
  onOpenService: (
    service: string,
  ) => void;
  onRefreshMetrics: () => void;
  onHealthCheck: () => void;
  onBackup: () => void;
};

function DashboardPage({
  services,
  metrics,
  cardStyle,
  runningCount,
  stoppedCount,
  unknownCount,
  allRunning,
  isBusy,
  isChecking,
  globalAction,
  serviceAction,
  openAction,
  onGlobalToggle,
  onStartService,
  onStopService,
  onOpenService,
  onRefreshMetrics,
  onHealthCheck,
  onBackup,
}: DashboardPageProps) {
  return (
    <>
      <section className="stats-grid">
        <StatCard
          title="Total Services"
          value={services.length}
          icon="🧩"
          accent="#60a5fa"
          cardStyle={cardStyle}
        />

        <StatCard
          title="Running"
          value={runningCount}
          icon="✅"
          accent="#22c55e"
          cardStyle={cardStyle}
        />

        <StatCard
          title="Stopped"
          value={stoppedCount}
          icon="⛔"
          accent="#ef4444"
          cardStyle={cardStyle}
        />

        <StatCard
          title="Unknown"
          value={unknownCount}
          icon="⚠️"
          accent="#facc15"
          cardStyle={cardStyle}
        />
      </section>

      <section className="section-block">
        <div className="section-header">
          <div>
            <h2>System Performance</h2>

            <p>
              Live macOS resource usage
            </p>
          </div>

          <button
            type="button"
            className="secondary-button"
            onClick={onRefreshMetrics}
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
            progress={metrics.cpu}
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
            <h2>System Status</h2>

            <p>
              Control your local AI
              services
            </p>
          </div>

          <span className="online-count">
            {runningCount}/
            {services.length} services
            online
          </span>
        </div>

        <ServiceList
          services={services}
          cardStyle={cardStyle}
          isBusy={isBusy}
          serviceAction={serviceAction}
          openAction={openAction}
          onStart={onStartService}
          onStop={onStopService}
          onOpen={onOpenService}
        />
      </section>

      <section className="bottom-actions">
        <ServiceToggle
          checked={allRunning}
          disabled={isBusy}
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
          onChange={onGlobalToggle}
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
            isBusy || isChecking
          }
          onClick={onHealthCheck}
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