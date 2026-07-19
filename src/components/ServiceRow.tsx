import type {
  CSSProperties,
} from "react";

import ServiceToggle from "./ServiceToggle";
import type {
  RuntimeServiceView,
} from "./ServiceList";

type ServiceRowProps = {
  service: RuntimeServiceView;
  cardStyle: CSSProperties;
  onStart: (
    runtimeId: string,
  ) => void;
  onStop: (
    runtimeId: string,
  ) => void;
  onOpen: (
    runtimeId: string,
  ) => void;
};

function lifecycleLabel(
  service: RuntimeServiceView,
): string {
  const operation =
    service.lifecycleOperation;
  if (service.lifecyclePending) {
    return "Working…";
  }
  if (operation === undefined) {
    return service.status === "Running"
      ? "Running"
      : service.status === "Stopped"
        ? "Stopped"
        : "Unknown";
  }
  if (operation.state === "queued") {
    switch (operation.action) {
      case "start":
        return "Queued to start";
      case "stop":
        return "Queued to stop";
      case "restart":
        return "Queued to restart";
      case "open":
        return "Working…";
    }
  }
  if (operation.state === "running") {
    const knownPhases: Record<
      string,
      string
    > = {
      validating: "Working…",
      "starting-application": "Starting…",
      "stopping-service": "Stopping…",
      "waiting-for-readiness": "Working…",
      "checking-dependency": "Working…",
      "starting-container": "Starting…",
      "stopping-container": "Stopping…",
      "restarting-container": "Restarting…",
      opening: "Opening…",
      verifying: "Verifying…",
      complete: "Working…",
    };
    return operation.progress === null
      ? operation.action === "start"
        ? "Starting…"
        : operation.action === "stop"
          ? "Stopping…"
          : "Restarting…"
      : knownPhases[
          operation.progress.phase
        ] ?? "Working…";
  }
  return "Working…";
}

function ServiceRow({
  service,
  cardStyle,
  onStart,
  onStop,
  onOpen,
}: ServiceRowProps) {
  const running =
    service.status === "Running";

  const lifecycleBusy =
    service.lifecyclePending ||
    service.lifecycleOperation !==
      undefined;
  const openBusy =
    service.openPending ||
    service.openOperation !== undefined;
  const nextActionSupported = running
    ? service.canStop
    : service.canStart;

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
          disabled={
            !service.listenerReady ||
            lifecycleBusy ||
            !nextActionSupported
          }
          loading={lifecycleBusy}
          label={lifecycleLabel(service)}
          onChange={() => {
            if (running) {
              onStop(service.runtimeId);
            } else {
              onStart(service.runtimeId);
            }
          }}
        />

        <button
          type="button"
          className="open-button"
          disabled={
            !service.listenerReady ||
            openBusy ||
            !service.canOpen
          }
          onClick={() =>
            onOpen(service.runtimeId)
          }
        >
          {openBusy
            ? "Opening…"
            : "↗ Open"}
        </button>
      </div>
    </div>
  );
}

export default ServiceRow;
