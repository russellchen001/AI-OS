import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
function MetricCard({ title, icon, value, progress, accent, cardStyle, }) {
    const safeProgress = Math.min(Math.max(progress, 0), 100);
    return (_jsxs("div", { className: "metric-card", style: cardStyle, children: [_jsxs("div", { className: "metric-header", children: [_jsxs("div", { children: [_jsx("span", { className: "metric-title", children: title }), _jsx("div", { className: "metric-value", children: value })] }), _jsx("span", { className: "metric-icon", children: icon })] }), _jsx("div", { className: "metric-track", children: _jsx("div", { className: "metric-progress", style: {
                        width: `${safeProgress}%`,
                        background: accent,
                        boxShadow: `0 0 14px ${accent}55`,
                    } }) }), _jsxs("div", { className: "metric-percent", children: [safeProgress.toFixed(1), "%"] })] }));
}
export default MetricCard;
