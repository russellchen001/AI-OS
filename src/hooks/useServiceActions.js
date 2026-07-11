import { useState, } from "react";
import { openSingleService, startAllServices, startSingleService, stopAllServices, stopSingleService, } from "../services/tauri";
export function useServiceActions({ settings, setMessage, addLog, healthCheck, }) {
    const [globalAction, setGlobalAction] = useState(null);
    const [serviceAction, setServiceAction] = useState(null);
    const isBusy = globalAction !== null ||
        serviceAction !== null;
    async function startAll() {
        try {
            setGlobalAction("start");
            setMessage("🚀 Starting all services...");
            addLog("info", "Starting all services");
            const result = await startAllServices();
            setMessage(result);
            addLog("success", "Start All command completed");
            window.setTimeout(async () => {
                await healthCheck(false);
                if (!settings.autoOpenWebUi) {
                    return;
                }
                try {
                    const openResult = await openSingleService("Open WebUI", settings);
                    setMessage(`${result}\n\n${openResult}`);
                    addLog("success", "Open WebUI opened automatically");
                }
                catch (error) {
                    const errorMessage = `Failed to automatically open Open WebUI: ${String(error)}`;
                    setMessage(errorMessage);
                    addLog("error", errorMessage);
                }
            }, 8000);
        }
        catch (error) {
            const errorMessage = `Start All failed: ${String(error)}`;
            setMessage(errorMessage);
            addLog("error", errorMessage);
        }
        finally {
            setGlobalAction(null);
        }
    }
    async function stopAll() {
        try {
            setGlobalAction("stop");
            setMessage("🛑 Stopping all services...");
            addLog("info", "Stopping all services");
            const result = await stopAllServices();
            setMessage(result);
            addLog("success", "Stop All command completed");
            window.setTimeout(() => {
                healthCheck(false);
            }, 5000);
        }
        catch (error) {
            const errorMessage = `Stop All failed: ${String(error)}`;
            setMessage(errorMessage);
            addLog("error", errorMessage);
        }
        finally {
            setGlobalAction(null);
        }
    }
    async function startService(service) {
        try {
            setServiceAction(`start:${service}`);
            setMessage(`🚀 Starting ${service}...`);
            addLog("info", `Starting ${service}`);
            const result = await startSingleService(service);
            setMessage(result);
            addLog("success", `${service} start completed`);
            window.setTimeout(() => healthCheck(false), service === "Docker"
                ? 7000
                : 2000);
        }
        catch (error) {
            const errorMessage = `Failed to start ${service}: ${String(error)}`;
            setMessage(errorMessage);
            addLog("error", errorMessage);
        }
        finally {
            setServiceAction(null);
        }
    }
    async function stopService(service) {
        try {
            setServiceAction(`stop:${service}`);
            setMessage(`🛑 Stopping ${service}...`);
            addLog("info", `Stopping ${service}`);
            const result = await stopSingleService(service);
            setMessage(result);
            addLog("success", `${service} stop completed`);
            window.setTimeout(() => healthCheck(false), service === "Docker"
                ? 5000
                : 1800);
        }
        catch (error) {
            const errorMessage = `Failed to stop ${service}: ${String(error)}`;
            setMessage(errorMessage);
            addLog("error", errorMessage);
        }
        finally {
            setServiceAction(null);
        }
    }
    async function openService(service) {
        try {
            setServiceAction(`open:${service}`);
            addLog("info", `Opening ${service}`);
            const result = await openSingleService(service, settings);
            setMessage(result);
            addLog("success", `${service} opened`);
        }
        catch (error) {
            const errorMessage = `Failed to open ${service}: ${String(error)}`;
            setMessage(errorMessage);
            addLog("error", errorMessage);
        }
        finally {
            setServiceAction(null);
        }
    }
    return {
        globalAction,
        serviceAction,
        isBusy,
        startAll,
        stopAll,
        startService,
        stopService,
        openService,
    };
}
