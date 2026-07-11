import { jsx as _jsx, jsxs as _jsxs, Fragment as _Fragment } from "react/jsx-runtime";
import { useMemo, useState, } from "react";
const EMPTY_FORM = {
    name: "",
    description: "",
    enabled: true,
    transport: "stdio",
    command: "",
    args: [],
    url: "",
    environment: {},
};
function argsToText(args) {
    return args.join("\n");
}
function textToArgs(value) {
    return value
        .split("\n")
        .map((item) => item.trim())
        .filter(Boolean);
}
function environmentToText(environment) {
    return Object.entries(environment)
        .map(([key, value]) => `${key}=${value}`)
        .join("\n");
}
function textToEnvironment(value) {
    const result = {};
    for (const line of value.split("\n")) {
        const trimmed = line.trim();
        if (!trimmed) {
            continue;
        }
        const separator = trimmed.indexOf("=");
        if (separator <= 0) {
            continue;
        }
        const key = trimmed
            .slice(0, separator)
            .trim();
        const environmentValue = trimmed
            .slice(separator + 1)
            .trim();
        if (key) {
            result[key] =
                environmentValue;
        }
    }
    return result;
}
function McpPage({ servers, enabledCount, status, activeServerId, searchText, error, cardStyle, onSearchChange, onRefresh, onCreate, onUpdate, onToggle, onDelete, }) {
    const [formOpen, setFormOpen,] = useState(false);
    const [editingId, setEditingId,] = useState(null);
    const [form, setForm,] = useState(EMPTY_FORM);
    const [argsText, setArgsText,] = useState("");
    const [environmentText, setEnvironmentText,] = useState("");
    const [formError, setFormError,] = useState("");
    const [confirmDelete, setConfirmDelete,] = useState(null);
    const isLoading = status === "loading";
    const disabledCount = servers.length -
        enabledCount;
    const transportCounts = useMemo(() => {
        return servers.reduce((counts, server) => {
            counts[server.transport] += 1;
            return counts;
        }, {
            stdio: 0,
            http: 0,
            sse: 0,
        });
    }, [servers]);
    function resetForm() {
        setForm(EMPTY_FORM);
        setArgsText("");
        setEnvironmentText("");
        setEditingId(null);
        setFormError("");
        setFormOpen(false);
    }
    function openCreateForm() {
        setForm(EMPTY_FORM);
        setArgsText("");
        setEnvironmentText("");
        setEditingId(null);
        setFormError("");
        setFormOpen(true);
    }
    function openEditForm(server) {
        setEditingId(server.id);
        setForm({
            name: server.name,
            description: server.description,
            enabled: server.enabled,
            transport: server.transport,
            command: server.command ?? "",
            args: server.args,
            url: server.url ?? "",
            environment: server.environment,
        });
        setArgsText(argsToText(server.args));
        setEnvironmentText(environmentToText(server.environment));
        setFormError("");
        setFormOpen(true);
    }
    async function submitForm() {
        const name = form.name.trim();
        if (!name) {
            setFormError("Server name is required.");
            return;
        }
        if (form.transport ===
            "stdio" &&
            !form.command?.trim()) {
            setFormError("A command is required for stdio servers.");
            return;
        }
        if (form.transport !==
            "stdio" &&
            !form.url?.trim()) {
            setFormError("A URL is required for HTTP and SSE servers.");
            return;
        }
        const payload = {
            ...form,
            name,
            description: form.description.trim(),
            command: form.transport ===
                "stdio"
                ? form.command?.trim()
                : undefined,
            url: form.transport ===
                "stdio"
                ? undefined
                : form.url?.trim(),
            args: form.transport ===
                "stdio"
                ? textToArgs(argsText)
                : [],
            environment: textToEnvironment(environmentText),
        };
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
    return (_jsxs("section", { className: "page-section", children: [_jsxs("div", { className: "section-header", children: [_jsxs("div", { children: [_jsx("h2", { children: "MCP Servers" }), _jsx("p", { children: "Manage Model Context Protocol integrations for local AI tools." })] }), _jsxs("div", { className: "mcp-header-actions", children: [_jsx("button", { type: "button", className: "secondary-button", disabled: isLoading, onClick: onRefresh, children: isLoading
                                    ? "Refreshing..."
                                    : "↻ Refresh" }), _jsx("button", { type: "button", className: "action-button backup-button", disabled: isLoading, onClick: openCreateForm, children: "\uFF0B Add Server" })] })] }), _jsxs("div", { className: "mcp-summary-grid", children: [_jsxs("div", { className: "mcp-summary-card", style: cardStyle, children: [_jsx("span", { children: "Total Servers" }), _jsx("strong", { children: servers.length })] }), _jsxs("div", { className: "mcp-summary-card mcp-summary-enabled", style: cardStyle, children: [_jsx("span", { children: "Enabled" }), _jsx("strong", { children: enabledCount })] }), _jsxs("div", { className: "mcp-summary-card", style: cardStyle, children: [_jsx("span", { children: "Disabled" }), _jsx("strong", { children: disabledCount })] }), _jsxs("div", { className: "mcp-summary-card", style: cardStyle, children: [_jsx("span", { children: "Transports" }), _jsxs("strong", { children: [transportCounts.stdio, " /", transportCounts.http, " /", transportCounts.sse] }), _jsx("small", { children: "stdio / http / sse" })] })] }), _jsx("div", { className: "mcp-toolbar", children: _jsx("input", { type: "search", className: "mcp-search", value: searchText, placeholder: "Search MCP servers...", onChange: (event) => onSearchChange(event.target.value) }) }), error && (_jsx("div", { className: "mcp-error", role: "alert", children: error })), servers.length === 0 ? (_jsxs("div", { className: "mcp-empty-state", style: cardStyle, children: [_jsx("span", { children: "\uD83D\uDD0C" }), _jsx("h3", { children: "No MCP servers" }), _jsx("p", { children: "Add a server to connect AI OS with local tools and external services." }), _jsx("button", { type: "button", className: "action-button backup-button", onClick: openCreateForm, children: "Add First Server" })] })) : (_jsx("div", { className: "mcp-grid", children: servers.map((server) => {
                    const busy = activeServerId ===
                        server.id;
                    const deleting = confirmDelete ===
                        server.id;
                    return (_jsxs("article", { className: [
                            "mcp-card",
                            server.enabled
                                ? "mcp-card-enabled"
                                : "",
                        ]
                            .filter(Boolean)
                            .join(" "), style: cardStyle, children: [_jsxs("div", { className: "mcp-card-header", children: [_jsxs("div", { className: "mcp-card-title", children: [_jsx("span", { className: "mcp-card-icon", children: "\uD83D\uDD0C" }), _jsxs("div", { children: [_jsx("h3", { children: server.name }), _jsx("p", { children: server.description })] })] }), _jsxs("label", { className: "mcp-switch", children: [_jsx("input", { type: "checkbox", checked: server.enabled, disabled: busy, onChange: (event) => onToggle(server.id, event.target
                                                    .checked) }), _jsx("span", { children: busy
                                                    ? "Updating..."
                                                    : server.enabled
                                                        ? "Enabled"
                                                        : "Disabled" })] })] }), _jsxs("div", { className: "mcp-meta-grid", children: [_jsxs("div", { children: [_jsx("span", { children: "Transport" }), _jsx("strong", { children: server.transport })] }), _jsxs("div", { children: [_jsx("span", { children: "Endpoint" }), _jsx("strong", { children: server.transport ===
                                                    "stdio"
                                                    ? server.command
                                                    : server.url })] }), _jsxs("div", { children: [_jsx("span", { children: "Arguments" }), _jsx("strong", { children: server.args
                                                    .length })] }), _jsxs("div", { children: [_jsx("span", { children: "Environment" }), _jsx("strong", { children: Object.keys(server.environment).length })] })] }), server.transport ===
                                "stdio" &&
                                server.args
                                    .length > 0 && (_jsx("div", { className: "mcp-command-preview", children: _jsxs("code", { children: [server.command, " ", server.args.join(" ")] }) })), _jsxs("div", { className: "mcp-card-actions", children: [_jsx("button", { type: "button", className: "secondary-button", disabled: busy, onClick: () => openEditForm(server), children: "Edit" }), deleting ? (_jsxs(_Fragment, { children: [_jsx("button", { type: "button", className: "danger-button", disabled: busy, onClick: () => {
                                                    onDelete(server.id);
                                                    setConfirmDelete(null);
                                                }, children: "Confirm" }), _jsx("button", { type: "button", className: "secondary-button", onClick: () => setConfirmDelete(null), children: "Cancel" })] })) : (_jsx("button", { type: "button", className: "danger-button", disabled: busy, onClick: () => setConfirmDelete(server.id), children: "Delete" }))] })] }, server.id));
                }) })), formOpen && (_jsx("div", { className: "mcp-modal-backdrop", role: "presentation", onMouseDown: (event) => {
                    if (event.target ===
                        event.currentTarget &&
                        !isLoading) {
                        resetForm();
                    }
                }, children: _jsxs("div", { className: "mcp-modal", style: cardStyle, role: "dialog", "aria-modal": "true", "aria-label": editingId
                        ? "Edit MCP server"
                        : "Add MCP server", children: [_jsxs("div", { className: "mcp-modal-header", children: [_jsxs("div", { children: [_jsx("h3", { children: editingId
                                                ? "Edit MCP Server"
                                                : "Add MCP Server" }), _jsx("p", { children: "Configure a stdio, HTTP or SSE MCP integration." })] }), _jsx("button", { type: "button", className: "secondary-button", disabled: isLoading, onClick: resetForm, children: "Close" })] }), _jsxs("div", { className: "mcp-form-grid", children: [_jsxs("label", { className: "setting-field", children: [_jsx("span", { children: "Name" }), _jsx("input", { type: "text", value: form.name, disabled: isLoading, placeholder: "Filesystem", onChange: (event) => setForm((current) => ({
                                                ...current,
                                                name: event.target
                                                    .value,
                                            })) })] }), _jsxs("label", { className: "setting-field", children: [_jsx("span", { children: "Transport" }), _jsxs("select", { value: form.transport, disabled: isLoading, onChange: (event) => setForm((current) => ({
                                                ...current,
                                                transport: event.target
                                                    .value,
                                            })), children: [_jsx("option", { value: "stdio", children: "stdio" }), _jsx("option", { value: "http", children: "HTTP" }), _jsx("option", { value: "sse", children: "SSE" })] })] })] }), _jsxs("label", { className: "setting-field", children: [_jsx("span", { children: "Description" }), _jsx("textarea", { rows: 3, value: form.description, disabled: isLoading, placeholder: "Describe what this MCP server provides.", onChange: (event) => setForm((current) => ({
                                        ...current,
                                        description: event.target
                                            .value,
                                    })) })] }), form.transport ===
                            "stdio" ? (_jsxs(_Fragment, { children: [_jsxs("label", { className: "setting-field", children: [_jsx("span", { children: "Command" }), _jsx("input", { type: "text", value: form.command ??
                                                "", disabled: isLoading, placeholder: "npx", onChange: (event) => setForm((current) => ({
                                                ...current,
                                                command: event.target
                                                    .value,
                                            })) })] }), _jsxs("label", { className: "setting-field", children: [_jsx("span", { children: "Arguments" }), _jsx("small", { children: "Enter one argument per line." }), _jsx("textarea", { rows: 5, value: argsText, disabled: isLoading, placeholder: "-y\n@modelcontextprotocol/server-filesystem\n/Users/name/Documents", onChange: (event) => setArgsText(event.target
                                                .value) })] })] })) : (_jsxs("label", { className: "setting-field", children: [_jsx("span", { children: "Server URL" }), _jsx("input", { type: "url", value: form.url ?? "", disabled: isLoading, placeholder: "http://localhost:3001/mcp", onChange: (event) => setForm((current) => ({
                                        ...current,
                                        url: event.target
                                            .value,
                                    })) })] })), _jsxs("label", { className: "setting-field", children: [_jsx("span", { children: "Environment Variables" }), _jsx("small", { children: "Enter one KEY=value pair per line. Secrets are stored locally." }), _jsx("textarea", { rows: 5, value: environmentText, disabled: isLoading, placeholder: "API_KEY=\nGITHUB_TOKEN=", onChange: (event) => setEnvironmentText(event.target
                                        .value) })] }), _jsxs("label", { className: "mcp-enabled-option", children: [_jsx("input", { type: "checkbox", checked: form.enabled, disabled: isLoading, onChange: (event) => setForm((current) => ({
                                        ...current,
                                        enabled: event.target
                                            .checked,
                                    })) }), "Enable this server"] }), formError && (_jsx("div", { className: "mcp-error", role: "alert", children: formError })), _jsxs("div", { className: "mcp-modal-actions", children: [_jsx("button", { type: "button", className: "secondary-button", disabled: isLoading, onClick: resetForm, children: "Cancel" }), _jsx("button", { type: "button", className: "action-button backup-button", disabled: isLoading, onClick: submitForm, children: isLoading
                                        ? "Saving..."
                                        : editingId
                                            ? "Save Changes"
                                            : "Add Server" })] })] }) }))] }));
}
export default McpPage;
