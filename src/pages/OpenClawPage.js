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
    return value.trim().replace(/\/+$/, "");
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
        case "error":
            return "Error";
        default:
            return "Not tested";
    }
}
function connectionIcon(server) {
    switch (server.connectionState) {
        case "testing":
            return "🟡";
        case "connected":
            return "🟢";
        case "unauthorized":
            return "🟠";
        case "unreachable":
        case "error":
            return "🔴";
        default:
            return "⚪";
    }
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
function OpenClawPage({ servers, activeServer, enabledCount, connectedCount, status, busyServerId, testingServerId, remoteStatus, searchText, error, cardStyle, onSearchChange, onRefresh, onCreate, onUpdate, onDelete, onToggle, onActivate, onTestSaved, onTestUnsaved, }) {
    const [formOpen, setFormOpen,] = useState(false);
    const [editingId, setEditingId,] = useState(null);
    const [form, setForm,] = useState(EMPTY_FORM);
    const [formError, setFormError,] = useState("");
    const [testMessage, setTestMessage,] = useState("");
    const [showToken, setShowToken,] = useState(false);
    const [confirmDelete, setConfirmDelete,] = useState(null);
    const isLoading = status === "loading";
    const disabledCount = servers.length -
        enabledCount;
    const activeStatusText = useMemo(() => {
        if (!activeServer) {
            return "No active server";
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
            // 留空代表保留后端已有 Token
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
            if (parsed.protocol !==
                "http:" &&
                parsed.protocol !==
                    "https:") {
                throw new Error("Unsupported protocol");
            }
        }
        catch {
            setFormError("Enter a valid HTTP or HTTPS server URL.");
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
            const message = String(nextError);
            setFormError(message);
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
    return (_jsxs("section", { className: "page-section", children: [_jsxs("div", { className: "section-header", children: [_jsxs("div", { children: [_jsx("h2", { children: "OpenClaw Servers" }), _jsx("p", { children: "Connect AI OS to local or remote OpenClaw gateways." })] }), _jsxs("div", { className: "openclaw-header-actions", children: [_jsx("button", { type: "button", className: "secondary-button", disabled: isLoading, onClick: onRefresh, children: isLoading
                                    ? "Refreshing..."
                                    : "↻ Refresh" }), _jsx("button", { type: "button", className: "action-button backup-button", disabled: isLoading, onClick: openCreateForm, children: "\uFF0B Add Server" })] })] }), _jsxs("div", { className: "openclaw-summary-grid", children: [_jsxs("div", { className: "openclaw-summary-card", style: cardStyle, children: [_jsx("span", { children: "Total Servers" }), _jsx("strong", { children: servers.length })] }), _jsxs("div", { className: "openclaw-summary-card", style: cardStyle, children: [_jsx("span", { children: "Enabled" }), _jsx("strong", { children: enabledCount })] }), _jsxs("div", { className: "openclaw-summary-card", style: cardStyle, children: [_jsx("span", { children: "Connected" }), _jsx("strong", { children: connectedCount })] }), _jsxs("div", { className: "openclaw-summary-card", style: cardStyle, children: [_jsx("span", { children: "Disabled" }), _jsx("strong", { children: disabledCount })] })] }), _jsxs("div", { className: "openclaw-active-card", style: cardStyle, children: [_jsxs("div", { className: "openclaw-active-heading", children: [_jsxs("div", { children: [_jsx("span", { className: "openclaw-card-icon", children: "\uD83E\uDD9E" }), _jsxs("div", { children: [_jsx("h3", { children: "Active OpenClaw" }), _jsx("p", { children: "The active server is used by AI OS remote operations." })] })] }), _jsxs("span", { className: [
                                    "openclaw-active-status",
                                    remoteStatus?.connected
                                        ? "openclaw-status-connected"
                                        : "",
                                ]
                                    .filter(Boolean)
                                    .join(" "), children: [remoteStatus?.connected
                                        ? "🟢"
                                        : "⚪", " ", activeStatusText] })] }), activeServer ? (_jsxs("div", { className: "openclaw-active-details", children: [_jsxs("div", { children: [_jsx("span", { children: "Name" }), _jsx("strong", { children: activeServer.name })] }), _jsxs("div", { children: [_jsx("span", { children: "Server URL" }), _jsx("strong", { children: activeServer.serverUrl })] }), _jsxs("div", { children: [_jsx("span", { children: "Token" }), _jsx("strong", { children: activeServer
                                            .hasGatewayToken
                                            ? "Configured"
                                            : "Missing" })] }), _jsxs("div", { children: [_jsx("span", { children: "Last Checked" }), _jsx("strong", { children: formatDate(activeServer
                                            .lastCheckedAt) })] })] })) : (_jsx("div", { className: "openclaw-no-active", children: "No active OpenClaw server. Add a server or mark an existing server as active." })), remoteStatus?.rawResponse && (_jsxs("details", { className: "openclaw-raw-status", children: [_jsx("summary", { children: "Remote Response" }), _jsx("pre", { children: remoteStatus.rawResponse })] }))] }), _jsx("div", { className: "openclaw-toolbar", children: _jsx("input", { type: "search", className: "openclaw-search", value: searchText, placeholder: "Search OpenClaw servers...", onChange: (event) => onSearchChange(event.target.value) }) }), error && (_jsx("div", { className: "openclaw-error", role: "alert", children: error })), servers.length === 0 ? (_jsxs("div", { className: "openclaw-empty-state", style: cardStyle, children: [_jsx("span", { children: "\uD83E\uDD9E" }), _jsx("h3", { children: "No OpenClaw servers" }), _jsx("p", { children: "Add a local or remote OpenClaw gateway using its Server URL and Gateway Token." }), _jsx("button", { type: "button", className: "action-button backup-button", onClick: openCreateForm, children: "Add First Server" })] })) : (_jsx("div", { className: "openclaw-grid", children: servers.map((server) => {
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
                            server.enabled
                                ? "openclaw-card-enabled"
                                : "",
                        ]
                            .filter(Boolean)
                            .join(" "), style: cardStyle, children: [_jsxs("div", { className: "openclaw-card-header", children: [_jsxs("div", { className: "openclaw-card-title", children: [_jsx("span", { className: "openclaw-card-icon", children: "\uD83E\uDD9E" }), _jsxs("div", { children: [_jsxs("div", { className: "openclaw-name-row", children: [_jsx("h3", { children: server.name }), server.active && (_jsx("span", { className: "openclaw-active-badge", children: "Active" }))] }), _jsx("p", { children: server.serverUrl })] })] }), _jsxs("label", { className: "openclaw-switch", children: [_jsx("input", { type: "checkbox", checked: server.enabled, disabled: busy, onChange: (event) => onToggle(server.id, event.target
                                                    .checked) }), _jsx("span", { children: busy
                                                    ? "Updating..."
                                                    : server.enabled
                                                        ? "Enabled"
                                                        : "Disabled" })] })] }), _jsxs("div", { className: "openclaw-connection-row", children: [_jsxs("span", { children: [connectionIcon(server), " ", testing
                                                ? "Testing..."
                                                : connectionLabel(server)] }), _jsx("small", { children: server
                                            .connectionMessage ||
                                            "Connection has not been tested." })] }), _jsxs("div", { className: "openclaw-meta-grid", children: [_jsxs("div", { children: [_jsx("span", { children: "Gateway Token" }), _jsx("strong", { children: server
                                                    .hasGatewayToken
                                                    ? "Configured"
                                                    : "Missing" })] }), _jsxs("div", { children: [_jsx("span", { children: "Auto Connect" }), _jsx("strong", { children: server
                                                    .autoConnect
                                                    ? "On"
                                                    : "Off" })] }), _jsxs("div", { children: [_jsx("span", { children: "Last Checked" }), _jsx("strong", { children: formatDate(server
                                                    .lastCheckedAt) })] })] }), _jsxs("div", { className: "openclaw-card-actions", children: [!server.active && (_jsx("button", { type: "button", className: "action-button health-button", disabled: busy ||
                                            !server.enabled, onClick: () => onActivate(server.id), children: "Set Active" })), _jsx("button", { type: "button", className: "secondary-button", disabled: busy ||
                                            testing ||
                                            !server.enabled, onClick: () => onTestSaved(server.id), children: testing
                                            ? "Testing..."
                                            : "Test Connection" }), _jsx("button", { type: "button", className: "secondary-button", disabled: busy, onClick: () => openEditForm(server), children: "Edit" }), deleting ? (_jsxs(_Fragment, { children: [_jsx("button", { type: "button", className: "danger-button", disabled: busy, onClick: () => {
                                                    onDelete(server.id);
                                                    setConfirmDelete(null);
                                                }, children: "Confirm" }), _jsx("button", { type: "button", className: "secondary-button", onClick: () => setConfirmDelete(null), children: "Cancel" })] })) : (_jsx("button", { type: "button", className: "danger-button", disabled: busy, onClick: () => setConfirmDelete(server.id), children: "Delete" }))] })] }, server.id));
                }) })), formOpen && (_jsx("div", { className: "openclaw-modal-backdrop", role: "presentation", onMouseDown: (event) => {
                    if (event.target ===
                        event.currentTarget &&
                        !isLoading &&
                        testingServerId !==
                            "__new__") {
                        resetForm();
                    }
                }, children: _jsxs("div", { className: "openclaw-modal", style: cardStyle, role: "dialog", "aria-modal": "true", "aria-label": editingId
                        ? "Edit OpenClaw server"
                        : "Add OpenClaw server", children: [_jsxs("div", { className: "openclaw-modal-header", children: [_jsxs("div", { children: [_jsx("h3", { children: editingId
                                                ? "Edit OpenClaw Server"
                                                : "Add OpenClaw Server" }), _jsx("p", { children: "Configure a local or remote OpenClaw gateway." })] }), _jsx("button", { type: "button", className: "secondary-button", disabled: isLoading ||
                                        testingServerId ===
                                            "__new__", onClick: resetForm, children: "Close" })] }), _jsxs("label", { className: "setting-field", children: [_jsx("span", { children: "Name" }), _jsx("input", { type: "text", value: form.name, disabled: isLoading, placeholder: "Home OpenClaw", onChange: (event) => setForm((current) => ({
                                        ...current,
                                        name: event.target
                                            .value,
                                    })) })] }), _jsxs("label", { className: "setting-field", children: [_jsx("span", { children: "Server URL" }), _jsx("small", { children: "Example: http://127.0.0.1:18789 or https://openclaw.example.com" }), _jsx("input", { type: "url", value: form.serverUrl, disabled: isLoading, placeholder: "http://127.0.0.1:18789", onChange: (event) => setForm((current) => ({
                                        ...current,
                                        serverUrl: event.target
                                            .value,
                                    })) })] }), _jsxs("label", { className: "setting-field", children: [_jsx("span", { children: "Gateway Token" }), _jsx("small", { children: editingId
                                        ? "Leave blank to keep the existing Token."
                                        : "The Token is saved locally by the Rust backend." }), _jsxs("div", { className: "openclaw-token-field", children: [_jsx("input", { type: showToken
                                                ? "text"
                                                : "password", value: form.gatewayToken, disabled: isLoading, autoComplete: "off", placeholder: editingId
                                                ? "Leave blank to keep existing Token"
                                                : "Paste Gateway Token", onChange: (event) => setForm((current) => ({
                                                ...current,
                                                gatewayToken: event.target
                                                    .value,
                                            })) }), _jsx("button", { type: "button", className: "secondary-button", onClick: () => setShowToken((current) => !current), children: showToken
                                                ? "Hide"
                                                : "Show" })] })] }), _jsxs("label", { className: "openclaw-option-row", children: [_jsx("input", { type: "checkbox", checked: form.enabled, disabled: isLoading, onChange: (event) => setForm((current) => ({
                                        ...current,
                                        enabled: event.target
                                            .checked,
                                    })) }), "Enable this server"] }), _jsxs("label", { className: "openclaw-option-row", children: [_jsx("input", { type: "checkbox", checked: form.autoConnect, disabled: isLoading, onChange: (event) => setForm((current) => ({
                                        ...current,
                                        autoConnect: event.target
                                            .checked,
                                    })) }), "Automatically monitor this server when active"] }), testMessage && (_jsx("div", { className: "openclaw-test-message", children: testMessage })), formError && (_jsx("div", { className: "openclaw-error", role: "alert", children: formError })), _jsxs("div", { className: "openclaw-modal-actions", children: [_jsx("button", { type: "button", className: "secondary-button", disabled: isLoading ||
                                        testingServerId ===
                                            "__new__", onClick: resetForm, children: "Cancel" }), _jsx("button", { type: "button", className: "secondary-button", disabled: isLoading ||
                                        testingServerId ===
                                            "__new__", onClick: testForm, children: testingServerId ===
                                        "__new__"
                                        ? "Testing..."
                                        : "Test Connection" }), _jsx("button", { type: "button", className: "action-button backup-button", disabled: isLoading ||
                                        testingServerId ===
                                            "__new__", onClick: submitForm, children: isLoading
                                        ? "Saving..."
                                        : editingId
                                            ? "Save Changes"
                                            : "Add Server" })] })] }) }))] }));
}
export default OpenClawPage;
