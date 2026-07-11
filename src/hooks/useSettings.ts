import {
  useCallback,
  useEffect,
  useState,
} from "react";

import {
  DEFAULT_SETTINGS,
} from "../config/constants";

import type {
  Settings,
} from "../types/index";

function loadSettings(): Settings {
  try {
    const stored =
      localStorage.getItem(
        "ai-os-settings",
      );

    if (!stored) {
      return DEFAULT_SETTINGS;
    }

    return {
      ...DEFAULT_SETTINGS,
      ...(JSON.parse(
        stored,
      ) as Partial<Settings>),
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
      "ai-os-settings",
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

  const resetSettings =
    useCallback(() => {
      setSettings(
        DEFAULT_SETTINGS,
      );

      onMessage(
        "⚙️ Settings restored to defaults.",
      );
    }, [onMessage]);

  return {
    settings,
    updateSetting,
    resetSettings,
  };
}

export default useSettings;