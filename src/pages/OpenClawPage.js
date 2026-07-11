import { jsx as _jsx, jsxs as _jsxs, Fragment as _Fragment } from "react/jsx-runtime";
import { useMemo, useState, } from "react";
const EMPTY_FORM = {
    name: "",
    serverUrl: "",
    gatewayToken: "",
    enabled: true,
    autoConnect: true,
};
function normalizeServerUrl(value) {
    return value
        .trim()
        .replace(/\/+$/, "");
}
function formatDate(value) {
    if (!value) {
        return "Never";
    }
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) {
        return value;
    }
    return date.toLocaleString();
}
function connectionLabel(server) {
    switch (server.connectionState) {
        case "testing":
            return "Testing";
        case "connected":
            return "Connected";
        case "unauthorized":
            return "Unauthorized";
        case "unreachable":
            return "Unreachable";
        case "pairing-required":
            return "Pairing Required";
        case "error":
            return "Error";
        default:
            return "Not Tested";
    }
}
function connectionIcon(server) {
    switch (server.connectionState) {
        case "testing":
            return "🟡";
        case "connected":
            return "🟢";
        case "unauthorized":
        case "pairing-required":
            return "🟠";
        case "unreachable":
        case "error":
            return "🔴";
        default:
            return "⚪";
    }
}
function OpenClawPage({ servers, activeServer, enabledCount, connectedCount, autoConnectCount, averageLatencyMs, status, busyServerId, testingServerId, isTestingAll, isImporting, isExporting, remoteStatus, runtimeConfig, searchText, error, cardStyle, onSearchChange, onRefresh, onCreate, onUpdate, onDelete, onDuplicate, onToggle, onActivate, onTestSaved, onTestUnsaved, onTestAll, onCopyUrl, onExport, onImport, }) {
    const [formOpen, setFormOpen,] = useState(false);
    const [editingId, setEditingId,] = useState(null);
    const [form, setForm,] = useState(EMPTY_FORM);
    const [formError, setFormError,] = useState("");
    const [testMessage, setTestMessage,] = useState("");
    const [showToken, setShowToken,] = useState(false);
    const [confirmDelete, setConfirmDelete,] = useState(null);
    const [transferOpen, setTransferOpen,] = useState(false);
    const [transferMode, setTransferMode,] = useState("export");
    const [transferJson, setTransferJson,] = useState("");
    const [transferError, setTransferError,] = useState("");
    const [includeSecrets, setIncludeSecrets,] = useState(false);
    const [replaceExisting, setReplaceExisting,] = useState(false);
    const isLoading = status === "loading";
    const disabledCount = servers.length -
        enabledCount;
    const activeStatusText = useMemo(() => {
        if (!activeServer) {
            return "No Active Server";
        }
        if (remoteStatus?.connected) {
            return "Connected";
        }
        return connectionLabel(activeServer);
    }, [
        activeServer,
        remoteStatus,
    ]);
    function resetForm() {
        setForm(EMPTY_FORM);
        setEditingId(null);
        setFormError("");
        setTestMessage("");
        setShowToken(false);
        setFormOpen(false);
    }
    function openCreateForm() {
        setForm(EMPTY_FORM);
        setEditingId(null);
        setFormError("");
        setTestMessage("");
        setShowToken(false);
        setFormOpen(true);
    }
    function openEditForm(server) {
        setEditingId(server.id);
        setForm({
            name: server.name,
            serverUrl: server.serverUrl,
            gatewayToken: "",
            enabled: server.enabled,
            autoConnect: server.autoConnect,
        });
        setFormError("");
        setTestMessage("");
        setShowToken(false);
        setFormOpen(true);
    }
    function validateForm() {
        const name = form.name.trim();
        const serverUrl = normalizeServerUrl(form.serverUrl);
        if (!name) {
            setFormError("Server name is required.");
            return null;
        }
        if (!serverUrl) {
            setFormError("Server URL is required.");
            return null;
        }
        try {
            const parsed = new URL(serverUrl);
            if (![
                "http:",
                "https:",
                "ws:",
                "wss:",
            ].includes(parsed.protocol)) {
                throw new Error("Unsupported protocol");
            }
        }
        catch {
            setFormError("Enter a valid HTTP, HTTPS, WS or WSS URL.");
            return null;
        }
        if (!editingId &&
            !form.gatewayToken.trim()) {
            setFormError("Gateway Token is required for a new server.");
            return null;
        }
        return {
            name,
            serverUrl,
            gatewayToken: form.gatewayToken.trim(),
            enabled: form.enabled,
            autoConnect: form.autoConnect,
        };
    }
    async function testForm() {
        const payload = validateForm();
        if (!payload) {
            return;
        }
        try {
            setFormError("");
            setTestMessage("Testing connection...");
            const result = await onTestUnsaved(payload);
            setTestMessage(result.message);
            if (!result.success) {
                setFormError(result.message);
            }
        }
        catch (nextError) {
            setFormError(String(nextError));
            setTestMessage("");
        }
    }
    async function submitForm() {
        const payload = validateForm();
        if (!payload) {
            return;
        }
        try {
            setFormError("");
            if (editingId) {
                await onUpdate(editingId, payload);
            }
            else {
                await onCreate(payload);
            }
            resetForm();
        }
        catch (nextError) {
            setFormError(String(nextError));
        }
    }
    function closeTransfer() {
        if (isImporting ||
            isExporting) {
            return;
        }
        setTransferOpen(false);
        setTransferJson("");
        setTransferError("");
        setIncludeSecrets(false);
        setReplaceExisting(false);
    }
    function openExport() {
        setTransferMode("export");
        setTransferJson("");
        setTransferError("");
        setIncludeSecrets(false);
        setTransferOpen(true);
    }
    function openImport() {
        setTransferMode("import");
        setTransferJson("");
        setTransferError("");
        setReplaceExisting(false);
        setTransferOpen(true);
    }
    async function createExport() {
        try {
            setTransferError("");
            const json = await onExport(includeSecrets);
            setTransferJson(json);
        }
        catch (nextError) {
            setTransferError(String(nextError));
        }
    }
    async function runImport() {
        if (!transferJson.trim()) {
            setTransferError("Paste an OpenClaw export document first.");
            return;
        }
        try {
            setTransferError("");
            await onImport({
                json: transferJson,
                replaceExisting,
            });
            closeTransfer();
        }
        catch (nextError) {
            setTransferError(String(nextError));
        }
    }
    return (_jsxs("section", { className: "page-section", children: [_jsxs("div", { className: "section-header", children: [_jsxs("div", { children: [_jsx("h2", { children: "OpenClaw Manager" }), _jsx("p", { children: "Manage local and remote OpenClaw Gateway endpoints." })] }), _jsxs("div", { className: "openclaw-header-actions", children: [_jsx("button", { type: "button", className: "secondary-button", disabled: isLoading ||
                                    isTestingAll, onClick: () => {
                                    void onTestAll();
                                }, children: isTestingAll
                                    ? "Testing All..."
                                    : "Test All" }), _jsx("button", { type: "button", className: "secondary-button", disabled: isLoading, onClick: openImport, children: "Import" }), _jsx("button", { type: "button", className: "secondary-button", disabled: isLoading, onClick: openExport, children: "Export" }), _jsx("button", { type: "button", className: "secondary-button", disabled: isLoading, onClick: onRefresh, children: isLoading
                                    ? "Refreshing..."
                                    : "↻ Refresh" }), _jsx("button", { type: "button", className: "action-button backup-button", disabled: isLoading, onClick: openCreateForm, children: "\uFF0B Add Server" })] })] }), _jsxs("div", { className: "openclaw-summary-grid", children: [_jsxs("div", { className: "openclaw-summary-card", style: cardStyle, children: [_jsx("span", { children: "Total Servers" }), _jsx("strong", { children: servers.length })] }), _jsxs("div", { className: "openclaw-summary-card", style: cardStyle, children: [_jsx("span", { children: "Connected" }), _jsx("strong", { children: connectedCount })] }), _jsxs("div", { className: "openclaw-summary-card", style: cardStyle, children: [_jsx("span", { children: "Auto Connect" }), _jsx("strong", { children: autoConnectCount })] }), _jsxs("div", { className: "openclaw-summary-card", style: cardStyle, children: [_jsx("span", { children: "Average Latency" }), _jsx("strong", { children: averageLatencyMs === null
                                    ? "—"
                                    : `${averageLatencyMs} ms` })] })] }), _jsxs("div", { className: "openclaw-active-card", style: cardStyle, children: [_jsxs("div", { className: "openclaw-active-heading", children: [_jsxs("div", { children: [_jsx("span", { className: "openclaw-card-icon", children: "\uD83E\uDD9E" }), _jsxs("div", { children: [_jsx("h3", { children: "Active Gateway" }), _jsx("p", { children: "All unified OpenClaw requests use this endpoint." })] })] }), _jsxs("span", { className: [
                                    "openclaw-active-status",
                                    remoteStatus?.connected
                                        ? "openclaw-status-connected"
                                        : "",
                                ]
                                    .filter(Boolean)
                                    .join(" "), children: [remoteStatus?.connected
                                        ? "🟢"
                                        : "⚪", " ", activeStatusText] })] }), activeServer ? (_jsxs("div", { className: "openclaw-active-details", children: [_jsxs("div", { children: [_jsx("span", { children: "Server" }), _jsx("strong", { children: activeServer.name })] }), _jsxs("div", { children: [_jsx("span", { children: "Runtime Mode" }), _jsx("strong", { children: runtimeConfig?.mode
                                            ?? "Unknown" })] }), _jsxs("div", { children: [_jsx("span", { children: "Version" }), _jsx("strong", { children: remoteStatus?.version
                                            ?? activeServer.version
                                            ?? "Unknown" })] }), _jsxs("div", { children: [_jsx("span", { children: "Latency" }), _jsxs("strong", { children: [remoteStatus?.latencyMs
                                                ?? activeServer.latencyMs
                                                ?? "—", " ", (remoteStatus?.latencyMs
                                                ?? activeServer.latencyMs) !== undefined
                                                ? "ms"
                                                : ""] })] })] })) : (_jsx("div", { className: "openclaw-no-active", children: "No active OpenClaw server. Add or enable a server to continue." }))] }), _jsxs("div", { className: "openclaw-toolbar", children: [_jsxs("div", { children: [_jsx("strong", { children: enabledCount }), " ", "enabled \u00B7", " ", _jsx("strong", { children: disabledCount }), " ", "disabled"] }), _jsx("input", { type: "search", className: "openclaw-search", value: searchText, placeholder: "Search name, URL, version or state...", onChange: (event) => onSearchChange(event.target.value) })] }), error && (_jsx("div", { className: "openclaw-error", role: "alert", children: error })), servers.length === 0 ? (_jsxs("div", { className: "openclaw-empty-state", style: cardStyle, children: [_jsx("span", { children: "\uD83E\uDD9E" }), _jsx("h3", { children: "No OpenClaw servers" }), _jsx("p", { children: "Add a local or remote Gateway using its URL and Token." }), _jsx("button", { type: "button", className: "action-button backup-button", onClick: openCreateForm, children: "Add First Server" })] })) : (_jsx("div", { className: "openclaw-grid", children: servers.map((server) => {
                    const busy = busyServerId ===
                        server.id;
                    const testing = testingServerId ===
                        server.id;
                    const deleting = confirmDelete ===
                        server.id;
                    return (_jsxs("article", { className: [
                            "openclaw-card",
                            server.active
                                ? "openclaw-card-active"
                                : "",
                        ]
                            .filter(Boolean)
                            .join(" "), style: cardStyle, children: [_jsxs("div", { className: "openclaw-card-header", children: [_jsxs("div", { className: "openclaw-card-title", children: [_jsx("span", { className: "openclaw-card-icon", children: "\uD83E\uDD9E" }), _jsxs("div", { children: [_jsxs("div", { className: "openclaw-name-row", children: [_jsx("h3", { children: server.name }), server.active && (_jsx("span", { className: "openclaw-active-badge", children: "Active" }))] }), _jsx("p", { children: server.serverUrl })] })] }), _jsxs("label", { className: "openclaw-switch", children: [_jsx("input", { type: "checkbox", checked: server.enabled, disabled: busy, onChange: (event) => onToggle(server.id, event.target.checked) }), _jsx("span", { children: busy
                                                    ? "Updating..."
                                                    : server.enabled
                                                        ? "Enabled"
                                                        : "Disabled" })] })] }), _jsxs("div", { className: "openclaw-connection-row", children: [_jsxs("span", { children: [connectionIcon(server), " ", testing
                                                ? "Testing..."
                                                : connectionLabel(server)] }), _jsx("small", { children: server.connectionMessage ||
                                            "Connection has not been tested." })] }), _jsxs("div", { className: "openclaw-meta-grid", children: [_jsxs("div", { children: [_jsx("span", { children: "Version" }), _jsx("strong", { children: server.version
                                                    ?? "Unknown" })] }), _jsxs("div", { children: [_jsx("span", { children: "Latency" }), _jsx("strong", { children: typeof server.latencyMs ===
                                                    "number"
                                                    ? `${server.latencyMs} ms`
                                                    : "—" })] }), _jsxs("div", { children: [_jsx("span", { children: "Last Checked" }), _jsx("strong", { children: formatDate(server.lastCheckedAt) })] }), _jsxs("div", { children: [_jsx("span", { children: "Gateway Token" }), _jsx("strong", { children: server.hasGatewayToken
                                                    ? "Configured"
                                                    : "Missing" })] }), _jsxs("div", { children: [_jsx("span", { children: "Auto Connect" }), _jsx("strong", { children: server.autoConnect
                                                    ? "On"
                                                    : "Off" })] }), _jsxs("div", { children: [_jsx("span", { children: "Gateway ID" }), _jsx("strong", { children: server.gatewayId
                                                    ?? "Unknown" })] })] }), _jsxs("div", { className: "openclaw-card-actions", children: [!server.active && (_jsx("button", { type: "button", className: "action-button health-button", disabled: busy ||
                                            !server.enabled, onClick: () => onActivate(server.id), children: "Set Active" })), _jsx("button", { type: "button", className: "secondary-button", disabled: busy ||
                                            testing ||
                                            !server.enabled, onClick: () => {
                                            void onTestSaved(server.id);
                                        }, children: testing
                                            ? "Testing..."
                                            : "Test" }), _jsx("button", { type: "button", className: "secondary-button", disabled: busy, onClick: () => onCopyUrl(server), children: "Copy URL" }), _jsx("button", { type: "button", className: "secondary-button", disabled: busy, onClick: () => {
                                            void onDuplicate(server.id);
                                        }, children: "Duplicate" }), _jsx("button", { type: "button", className: "secondary-button", disabled: busy, onClick: () => openEditForm(server), children: "Edit" }), deleting ? (_jsxs(_Fragment, { children: [_jsx("button", { type: "button", className: "danger-button", disabled: busy, onClick: () => {
                                                    onDelete(server.id);
                                                    setConfirmDelete(null);
                                                }, children: "Confirm" }), _jsx("button", { type: "button", className: "secondary-button", onClick: () => setConfirmDelete(null), children: "Cancel" })] })) : (_jsx("button", { type: "button", className: "danger-button", disabled: busy, onClick: () => setConfirmDelete(server.id), children: "Delete" }))] })] }, server.id));
                }) })), formOpen && (_jsx("div", { className: "openclaw-modal-backdrop", children: _jsxs("div", { className: "openclaw-modal", style: cardStyle, role: "dialog", "aria-modal": "true", children: [_jsxs("div", { className: "openclaw-modal-header", children: [_jsxs("div", { children: [_jsx("h3", { children: editingId
                                                ? "Edit OpenClaw Server"
                                                : "Add OpenClaw Server" }), _jsx("p", { children: "Configure the Gateway endpoint and authentication." })] }), _jsx("button", { type: "button", className: "secondary-button", onClick: resetForm, children: "Close" })] }), _jsxs("label", { className: "setting-field", children: [_jsx("span", { children: "Name" }), _jsx("input", { type: "text", value: form.name, placeholder: "Home OpenClaw", onChange: (event) => setForm((current) => ({
                                        ...current,
                                        name: event.target.value,
                                    })) })] }), _jsxs("label", { className: "setting-field", children: [_jsx("span", { children: "Server URL" }), _jsx("input", { type: "url", value: form.serverUrl, placeholder: "http://127.0.0.1:18789", onChange: (event) => setForm((current) => ({
                                        ...current,
                                        serverUrl: event.target.value,
                                    })) })] }), _jsxs("label", { className: "setting-field", children: [_jsx("span", { children: "Gateway Token" }), _jsx("small", { children: editingId
                                        ? "Leave blank to keep the existing Token."
                                        : "The Token is stored by the Rust backend." }), _jsxs("div", { className: "openclaw-token-field", children: [_jsx("input", { type: showToken
                                                ? "text"
                                                : "password", value: form.gatewayToken, autoComplete: "off", onChange: (event) => setForm((current) => ({
                                                ...current,
                                                gatewayToken: event.target.value,
                                            })) }), _jsx("button", { type: "button", className: "secondary-button", onClick: () => setShowToken((current) => !current), children: showToken
                                                ? "Hide"
                                                : "Show" })] })] }), _jsxs("label", { className: "openclaw-option-row", children: [_jsx("input", { type: "checkbox", checked: form.enabled, onChange: (event) => setForm((current) => ({
                                        ...current,
                                        enabled: event.target.checked,
                                    })) }), "Enable this server"] }), _jsxs("label", { className: "openclaw-option-row", children: [_jsx("input", { type: "checkbox", checked: form.autoConnect, onChange: (event) => setForm((current) => ({
                                        ...current,
                                        autoConnect: event.target.checked,
                                    })) }), "Automatically monitor this server"] }), testMessage && (_jsx("div", { className: "openclaw-test-message", children: testMessage })), formError && (_jsx("div", { className: "openclaw-error", children: formError })), _jsxs("div", { className: "openclaw-modal-actions", children: [_jsx("button", { type: "button", className: "secondary-button", onClick: resetForm, children: "Cancel" }), _jsx("button", { type: "button", className: "secondary-button", disabled: testingServerId ===
                                        "__new__", onClick: () => {
                                        void testForm();
                                    }, children: testingServerId ===
                                        "__new__"
                                        ? "Testing..."
                                        : "Test Connection" }), _jsx("button", { type: "button", className: "action-button backup-button", disabled: isLoading, onClick: () => {
                                        void submitForm();
                                    }, children: editingId
                                        ? "Save Changes"
                                        : "Add Server" })] })] }) })), transferOpen && (_jsx("div", { className: "openclaw-modal-backdrop", children: _jsxs("div", { className: "openclaw-modal", style: cardStyle, role: "dialog", "aria-modal": "true", children: [_jsxs("div", { className: "openclaw-modal-header", children: [_jsxs("div", { children: [_jsx("h3", { children: transferMode ===
                                                "export"
                                                ? "Export Servers"
                                                : "Import Servers" }), _jsx("p", { children: transferMode ===
                                                "export"
                                                ? "Create a portable JSON configuration."
                                                : "Paste an exported OpenClaw JSON document." })] }), _jsx("button", { type: "button", className: "secondary-button", onClick: closeTransfer, children: "Close" })] }), transferMode ===
                            "export" ? (_jsxs(_Fragment, { children: [_jsxs("label", { className: "openclaw-option-row", children: [_jsx("input", { type: "checkbox", checked: includeSecrets, onChange: (event) => setIncludeSecrets(event.target.checked) }), "Include Gateway Tokens in export"] }), _jsx("button", { type: "button", className: "secondary-button", disabled: isExporting, onClick: () => {
                                        void createExport();
                                    }, children: isExporting
                                        ? "Exporting..."
                                        : "Generate JSON" })] })) : (_jsxs("label", { className: "openclaw-option-row", children: [_jsx("input", { type: "checkbox", checked: replaceExisting, onChange: (event) => setReplaceExisting(event.target.checked) }), "Replace all existing servers"] })), _jsxs("label", { className: "setting-field", children: [_jsx("span", { children: "JSON" }), _jsx("textarea", { rows: 15, value: transferJson, readOnly: transferMode ===
                                        "export", placeholder: transferMode ===
                                        "export"
                                        ? "Click Generate JSON."
                                        : "Paste export JSON here...", onChange: (event) => setTransferJson(event.target.value) })] }), transferError && (_jsx("div", { className: "openclaw-error", children: transferError })), _jsxs("div", { className: "openclaw-modal-actions", children: [_jsx("button", { type: "button", className: "secondary-button", onClick: closeTransfer, children: "Cancel" }), transferMode ===
                                    "import" && (_jsx("button", { type: "button", className: "action-button backup-button", disabled: isImporting, onClick: () => {
                                        void runImport();
                                    }, children: isImporting
                                        ? "Importing..."
                                        : "Import Servers" }))] })] }) }))] }));
}
export default OpenClawPage;
