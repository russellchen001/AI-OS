import type {
  CSSProperties,
} from "react";

import type {
  Service,
} from "../types/index";

import ServiceList from "../components/ServiceList";
import ServiceToggle from "../components/ServiceToggle";

type ServicesPageProps = {
  services: Service[];
  cardStyle: CSSProperties;
  allRunning: boolean;
  isBusy: boolean;
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
};

function ServicesPage({
  services,
  cardStyle,
  allRunning,
  isBusy,
  globalAction,
  serviceAction,
  openAction,
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
                  ? "Stop All Services"
                  : "Start All Services"
          }
          onChange={onGlobalToggle}
        />
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
  );
}

export default ServicesPage;