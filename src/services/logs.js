import { invoke } from "@tauri-apps/api/core";
export async function getLogs(query) {
    return invoke("get_logs", {
        query,
    });
}
export async function clearLogs(source) {
    return invoke("clear_logs", {
        source,
    });
}
