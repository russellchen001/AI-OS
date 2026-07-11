import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
function Header({ isChecking, lastUpdated, }) {
    return (_jsxs("header", { className: "top-header", children: [_jsxs("div", { children: [_jsx("h1", { children: "Russell AI OS" }), _jsx("p", { children: "Your Personal AI Workspace" })] }), _jsxs("div", { className: "updated-badge", children: [_jsx("span", { className: [
                            "updated-dot",
                            isChecking
                                ? "updated-dot-checking"
                                : "",
                        ]
                            .filter(Boolean)
                            .join(" ") }), isChecking
                        ? "Checking services..."
                        : `Updated ${lastUpdated}`] })] }));
}
export default Header;
