import type { Service } from "../types";
import ServiceToggle from "./ServiceToggle";

type ServiceRowProps = {
  service: Service;
  busy: boolean;
  serviceAction: string | null;
  onStart: (service: string) => void;
  onStop: (service: string) => void;
  onOpen: (service: string) => void;
};

function ServiceRow({
  service,
  busy,
  serviceAction,
  onStart,
  onStop,
  onOpen,
}: ServiceRowProps) {
  const starting =
    serviceAction === `start:${service.name}`;

  const stopping =
    serviceAction === `stop:${service.name}`;

  const opening =
    serviceAction === `open:${service.name}`;

  const isRunning =
    service.status === "Running";

  const isLoading = starting || stopping;

  function handleToggle() {
    if (busy) {
      return;
    }

    if (isRunning) {
      onStop(service.name);
    } else {
      onStart(service.name);
    }
  }

  function getToggleLabel(): string {
    if (starting) {
      return "Starting...";
    }

    if (stopping) {
      return "Stopping...";
    }

    if (service.status === "Unknown") {
      return "Unknown";
    }

    return isRunning
      ? "Running"
      : "Stopped";
  }

  return (
    <div className="service-row">
      <div className="service-identity">
        <span className="service-icon">
          {service.icon}
        </span>

        <div>
          <div className="service-name">
            {service.name}
          </div>

          <div className="service-description">
            Local workspace service
          </div>
        </div>
      </div>

      <ServiceToggle
        checked={isRunning}
        disabled={busy}
        loading={isLoading}
        label={getToggleLabel()}
        onChange={handleToggle}
      />

      <div className="service-buttons">
        {service.canOpen && (
          <button
            className="mini-button mini-purple"
            disabled={busy}
            onClick={() =>
              onOpen(service.name)
            }
          >
            {opening
              ? "⏳ Opening"
              : "↗ Open"}
          </button>
        )}
      </div>
    </div>
  );
}

export default ServiceRow;