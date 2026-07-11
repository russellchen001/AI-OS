import { invoke } from "@tauri-apps/api/core";
export async function fetchHealthStatus() {
    return invoke("health_check");
}
export async function fetchSystemMetrics() {
    return invoke("system_metrics");
}
export async function startAllServices() {
    return invoke("start_all");
}
export async function stopAllServices() {
    return invoke("stop_all");
}
export async function startSingleService(service) {
    return invoke("start_service", {
        service,
    });
}
export async function stopSingleService(service) {
    return invoke("stop_service", {
        service,
    });
}
export async function openSingleService(service, settings) {
    return invoke("open_service", {
        service,
        openClawUrl: settings.openClawUrl,
        openWebUiUrl: settings.openWebUiUrl,
        ollamaUrl: settings.ollamaUrl,
    });
}
