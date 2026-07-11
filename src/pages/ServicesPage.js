import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import ServiceList from "../components/ServiceList";
import ServiceToggle from "../components/ServiceToggle";
function ServicesPage({ services, cardStyle, allRunning, isBusy, globalAction, serviceAction, openAction, onGlobalToggle, onStartService, onStopService, onOpenService, }) {
    return (_jsxs("section", { className: "page-section", children: [_jsxs("div", { className: "section-header", children: [_jsxs("div", { children: [_jsx("h2", { children: "Services" }), _jsx("p", { children: "Start, stop and open each local service" })] }), _jsx(ServiceToggle, { checked: allRunning, disabled: isBusy, loading: globalAction !== null, large: true, label: globalAction === "start"
                            ? "Starting All..."
                            : globalAction === "stop"
                                ? "Stopping All..."
                                : allRunning
                                    ? "Stop All Services"
                                    : "Start All Services", onChange: onGlobalToggle })] }), _jsx(ServiceList, { services: services, cardStyle: cardStyle, isBusy: isBusy, serviceAction: serviceAction, openAction: openAction, onStart: onStartService, onStop: onStopService, onOpen: onOpenService })] }));
}
export default ServicesPage;
