import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
function StatusBadge({ status, }) {
    return (_jsxs("span", { className: `status-badge status-${status.toLowerCase()}`, children: [_jsx("span", { children: "\u25CF" }), status] }));
}
export default StatusBadge;
