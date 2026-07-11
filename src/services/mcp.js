import { invoke } from "@tauri-apps/api/core";
export async function listMcpServers() {
    return invoke("list_mcp_servers");
}
export async function saveMcpServer(server) {
    return invoke("save_mcp_server", {
        server,
    });
}
export async function updateMcpServer(id, server) {
    return invoke("update_mcp_server", {
        id,
        server,
    });
}
export async function toggleMcpServer(id, enabled) {
    return invoke("toggle_mcp_server", {
        id,
        enabled,
    });
}
export async function deleteMcpServer(id) {
    return invoke("delete_mcp_server", {
        id,
    });
}
