import { invoke, } from "@tauri-apps/api/core";
export async function listOpenClawServers() {
    return invoke("list_openclaw_servers");
}
export async function saveOpenClawServer(server) {
    return invoke("save_openclaw_server", {
        server,
    });
}
export async function updateOpenClawServer(id, server) {
    return invoke("update_openclaw_server", {
        id,
        server,
    });
}
export async function deleteOpenClawServer(id) {
    return invoke("delete_openclaw_server", {
        id,
    });
}
export async function duplicateOpenClawServer(id) {
    return invoke("duplicate_openclaw_server", {
        id,
    });
}
export async function toggleOpenClawServer(id, enabled) {
    return invoke("toggle_openclaw_server", {
        id,
        enabled,
    });
}
export async function setActiveOpenClawServer(id) {
    return invoke("set_active_openclaw_server", {
        id,
    });
}
export async function testOpenClawConnection(id) {
    return invoke("test_openclaw_connection", {
        id,
    });
}
export async function testOpenClawConnectionInput(server) {
    return invoke("test_openclaw_connection_input", {
        server,
    });
}
export async function testAllOpenClawServers() {
    return invoke("test_all_openclaw_servers");
}
export async function getActiveOpenClawStatus() {
    return invoke("get_active_openclaw_status");
}
export async function getOpenClawDashboardSummary() {
    return invoke("get_openclaw_dashboard_summary");
}
export async function getOpenClawRuntimeConfig() {
    return invoke("get_openclaw_runtime_config");
}
export async function exportOpenClawServers(includeSecrets = false) {
    return invoke("export_openclaw_servers", {
        includeSecrets,
    });
}
export async function importOpenClawServers(json, replaceExisting = false) {
    return invoke("import_openclaw_servers", {
        json,
        replaceExisting,
    });
}
export async function invokeActiveOpenClawGateway(request) {
    return invoke("invoke_active_openclaw_gateway", {
        request,
    });
}
