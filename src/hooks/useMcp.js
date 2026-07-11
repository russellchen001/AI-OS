import { useCallback, useEffect, useMemo, useState, } from "react";
import { deleteMcpServer, listMcpServers, saveMcpServer, toggleMcpServer, updateMcpServer, } from "../services/mcp";
function useMcp({ onMessage, }) {
    const [servers, setServers,] = useState([]);
    const [status, setStatus,] = useState("idle");
    const [activeServerId, setActiveServerId,] = useState(null);
    const [searchText, setSearchText,] = useState("");
    const [error, setError,] = useState("");
    const refreshServers = useCallback(async () => {
        try {
            setStatus("loading");
            setError("");
            const result = await listMcpServers();
            setServers(result);
            setStatus("success");
        }
        catch (nextError) {
            const message = `Unable to load MCP servers: ${String(nextError)}`;
            setStatus("error");
            setError(message);
        }
    }, []);
    useEffect(() => {
        refreshServers();
    }, [refreshServers]);
    const createServer = useCallback(async (server) => {
        try {
            setStatus("loading");
            setError("");
            const result = await saveMcpServer(server);
            if (!result.success) {
                throw new Error(result.message);
            }
            onMessage(`✅ ${result.message}`);
            await refreshServers();
            return result.server ??
                null;
        }
        catch (nextError) {
            const message = `Unable to add MCP server: ${String(nextError)}`;
            setStatus("error");
            setError(message);
            onMessage(`❌ ${message}`);
            throw new Error(message);
        }
        finally {
            setStatus((current) => current === "error"
                ? "error"
                : "success");
        }
    }, [
        onMessage,
        refreshServers,
    ]);
    const editServer = useCallback(async (id, server) => {
        try {
            setActiveServerId(id);
            setStatus("loading");
            setError("");
            const result = await updateMcpServer(id, server);
            if (!result.success) {
                throw new Error(result.message);
            }
            onMessage(`✅ ${result.message}`);
            await refreshServers();
            return result.server ??
                null;
        }
        catch (nextError) {
            const message = `Unable to update MCP server: ${String(nextError)}`;
            setStatus("error");
            setError(message);
            onMessage(`❌ ${message}`);
            throw new Error(message);
        }
        finally {
            setActiveServerId(null);
            setStatus((current) => current === "error"
                ? "error"
                : "success");
        }
    }, [
        onMessage,
        refreshServers,
    ]);
    const setServerEnabled = useCallback(async (id, enabled) => {
        try {
            setActiveServerId(id);
            setError("");
            const result = await toggleMcpServer(id, enabled);
            if (!result.success) {
                throw new Error(result.message);
            }
            setServers((current) => current.map((server) => server.id === id
                ? {
                    ...server,
                    enabled,
                }
                : server));
            onMessage(`🔌 ${result.message}`);
        }
        catch (nextError) {
            const message = `Unable to change MCP status: ${String(nextError)}`;
            setError(message);
            onMessage(`❌ ${message}`);
            await refreshServers();
        }
        finally {
            setActiveServerId(null);
        }
    }, [
        onMessage,
        refreshServers,
    ]);
    const removeServer = useCallback(async (id) => {
        try {
            setActiveServerId(id);
            setError("");
            const result = await deleteMcpServer(id);
            if (!result.success) {
                throw new Error(result.message);
            }
            setServers((current) => current.filter((server) => server.id !== id));
            onMessage(`🗑️ ${result.message}`);
        }
        catch (nextError) {
            const message = `Unable to delete MCP server: ${String(nextError)}`;
            setError(message);
            onMessage(`❌ ${message}`);
        }
        finally {
            setActiveServerId(null);
        }
    }, [onMessage]);
    const filteredServers = useMemo(() => {
        const search = searchText
            .trim()
            .toLowerCase();
        if (!search) {
            return servers;
        }
        return servers.filter((server) => server.name
            .toLowerCase()
            .includes(search) ||
            server.description
                .toLowerCase()
                .includes(search) ||
            server.transport
                .toLowerCase()
                .includes(search) ||
            server.command
                ?.toLowerCase()
                .includes(search) ||
            server.url
                ?.toLowerCase()
                .includes(search));
    }, [
        searchText,
        servers,
    ]);
    const enabledCount = useMemo(() => servers.filter((server) => server.enabled).length, [servers]);
    return {
        servers,
        filteredServers,
        enabledCount,
        status,
        activeServerId,
        searchText,
        error,
        setSearchText,
        refreshServers,
        createServer,
        editServer,
        setServerEnabled,
        removeServer,
    };
}
export default useMcp;
