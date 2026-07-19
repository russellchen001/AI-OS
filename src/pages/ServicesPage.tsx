import type {
  CSSProperties,
} from "react";

import ServiceList from "../components/ServiceList";
import type {
  RuntimeServiceView,
} from "../components/ServiceList";
import ServiceToggle from "../components/ServiceToggle";

type ServicesPageProps = {
  services: RuntimeServiceView[];
  cardStyle: CSSProperties;
  allRunning: boolean;
  hasCanonicalActivity: boolean;
  globalAction:
    | "start"
    | "stop"
    | null;
  bulkIsolationActive: boolean;
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
};

function ServicesPage({
  services,
  cardStyle,
  allRunning,
  hasCanonicalActivity,
  globalAction,
  bulkIsolationActive,
  onGlobalToggle,
  onStartService,
  onStopService,
  onOpenService,
}: ServicesPageProps) {
  return (
    <section className="page-section">
      <div className="section-header">
        <div>
          <h2>Services</h2>

          <p>
            Start, stop and open each
            local service
          </p>
        </div>

        <ServiceToggle
          checked={allRunning}
          disabled={
            bulkIsolationActive ||
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
                  ? "Stop All Services"
                  : "Start All Services"
          }
          onChange={onGlobalToggle}
        />
      </div>

      <ServiceList
        services={services}
        cardStyle={cardStyle}
        bulkActive={
          bulkIsolationActive
        }
        onStart={onStartService}
        onStop={onStopService}
        onOpen={onOpenService}
      />
    </section>
  );
}

export default ServicesPage;
