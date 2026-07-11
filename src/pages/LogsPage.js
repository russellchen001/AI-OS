import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { useEffect, useMemo, useRef, useState, } from "react";
const sourceOptions = [
    "All",
    "AI OS",
    "OpenClaw",
    "Ollama",
    "Docker",
    "Open WebUI",
    "Cherry Studio",
];
const levelOptions = [
    "All",
    "debug",
    "info",
    "warning",
    "error",
];
function formatTimestamp(value) {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) {
        return value;
    }
    return date.toLocaleString([], {
        hour12: false,
    });
}
function levelIcon(level) {
    switch (level) {
        case "error":
            return "⛔";
        case "warning":
            return "⚠️";
        case "debug":
            return "🛠️";
        default:
            return "ℹ️";
    }
}
function LogsPage({ logs, selectedSource, selectedLevel, searchText, isLoading, isAutoRefresh, error, cardStyle, onSourceChange, onLevelChange, onSearchChange, onAutoRefreshChange, onRefresh, onClear, }) {
    const logContainerRef = useRef(null);
    const [autoScroll, setAutoScroll,] = useState(true);
    const [confirmClear, setConfirmClear,] = useState(false);
    useEffect(() => {
        if (!autoScroll ||
            !logContainerRef.current) {
            return;
        }
        logContainerRef.current
            .scrollTo({
            top: logContainerRef
                .current
                .scrollHeight,
            behavior: "smooth",
        });
    }, [
        autoScroll,
        logs,
    ]);
    const summary = useMemo(() => {
        const counts = {
            debug: 0,
            info: 0,
            warning: 0,
            error: 0,
        };
        for (const log of logs) {
            counts[log.level] += 1;
        }
        return counts;
    }, [logs]);
    return (_jsxs("section", { className: "page-section", children: [_jsxs("div", { className: "section-header", children: [_jsxs("div", { children: [_jsx("h2", { children: "System Logs" }), _jsx("p", { children: "Monitor AI OS and local service activity." })] }), _jsxs("div", { className: "logs-header-actions", children: [_jsxs("label", { className: "logs-toggle-option", children: [_jsx("input", { type: "checkbox", checked: isAutoRefresh, onChange: (event) => onAutoRefreshChange(event.target
                                            .checked) }), "Auto refresh"] }), _jsx("button", { type: "button", className: "secondary-button", disabled: isLoading, onClick: onRefresh, children: isLoading
                                    ? "Refreshing..."
                                    : "↻ Refresh" })] })] }), _jsxs("div", { className: "logs-summary-grid", children: [_jsxs("div", { className: "logs-summary-card", style: cardStyle, children: [_jsx("span", { children: "Total" }), _jsx("strong", { children: logs.length })] }), _jsxs("div", { className: "logs-summary-card logs-summary-info", style: cardStyle, children: [_jsx("span", { children: "Info" }), _jsx("strong", { children: summary.info })] }), _jsxs("div", { className: "logs-summary-card logs-summary-warning", style: cardStyle, children: [_jsx("span", { children: "Warnings" }), _jsx("strong", { children: summary.warning })] }), _jsxs("div", { className: "logs-summary-card logs-summary-error", style: cardStyle, children: [_jsx("span", { children: "Errors" }), _jsx("strong", { children: summary.error })] })] }), _jsxs("div", { className: "logs-panel", style: cardStyle, children: [_jsxs("div", { className: "logs-toolbar", children: [_jsxs("label", { className: "logs-filter-field", children: [_jsx("span", { children: "Source" }), _jsx("select", { value: selectedSource, onChange: (event) => onSourceChange(event.target
                                            .value), children: sourceOptions.map((source) => (_jsx("option", { value: source, children: source }, source))) })] }), _jsxs("label", { className: "logs-filter-field", children: [_jsx("span", { children: "Level" }), _jsx("select", { value: selectedLevel, onChange: (event) => onLevelChange(event.target
                                            .value), children: levelOptions.map((level) => (_jsx("option", { value: level, children: level ===
                                                "All"
                                                ? "All levels"
                                                : level }, level))) })] }), _jsxs("label", { className: "logs-search-field", children: [_jsx("span", { children: "Search" }), _jsx("input", { type: "search", value: searchText, placeholder: "Search logs...", onChange: (event) => onSearchChange(event.target
                                            .value) })] }), _jsxs("label", { className: "logs-toggle-option logs-auto-scroll", children: [_jsx("input", { type: "checkbox", checked: autoScroll, onChange: (event) => setAutoScroll(event.target
                                            .checked) }), "Auto scroll"] }), confirmClear ? (_jsxs("div", { className: "logs-clear-confirmation", children: [_jsx("button", { type: "button", className: "danger-button", disabled: isLoading, onClick: () => {
                                            onClear();
                                            setConfirmClear(false);
                                        }, children: "Confirm Clear" }), _jsx("button", { type: "button", className: "secondary-button", onClick: () => setConfirmClear(false), children: "Cancel" })] })) : (_jsx("button", { type: "button", className: "danger-button", disabled: isLoading ||
                                    logs.length === 0, onClick: () => setConfirmClear(true), children: "Clear Logs" }))] }), error && (_jsx("div", { className: "logs-error", role: "alert", children: error })), _jsx("div", { ref: logContainerRef, className: "logs-console", children: isLoading &&
                            logs.length === 0 ? (_jsxs("div", { className: "logs-empty-state", children: [_jsx("span", { children: "\u23F3" }), _jsx("p", { children: "Loading logs..." })] })) : logs.length === 0 ? (_jsxs("div", { className: "logs-empty-state", children: [_jsx("span", { children: "\uD83D\uDCDC" }), _jsx("h3", { children: "No log entries" }), _jsx("p", { children: "Logs will appear when AI OS or a managed service produces activity." })] })) : (logs.map((entry) => (_jsxs("article", { className: [
                                "log-entry",
                                `log-entry-${entry.level}`,
                            ].join(" "), children: [_jsx("span", { className: "log-level-icon", children: levelIcon(entry.level) }), _jsx("time", { className: "log-time", children: formatTimestamp(entry.timestamp) }), _jsx("span", { className: "log-source", children: entry.source }), _jsx("span", { className: "log-level", children: entry.level }), _jsx("pre", { className: "log-message", children: entry.message })] }, entry.id)))) })] })] }));
}
export default LogsPage;
