import { useCallback, useEffect, useState, } from "react";
import { DEFAULT_SETTINGS, STORAGE_KEYS, } from "../config/constants";
function loadSettings() {
    try {
        const stored = localStorage.getItem(STORAGE_KEYS.settings);
        if (!stored) {
            return DEFAULT_SETTINGS;
        }
        const parsed = JSON.parse(stored);
        return {
            ...DEFAULT_SETTINGS,
            ...parsed,
        };
    }
    catch {
        return DEFAULT_SETTINGS;
    }
}
function useSettings(onMessage) {
    const [settings, setSettings,] = useState(loadSettings);
    useEffect(() => {
        localStorage.setItem(STORAGE_KEYS.settings, JSON.stringify(settings));
        document.documentElement.dataset.theme =
            settings.theme;
    }, [settings]);
    const updateSetting = useCallback((key, value) => {
        setSettings((current) => ({
            ...current,
            [key]: value,
        }));
    }, []);
    const mergeSettings = useCallback((restored) => {
        setSettings((current) => ({
            ...current,
            ...restored,
        }));
        onMessage("✅ AI OS settings restored.");
    }, [onMessage]);
    const resetSettings = useCallback(() => {
        setSettings({
            ...DEFAULT_SETTINGS,
        });
        onMessage("⚙️ Settings restored to defaults.");
    }, [onMessage]);
    return {
        settings,
        updateSetting,
        mergeSettings,
        resetSettings,
    };
}
export default useSettings;
