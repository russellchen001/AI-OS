import { jsx as _jsx, jsxs as _jsxs, Fragment as _Fragment } from "react/jsx-runtime";
import { useMemo, useState, } from "react";
function formatBytes(bytes) {
    if (!Number.isFinite(bytes) ||
        bytes <= 0) {
        return "0 B";
    }
    const units = [
        "B",
        "KB",
        "MB",
        "GB",
    ];
    const index = Math.min(Math.floor(Math.log(bytes) /
        Math.log(1024)), units.length - 1);
    const value = bytes /
        1024 ** index;
    return `${value.toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}
function formatDate(value) {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) {
        return value;
    }
    return date.toLocaleString();
}
function BackupPage({ settings, backups, status, selectedBackup, error, cardStyle, onUpdateSetting, onCreateBackup, onRestoreBackup, onRefresh, onReveal, onDelete, }) {
    const [restoreOpenClaw, setRestoreOpenClaw,] = useState(true);
    const [restoreSettings, setRestoreSettings,] = useState(true);
    const [confirmingDelete, setConfirmingDelete,] = useState(null);
    const isCreating = status === "creating";
    const isRestoring = status === "restoring";
    const totalSize = useMemo(() => backups.reduce((total, backup) => total +
        backup.sizeBytes, 0), [backups]);
    return (_jsxs("section", { className: "page-section", children: [_jsxs("div", { className: "section-header", children: [_jsxs("div", { children: [_jsx("h2", { children: "Backup & Restore" }), _jsx("p", { children: "Protect your OpenClaw configuration and AI OS settings." })] }), _jsx("button", { type: "button", className: "secondary-button", disabled: isCreating ||
                            isRestoring, onClick: onRefresh, children: "\u21BB Refresh" })] }), _jsxs("div", { className: "backup-layout", children: [_jsxs("div", { className: "settings-card backup-create-card", style: cardStyle, children: [_jsxs("div", { className: "backup-card-heading", children: [_jsxs("div", { children: [_jsx("h3", { children: "Create Backup" }), _jsx("p", { children: "Backups are saved as timestamped archives. Ollama model files are excluded." })] }), _jsx("span", { className: "backup-card-icon", children: "\uD83D\uDCBE" })] }), _jsxs("label", { className: "setting-field", children: [_jsx("span", { children: "Destination Directory" }), _jsx("small", { children: "Enter an absolute folder path, for example /Users/your-name/Backups." }), _jsx("input", { type: "text", value: settings
                                            .backupDirectory, placeholder: "/Users/your-name/Backups", disabled: isCreating ||
                                            isRestoring, onChange: (event) => onUpdateSetting("backupDirectory", event.target.value) })] }), _jsxs("div", { className: "backup-options", children: [_jsxs("label", { className: "backup-checkbox-row", children: [_jsx("input", { type: "checkbox", checked: settings
                                                    .includeOpenClawConfig, disabled: isCreating ||
                                                    isRestoring, onChange: (event) => onUpdateSetting("includeOpenClawConfig", event.target
                                                    .checked) }), _jsxs("span", { children: [_jsx("strong", { children: "OpenClaw configuration" }), _jsx("small", { children: "Includes the local OpenClaw config folder when it exists." })] })] }), _jsxs("label", { className: "backup-checkbox-row", children: [_jsx("input", { type: "checkbox", checked: settings
                                                    .includeAiOsSettings, disabled: isCreating ||
                                                    isRestoring, onChange: (event) => onUpdateSetting("includeAiOsSettings", event.target
                                                    .checked) }), _jsxs("span", { children: [_jsx("strong", { children: "AI OS settings" }), _jsx("small", { children: "Includes refresh, URLs, theme and backup preferences." })] })] })] }), _jsx("button", { type: "button", className: "action-button backup-button backup-primary-button", disabled: isCreating ||
                                    isRestoring, onClick: onCreateBackup, children: isCreating
                                    ? "⏳ Creating Backup..."
                                    : "💾 Create Backup" }), error && (_jsx("div", { className: "backup-error", role: "alert", children: error }))] }), _jsxs("div", { className: "settings-card backup-summary-card", style: cardStyle, children: [_jsxs("div", { className: "backup-card-heading", children: [_jsxs("div", { children: [_jsx("h3", { children: "Backup Summary" }), _jsx("p", { children: "Local backup archive overview." })] }), _jsx("span", { className: "backup-card-icon", children: "\uD83D\uDCE6" })] }), _jsxs("div", { className: "backup-summary-grid", children: [_jsxs("div", { className: "backup-summary-item", children: [_jsx("span", { children: "Archives" }), _jsx("strong", { children: backups.length })] }), _jsxs("div", { className: "backup-summary-item", children: [_jsx("span", { children: "Total Size" }), _jsx("strong", { children: formatBytes(totalSize) })] }), _jsxs("div", { className: "backup-summary-item", children: [_jsx("span", { children: "OpenClaw" }), _jsx("strong", { children: settings
                                                    .includeOpenClawConfig
                                                    ? "Included"
                                                    : "Excluded" })] }), _jsxs("div", { className: "backup-summary-item", children: [_jsx("span", { children: "AI OS" }), _jsx("strong", { children: settings
                                                    .includeAiOsSettings
                                                    ? "Included"
                                                    : "Excluded" })] })] }), _jsxs("div", { className: "backup-note", children: [_jsx("strong", { children: "Restore options" }), _jsxs("label", { className: "backup-inline-option", children: [_jsx("input", { type: "checkbox", checked: restoreOpenClaw, onChange: (event) => setRestoreOpenClaw(event.target
                                                    .checked) }), "Restore OpenClaw configuration"] }), _jsxs("label", { className: "backup-inline-option", children: [_jsx("input", { type: "checkbox", checked: restoreSettings, onChange: (event) => setRestoreSettings(event.target
                                                    .checked) }), "Restore AI OS settings"] })] })] })] }), _jsx("div", { className: "section-header backup-history-header", children: _jsxs("div", { children: [_jsx("h2", { children: "Backup History" }), _jsx("p", { children: "Restore, reveal or delete existing archives." })] }) }), backups.length === 0 ? (_jsxs("div", { className: "backup-empty-state", style: cardStyle, children: [_jsx("span", { children: "\uD83D\uDDC2\uFE0F" }), _jsx("h3", { children: "No backups found" }), _jsx("p", { children: "Enter a destination directory and create your first backup." })] })) : (_jsx("div", { className: "backup-list", children: backups.map((backup) => {
                    const busy = selectedBackup ===
                        backup.path;
                    const deleting = confirmingDelete ===
                        backup.path;
                    return (_jsxs("article", { className: "backup-record", style: cardStyle, children: [_jsxs("div", { className: "backup-record-main", children: [_jsx("span", { className: "backup-record-icon", children: "\uD83D\uDCE6" }), _jsxs("div", { children: [_jsx("h3", { children: backup.fileName }), _jsx("p", { children: formatDate(backup.createdAt) }), _jsx("small", { children: formatBytes(backup.sizeBytes) })] })] }), _jsxs("div", { className: "backup-record-actions", children: [_jsx("button", { type: "button", className: "secondary-button", disabled: busy ||
                                            isCreating ||
                                            isRestoring, onClick: () => onReveal(backup.path), children: "Show" }), _jsx("button", { type: "button", className: "action-button health-button", disabled: busy ||
                                            isCreating ||
                                            isRestoring ||
                                            (!restoreOpenClaw &&
                                                !restoreSettings), onClick: () => onRestoreBackup(backup.path, restoreOpenClaw, restoreSettings), children: busy &&
                                            isRestoring
                                            ? "Restoring..."
                                            : "Restore" }), deleting ? (_jsxs(_Fragment, { children: [_jsx("button", { type: "button", className: "danger-button", disabled: busy, onClick: () => {
                                                    onDelete(backup.path);
                                                    setConfirmingDelete(null);
                                                }, children: "Confirm" }), _jsx("button", { type: "button", className: "secondary-button", onClick: () => setConfirmingDelete(null), children: "Cancel" })] })) : (_jsx("button", { type: "button", className: "danger-button", disabled: busy ||
                                            isCreating ||
                                            isRestoring, onClick: () => setConfirmingDelete(backup.path), children: "Delete" }))] })] }, backup.id));
                }) }))] }));
}
export default BackupPage;
