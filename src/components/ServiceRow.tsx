import type {
  CSSProperties,
} from "react";

import type {
  Service,
} from "../types/index";

import ServiceToggle from "./ServiceToggle";

type ServiceRowProps = {
  service: Service;
  cardStyle: CSSProperties;
  isBusy: boolean;
  serviceAction: string | null;
  openAction: string | null;
  onStart: (
    service: string,
  ) => void;
  onStop: (
    service: string,
  ) => void;
  onOpen: (
    service: string,
  ) => void;
};

function ServiceRow({
  service,
  cardStyle,
  isBusy,
  serviceAction,
  openAction,
  onStart,
  onStop,
  onOpen,
}: ServiceRowProps) {
  const running =
    service.status === "Running";

  const starting =
    serviceAction ===
    `start:${service.name}`;

  const stopping =
    serviceAction ===
    `stop:${service.name}`;

  const loading =
    starting || stopping;

  return (
    <div
      className="service-row"
      style={cardStyle}
    >
      <div className="service-info">
        <span className="service-icon">
          {service.icon}
        </span>

        <div>
          <div className="service-name">
            {service.name}
          </div>

          <div className="service-description">
            {service.description}
          </div>
        </div>
      </div>

      <div className="service-actions">
        <span
          className={[
            "status-badge",
            running
              ? "status-running"
              : service.status ===
                  "Stopped"
                ? "status-stopped"
                : "status-unknown",
          ].join(" ")}
        >
          ● {service.status}
        </span>

        <ServiceToggle
          checked={running}
          disabled={isBusy}
          loading={loading}
          label={
            starting
              ? "Starting..."
              : stopping
                ? "Stopping..."
                : running
                  ? "Running"
                  : "Stopped"
          }
          onChange={() => {
            if (running) {
              onStop(service.name);
            } else {
              onStart(service.name);
            }
          }}
        />

        <button
          type="button"
          className="open-button"
          disabled={
            openAction === service.name
          }
          onClick={() =>
            onOpen(service.name)
          }
        >
          {openAction === service.name
            ? "Opening..."
            : "↗ Open"}
        </button>
      </div>
    </div>
  );
}

export default ServiceRow;