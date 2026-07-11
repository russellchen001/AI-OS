import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import ServiceToggle from "./ServiceToggle";
function ServiceRow({ service, cardStyle, isBusy, serviceAction, openAction, onStart, onStop, onOpen, }) {
    const running = service.status === "Running";
    const starting = serviceAction ===
        `start:${service.name}`;
    const stopping = serviceAction ===
        `stop:${service.name}`;
    const loading = starting || stopping;
    return (_jsxs("div", { className: "service-row", style: cardStyle, children: [_jsxs("div", { className: "service-info", children: [_jsx("span", { className: "service-icon", children: service.icon }), _jsxs("div", { children: [_jsx("div", { className: "service-name", children: service.name }), _jsx("div", { className: "service-description", children: service.description })] })] }), _jsxs("div", { className: "service-actions", children: [_jsxs("span", { className: [
                            "status-badge",
                            running
                                ? "status-running"
                                : service.status ===
                                    "Stopped"
                                    ? "status-stopped"
                                    : "status-unknown",
                        ].join(" "), children: ["\u25CF ", service.status] }), _jsx(ServiceToggle, { checked: running, disabled: isBusy, loading: loading, label: starting
                            ? "Starting..."
                            : stopping
                                ? "Stopping..."
                                : running
                                    ? "Running"
                                    : "Stopped", onChange: () => {
                            if (running) {
                                onStop(service.name);
                            }
                            else {
                                onStart(service.name);
                            }
                        } }), _jsx("button", { type: "button", className: "open-button", disabled: openAction === service.name, onClick: () => onOpen(service.name), children: openAction === service.name
                            ? "Opening..."
                            : "↗ Open" })] })] }));
}
export default ServiceRow;
