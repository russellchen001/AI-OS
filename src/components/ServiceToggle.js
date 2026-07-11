import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
function ServiceToggle({ checked, disabled = false, loading = false, label, large = false, onChange, }) {
    return (_jsxs("button", { type: "button", role: "switch", "aria-checked": checked, "aria-label": label, disabled: disabled, className: [
            "service-toggle",
            checked
                ? "service-toggle-on"
                : "service-toggle-off",
            large
                ? "service-toggle-large"
                : "",
            loading
                ? "service-toggle-loading"
                : "",
        ]
            .filter(Boolean)
            .join(" "), onClick: onChange, children: [_jsx("span", { className: "service-toggle-track", children: _jsx("span", { className: "service-toggle-thumb", children: loading && (_jsx("span", { className: "toggle-spinner" })) }) }), _jsx("span", { className: "service-toggle-label", children: label })] }));
}
export default ServiceToggle;
