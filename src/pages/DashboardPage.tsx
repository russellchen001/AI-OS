import MetricCard from "../components/MetricCard";
import ServiceRow from "../components/ServiceRow";
import ServiceToggle from "../components/ServiceToggle";
import StatCard from "../components/StatCard";

import type {
  Service,
  SystemMetrics,
} from "../types";

type DashboardPageProps = {
  services: Service[];
  runningCount: number;
  stoppedCount: number;
  unknownCount: number;
  metrics: SystemMetrics;
  isChecking: boolean;
  isBusy: boolean;
  globalAction: "start" | "stop" | null;
  serviceAction: string | null;
  message: string;
  onStartAll: () => void;
  onStopAll: () => void;
  onHealthCheck: () => void;
  onStartService: (service: string) => void;
  onStopService: (service: string) => void;
  onOpenService: (service: string) => void;
  onBackup: () => void;
};

function DashboardPage({
  services,
  runningCount,
  stoppedCount,
  unknownCount,
  metrics,
  isChecking,
  isBusy,
  globalAction,
  serviceAction,
  message,
  onStartAll,
  onStopAll,
  onHealthCheck,
  onStartService,
  onStopService,
  onOpenService,
  onBackup,
}: DashboardPageProps) {
  const allRunning =
    services.length > 0 &&
    runningCount === services.length;

  const globalLoading =
    globalAction !== null;

  function handleGlobalToggle() {
    if (isBusy) {
      return;
    }

    if (allRunning) {
      onStopAll();
    } else {
      onStartAll();
    }
  }

  function getGlobalLabel(): string {
    if (globalAction === "start") {
      return "Starting All...";
    }

    if (globalAction === "stop") {
      return "Stopping All...";
    }

    return allRunning
      ? "All Services On"
      : "Start All Services";
  }

  return (
    <>
      <section className="stats-grid">
        <StatCard
          title="Total Services"
          value={services.length}
          icon="🧩"
          tone="blue"
        />

        <StatCard
          title="Running"
          value={runningCount}
          icon="✅"
          tone="green"
        />

        <StatCard
          title="Stopped"
          value={stoppedCount}
          icon="⛔"
          tone="red"
        />

        <StatCard
          title="Unknown"
          value={unknownCount}
          icon="⚠️"
          tone="yellow"
        />
      </section>

      <section className="metrics-grid">
        <MetricCard
          label="CPU Usage"
          value={`${metrics.cpuUsage.toFixed(1)}%`}
          percent={metrics.cpuUsage}
          icon="🖥️"
        />

        <MetricCard
          label="Memory"
          value={
            metrics.memoryTotalGb > 0
              ? `${metrics.memoryUsedGb.toFixed(
                  1,
                )} / ${metrics.memoryTotalGb.toFixed(
                  1,
                )} GB`
              : "Waiting for data"
          }
          percent={
            metrics.memoryTotalGb > 0
              ? (metrics.memoryUsedGb /
                  metrics.memoryTotalGb) *
                100
              : 0
          }
          icon="🧠"
        />

        <MetricCard
          label="Disk"
          value={
            metrics.diskTotalGb > 0
              ? `${metrics.diskUsedGb.toFixed(
                  0,
                )} / ${metrics.diskTotalGb.toFixed(
                  0,
                )} GB`
              : "Waiting for data"
          }
          percent={
            metrics.diskTotalGb > 0
              ? (metrics.diskUsedGb /
                  metrics.diskTotalGb) *
                100
              : 0
          }
          icon="💽"
        />
      </section>

      <section className="section-block">
        <div className="section-heading">
          <div>
            <h2>System Status</h2>

            <p>
              Use each switch to start or stop a
              service.
            </p>
          </div>

          <span className="online-count">
            {runningCount}/{services.length} online
          </span>
        </div>

        <div className="service-list">
          {services.map((service) => (
            <ServiceRow
              key={service.name}
              service={service}
              busy={isBusy}
              serviceAction={serviceAction}
              onStart={onStartService}
              onStop={onStopService}
              onOpen={onOpenService}
            />
          ))}
        </div>
      </section>

      <section className="global-actions">
        <ServiceToggle
          checked={allRunning}
          disabled={isBusy}
          loading={globalLoading}
          label={getGlobalLabel()}
          onChange={handleGlobalToggle}
          size="large"
        />

        <button
          className="action-button action-gray"
          disabled={isBusy}
          onClick={onBackup}
        >
          💾 Backup
        </button>

        <button
          className="action-button action-green"
          disabled={isBusy || isChecking}
          onClick={onHealthCheck}
        >
          {isChecking
            ? "⏳ Checking..."
            : "🩺 Health Check"}
        </button>
      </section>

      {message && (
        <section className="terminal-panel">
          {message}
        </section>
      )}
    </>
  );
}

export default DashboardPage;