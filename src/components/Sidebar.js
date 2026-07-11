import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
const navItems = [
    {
        name: "Dashboard",
        icon: "🏠",
        label: "Dashboard",
    },
    {
        name: "Services",
        icon: "🚀",
        label: "Services",
    },
    {
        name: "OpenClaw",
        icon: "🦞",
        label: "OpenClaw",
    },
    {
        name: "Backup",
        icon: "💾",
        label: "Backup",
    },
    {
        name: "Logs",
        icon: "📜",
        label: "Logs",
    },
    {
        name: "Models",
        icon: "🧠",
        label: "Models",
    },
    {
        name: "MCP",
        icon: "🔌",
        label: "MCP",
    },
    {
        name: "Settings",
        icon: "⚙️",
        label: "Settings",
    },
];
function Sidebar({ activePage, settings, onPageChange, }) {
    return (_jsxs("aside", { className: "sidebar", children: [_jsxs("div", { className: "brand", children: [_jsx("div", { className: "brand-icon", children: "\uD83E\uDD16" }), _jsxs("div", { children: [_jsx("div", { className: "brand-title", children: "AI OS" }), _jsx("div", { className: "brand-subtitle", children: "Control Center" })] })] }), _jsx("nav", { className: "nav-list", "aria-label": "Main navigation", children: navItems.map((item) => {
                    const active = activePage ===
                        item.name;
                    return (_jsxs("button", { type: "button", className: [
                            "nav-item",
                            active
                                ? "nav-item-active"
                                : "",
                        ]
                            .filter(Boolean)
                            .join(" "), "aria-current": active
                            ? "page"
                            : undefined, onClick: () => onPageChange(item.name), children: [_jsx("span", { className: "nav-item-icon", "aria-hidden": "true", children: item.icon }), _jsx("span", { children: item.label })] }, item.name));
                }) }), _jsxs("div", { className: "sidebar-footer", children: [_jsxs("div", { className: "refresh-card", children: [_jsx("div", { className: "refresh-label", children: "Auto Refresh" }), _jsxs("div", { className: "refresh-value", children: [_jsx("span", { className: "online-dot", "aria-hidden": "true" }), "Every", " ", settings.refreshInterval, " ", "seconds"] })] }), _jsx("div", { className: "sidebar-version", children: "AI OS v1.2.0" })] })] }));
}
export default Sidebar;
