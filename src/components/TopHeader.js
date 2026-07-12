import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
function TopHeader({ isChecking, lastUpdated, settings, onSettingsChange, }) {
    function toggleTheme() {
        onSettingsChange((current) => ({
            ...current,
            theme: current.theme === "dark"
                ? "light"
                : "dark",
        }));
    }
    return (_jsxs("header", { className: "top-header", children: [_jsxs("div", { children: [_jsx("h1", { children: "AI OS" }), _jsx("p", { children: "Your Personal AI Workspace" })] }), _jsxs("div", { className: "header-actions", children: [_jsxs("div", { className: "update-status", children: [_jsx("span", { className: `status-light ${isChecking
                                    ? "checking"
                                    : ""}` }), isChecking
                                ? "Checking services..."
                                : `Updated ${lastUpdated}`] }), _jsx("button", { className: "theme-button", onClick: toggleTheme, title: "Switch theme", children: settings.theme === "dark"
                            ? "☀️"
                            : "🌙" })] })] }));
}
export default TopHeader;
