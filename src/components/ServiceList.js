import { jsx as _jsx } from "react/jsx-runtime";
import ServiceRow from "./ServiceRow";
function ServiceList({ services, cardStyle, isBusy, serviceAction, openAction, onStart, onStop, onOpen, }) {
    return (_jsx("div", { className: "service-list", children: services.map((service) => (_jsx(ServiceRow, { service: service, cardStyle: cardStyle, isBusy: isBusy, serviceAction: serviceAction, openAction: openAction, onStart: onStart, onStop: onStop, onOpen: onOpen }, service.name))) }));
}
export default ServiceList;
