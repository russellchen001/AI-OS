import ServiceRow from "../components/ServiceRow";
import type { Service } from "../types";

type ServicesPageProps = {
  services: Service[];
  isBusy: boolean;
  serviceAction: string | null;
  onStartService: (service: string) => void;
  onStopService: (service: string) => void;
  onOpenService: (service: string) => void;
};

function ServicesPage({
  services,
  isBusy,
  serviceAction,
  onStartService,
  onStopService,
  onOpenService,
}: ServicesPageProps) {
  return (
    <section className="page-card">
      <div className="page-title">
        <div>
          <h2>Services</h2>
          <p>
            Use each switch to start or stop an individual service.
          </p>
        </div>
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
  );
}

export default ServicesPage;