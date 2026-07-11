import { useCallback, useEffect, useState, } from "react";
import { STORAGE_KEYS, } from "../config/constants";
import { createBackup, deleteBackup, listBackups, restoreBackup, revealBackup, } from "../services/backup";
function readCachedHistory() {
    try {
        const value = localStorage.getItem(STORAGE_KEYS.backupHistory);
        if (!value) {
            return [];
        }
        const parsed = JSON.parse(value);
        return Array.isArray(parsed)
            ? parsed
            : [];
    }
    catch {
        return [];
    }
}
function useBackup({ settings, onMessage, onSettingsRestored, }) {
    const [status, setStatus,] = useState("idle");
    const [backups, setBackups,] = useState(readCachedHistory);
    const [selectedBackup, setSelectedBackup,] = useState(null);
    const [error, setError,] = useState("");
    const saveHistory = useCallback((records) => {
        setBackups(records);
        localStorage.setItem(STORAGE_KEYS.backupHistory, JSON.stringify(records));
    }, []);
    const refreshBackups = useCallback(async () => {
        if (!settings.backupDirectory
            .trim()) {
            setBackups([]);
            return;
        }
        try {
            setError("");
            const records = await listBackups(settings.backupDirectory);
            saveHistory(records);
        }
        catch (nextError) {
            const message = `Unable to load backups: ${String(nextError)}`;
            setError(message);
        }
    }, [
        saveHistory,
        settings.backupDirectory,
    ]);
    useEffect(() => {
        refreshBackups();
    }, [refreshBackups]);
    const runBackup = useCallback(async () => {
        const destination = settings.backupDirectory.trim();
        if (!destination) {
            const message = "Please enter a backup directory first.";
            setError(message);
            onMessage(`💾 ${message}`);
            return;
        }
        if (!settings
            .includeOpenClawConfig &&
            !settings
                .includeAiOsSettings) {
            const message = "Select at least one item to back up.";
            setError(message);
            onMessage(`💾 ${message}`);
            return;
        }
        try {
            setStatus("creating");
            setError("");
            const result = await createBackup({
                destinationDirectory: destination,
                includeOpenClawConfig: settings
                    .includeOpenClawConfig,
                includeAiOsSettings: settings
                    .includeAiOsSettings,
                settingsJson: JSON.stringify(settings, null, 2),
            });
            if (!result.success) {
                throw new Error(result.message);
            }
            setStatus("success");
            onMessage(`✅ ${result.message}`);
            await refreshBackups();
        }
        catch (nextError) {
            const message = `Backup failed: ${String(nextError)}`;
            setStatus("error");
            setError(message);
            onMessage(`❌ ${message}`);
        }
    }, [
        onMessage,
        refreshBackups,
        settings,
    ]);
    const runRestore = useCallback(async (request) => {
        try {
            setSelectedBackup(request.archivePath);
            setStatus("restoring");
            setError("");
            const result = await restoreBackup(request);
            if (!result.success) {
                throw new Error(result.message);
            }
            if (result
                .restoredSettingsJson &&
                onSettingsRestored) {
                try {
                    const restored = JSON.parse(result
                        .restoredSettingsJson);
                    onSettingsRestored(restored);
                }
                catch (parseError) {
                    console.error("Unable to parse restored settings:", parseError);
                }
            }
            setStatus("success");
            onMessage(`✅ ${result.message}`);
        }
        catch (nextError) {
            const message = `Restore failed: ${String(nextError)}`;
            setStatus("error");
            setError(message);
            onMessage(`❌ ${message}`);
        }
        finally {
            setSelectedBackup(null);
        }
    }, [
        onMessage,
        onSettingsRestored,
    ]);
    const openBackupLocation = useCallback(async (archivePath) => {
        try {
            const result = await revealBackup(archivePath);
            onMessage(result);
        }
        catch (nextError) {
            onMessage(`Unable to reveal backup: ${String(nextError)}`);
        }
    }, [onMessage]);
    const removeBackup = useCallback(async (archivePath) => {
        try {
            setSelectedBackup(archivePath);
            const result = await deleteBackup(archivePath);
            onMessage(result);
            await refreshBackups();
        }
        catch (nextError) {
            onMessage(`Unable to delete backup: ${String(nextError)}`);
        }
        finally {
            setSelectedBackup(null);
        }
    }, [
        onMessage,
        refreshBackups,
    ]);
    return {
        status,
        backups,
        selectedBackup,
        error,
        isBusy: status === "creating" ||
            status === "restoring",
        runBackup,
        runRestore,
        refreshBackups,
        openBackupLocation,
        removeBackup,
    };
}
export default useBackup;
