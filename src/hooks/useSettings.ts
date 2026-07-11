import {
  useCallback,
  useEffect,
  useState,
} from "react";

import {
  DEFAULT_SETTINGS,
  STORAGE_KEYS,
} from "../config/constants";

import type {
  Settings,
} from "../types/index";

function loadSettings(): Settings {
  try {
    const stored =
      localStorage.getItem(
        STORAGE_KEYS.settings,
      );

    if (!stored) {
      return DEFAULT_SETTINGS;
    }

    const parsed =
      JSON.parse(
        stored,
      ) as Partial<Settings>;

    return {
      ...DEFAULT_SETTINGS,
      ...parsed,
    };
  } catch {
    return DEFAULT_SETTINGS;
  }
}

function useSettings(
  onMessage: (
    message: string,
  ) => void,
) {
  const [
    settings,
    setSettings,
  ] = useState<Settings>(
    loadSettings,
  );

  useEffect(() => {
    localStorage.setItem(
      STORAGE_KEYS.settings,
      JSON.stringify(settings),
    );

    document.documentElement.dataset.theme =
      settings.theme;
  }, [settings]);

  const updateSetting =
    useCallback(
      <
        K extends keyof Settings,
      >(
        key: K,
        value: Settings[K],
      ) => {
        setSettings(
          (current) => ({
            ...current,
            [key]: value,
          }),
        );
      },
      [],
    );

  const mergeSettings =
    useCallback(
      (
        restored:
          Partial<Settings>,
      ) => {
        setSettings(
          (current) => ({
            ...current,
            ...restored,
          }),
        );

        onMessage(
          "✅ AI OS settings restored.",
        );
      },
      [onMessage],
    );

  const resetSettings =
    useCallback(() => {
      setSettings({
        ...DEFAULT_SETTINGS,
      });

      onMessage(
        "⚙️ Settings restored to defaults.",
      );
    }, [onMessage]);

  return {
    settings,
    updateSetting,
    mergeSettings,
    resetSettings,
  };
}

export default useSettings;