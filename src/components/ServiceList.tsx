import type {
  CSSProperties,
} from "react";

import type {
  Service,
} from "../types/index";

import ServiceRow from "./ServiceRow";

type ServiceListProps = {
  services: Service[];
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

function ServiceList({
  services,
  cardStyle,
  isBusy,
  serviceAction,
  openAction,
  onStart,
  onStop,
  onOpen,
}: ServiceListProps) {
  return (
    <div className="service-list">
      {services.map((service) => (
        <ServiceRow
          key={service.name}
          service={service}
          cardStyle={cardStyle}
          isBusy={isBusy}
          serviceAction={
            serviceAction
          }
          openAction={openAction}
          onStart={onStart}
          onStop={onStop}
          onOpen={onOpen}
        />
      ))}
    </div>
  );
}

export default ServiceList;