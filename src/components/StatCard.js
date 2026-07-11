import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
function StatCard({ title, value, icon, accent, cardStyle, }) {
    return (_jsxs("div", { className: "stat-card", style: cardStyle, children: [_jsx("div", { className: "stat-card-glow", style: {
                    background: accent,
                } }), _jsxs("div", { className: "stat-card-header", children: [_jsx("span", { children: title }), _jsx("span", { children: icon })] }), _jsx("div", { className: "stat-card-value", style: { color: accent }, children: value })] }));
}
export default StatCard;
