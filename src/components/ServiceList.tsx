import type {
  CSSProperties,
} from "react";

import type {
  ServiceStatus,
} from "../types/index";
import type {
  RuntimeOperationSnapshot,
} from "../types/runtime";

import ServiceRow from "./ServiceRow";

export type RuntimeServiceView = {
  runtimeId: string;
  name: string;
  icon: string;
  description: string;
  status: ServiceStatus;
  canStart: boolean;
  canStop: boolean;
  canOpen: boolean;
  listenerReady: boolean;
  lifecyclePending: boolean;
  openPending: boolean;
  lifecycleOperation:
    | RuntimeOperationSnapshot
    | undefined;
  openOperation:
    | RuntimeOperationSnapshot
    | undefined;
};

type ServiceListProps = {
  services: RuntimeServiceView[];
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

function ServiceList({
  services,
  cardStyle,
  onStart,
  onStop,
  onOpen,
}: ServiceListProps) {
  return (
    <div className="service-list">
      {services.map((service) => (
        <ServiceRow
          key={service.runtimeId}
          service={service}
          cardStyle={cardStyle}
          onStart={onStart}
          onStop={onStop}
          onOpen={onOpen}
        />
      ))}
    </div>
  );
}

export default ServiceList;
