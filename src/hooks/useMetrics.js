import { useCallback, useState, } from "react";
import { invoke } from "@tauri-apps/api/core";
import { EMPTY_METRICS } from "../config/constants";
function useMetrics() {
    const [metrics, setMetrics] = useState(EMPTY_METRICS);
    const refreshMetrics = useCallback(async () => {
        try {
            const result = await invoke("system_metrics");
            const [cpu, memoryUsed, memoryTotal, diskUsed, diskTotal,] = result
                .split("|")
                .map(Number);
            setMetrics({
                cpu: Number.isFinite(cpu)
                    ? cpu
                    : 0,
                memoryUsed: Number.isFinite(memoryUsed)
                    ? memoryUsed
                    : 0,
                memoryTotal: Number.isFinite(memoryTotal)
                    ? memoryTotal
                    : 0,
                diskUsed: Number.isFinite(diskUsed)
                    ? diskUsed
                    : 0,
                diskTotal: Number.isFinite(diskTotal)
                    ? diskTotal
                    : 0,
            });
        }
        catch (error) {
            console.error("Metrics refresh failed:", error);
        }
    }, []);
    return {
        metrics,
        refreshMetrics,
    };
}
export default useMetrics;
