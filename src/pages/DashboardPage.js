import { jsx as _jsx, jsxs as _jsxs, Fragment as _Fragment } from "react/jsx-runtime";
import MetricCard from "../components/MetricCard";
import ServiceList from "../components/ServiceList";
import ServiceToggle from "../components/ServiceToggle";
import StatCard from "../components/StatCard";
function DashboardPage({ services, metrics, cardStyle, runningCount, stoppedCount, unknownCount, allRunning, isBusy, isChecking, globalAction, serviceAction, openAction, onGlobalToggle, onStartService, onStopService, onOpenService, onRefreshMetrics, onHealthCheck, onBackup, }) {
    return (_jsxs(_Fragment, { children: [_jsxs("section", { className: "stats-grid", children: [_jsx(StatCard, { title: "Total Services", value: services.length, icon: "\uD83E\uDDE9", accent: "#60a5fa", cardStyle: cardStyle }), _jsx(StatCard, { title: "Running", value: runningCount, icon: "\u2705", accent: "#22c55e", cardStyle: cardStyle }), _jsx(StatCard, { title: "Stopped", value: stoppedCount, icon: "\u26D4", accent: "#ef4444", cardStyle: cardStyle }), _jsx(StatCard, { title: "Unknown", value: unknownCount, icon: "\u26A0\uFE0F", accent: "#facc15", cardStyle: cardStyle })] }), _jsxs("section", { className: "section-block", children: [_jsxs("div", { className: "section-header", children: [_jsxs("div", { children: [_jsx("h2", { children: "System Performance" }), _jsx("p", { children: "Live macOS resource usage" })] }), _jsx("button", { type: "button", className: "secondary-button", onClick: onRefreshMetrics, children: "\u21BB Refresh" })] }), _jsxs("div", { className: "metrics-grid", children: [_jsx(MetricCard, { title: "CPU Usage", icon: "\uD83E\uDDE0", value: `${metrics.cpu.toFixed(1)}%`, progress: metrics.cpu, accent: "#3b82f6", cardStyle: cardStyle }), _jsx(MetricCard, { title: "Memory", icon: "\uD83D\uDCBE", value: `${metrics.memoryUsed.toFixed(1)} / ${metrics.memoryTotal.toFixed(1)} GB`, progress: metrics.memoryTotal > 0
                                    ? (metrics.memoryUsed /
                                        metrics.memoryTotal) *
                                        100
                                    : 0, accent: "#8b5cf6", cardStyle: cardStyle }), _jsx(MetricCard, { title: "Disk", icon: "\uD83D\uDDC4\uFE0F", value: `${metrics.diskUsed.toFixed(1)} / ${metrics.diskTotal.toFixed(1)} GB`, progress: metrics.diskTotal > 0
                                    ? (metrics.diskUsed /
                                        metrics.diskTotal) *
                                        100
                                    : 0, accent: "#f59e0b", cardStyle: cardStyle })] })] }), _jsxs("section", { className: "section-block", children: [_jsxs("div", { className: "section-header", children: [_jsxs("div", { children: [_jsx("h2", { children: "System Status" }), _jsx("p", { children: "Control your local AI services" })] }), _jsxs("span", { className: "online-count", children: [runningCount, "/", services.length, " services online"] })] }), _jsx(ServiceList, { services: services, cardStyle: cardStyle, isBusy: isBusy, serviceAction: serviceAction, openAction: openAction, onStart: onStartService, onStop: onStopService, onOpen: onOpenService })] }), _jsxs("section", { className: "bottom-actions", children: [_jsx(ServiceToggle, { checked: allRunning, disabled: isBusy, loading: globalAction !== null, large: true, label: globalAction === "start"
                            ? "Starting All..."
                            : globalAction === "stop"
                                ? "Stopping All..."
                                : allRunning
                                    ? "All Services Running"
                                    : "Start All Services", onChange: onGlobalToggle }), _jsx("button", { type: "button", className: "action-button backup-button", onClick: onBackup, children: "\uD83D\uDCBE Backup" }), _jsx("button", { type: "button", className: "action-button health-button", disabled: isBusy || isChecking, onClick: onHealthCheck, children: isChecking
                            ? "⏳ Checking..."
                            : "🩺 Health Check" })] })] }));
}
export default DashboardPage;
