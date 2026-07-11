import type {
  CSSProperties,
} from "react";

import type {
  Settings,
  ThemeMode,
} from "../types/index";

type SettingsPageProps = {
  settings: Settings;
  cardStyle: CSSProperties;
  onUpdateSetting: <
    K extends keyof Settings,
  >(
    key: K,
    value: Settings[K],
  ) => void;
  onReset: () => void;
};

function SettingsPage({
  settings,
  cardStyle,
  onUpdateSetting,
  onReset,
}: SettingsPageProps) {
  return (
    <section className="page-section">
      <div className="section-header">
        <div>
          <h2>Settings</h2>

          <p>
            Configure refresh,
            addresses and appearance
          </p>
        </div>

        <button
          type="button"
          className="secondary-button"
          onClick={onReset}
        >
          Reset Defaults
        </button>
      </div>

      <div
        className="settings-card"
        style={cardStyle}
      >
        <label className="setting-field">
          <span>
            Auto Refresh Interval
          </span>

          <small>
            Minimum refresh interval
            is 2 seconds.
          </small>

          <select
            value={
              settings.refreshInterval
            }
            onChange={(event) =>
              onUpdateSetting(
                "refreshInterval",
                Number(
                  event.target.value,
                ),
              )
            }
          >
            <option value={2}>
              2 seconds
            </option>

            <option value={5}>
              5 seconds
            </option>

            <option value={10}>
              10 seconds
            </option>

            <option value={30}>
              30 seconds
            </option>

            <option value={60}>
              60 seconds
            </option>
          </select>
        </label>

        <label className="setting-field">
          <span>OpenClaw URL</span>

          <small>
            Used by the Open button.
          </small>

          <input
            type="url"
            value={
              settings.openClawUrl
            }
            onChange={(event) =>
              onUpdateSetting(
                "openClawUrl",
                event.target.value,
              )
            }
            placeholder="http://localhost:18789"
          />
        </label>

        <label className="setting-field">
          <span>Ollama URL</span>

          <small>
            Used by the Open button.
          </small>

          <input
            type="url"
            value={settings.ollamaUrl}
            onChange={(event) =>
              onUpdateSetting(
                "ollamaUrl",
                event.target.value,
              )
            }
            placeholder="http://localhost:11434"
          />
        </label>

        <label className="setting-field">
          <span>Open WebUI URL</span>

          <small>
            Used by the Open button.
          </small>

          <input
            type="url"
            value={
              settings.openWebUiUrl
            }
            onChange={(event) =>
              onUpdateSetting(
                "openWebUiUrl",
                event.target.value,
              )
            }
            placeholder="http://localhost:3000"
          />
        </label>

        <label className="setting-field">
          <span>Theme</span>

          <small>
            Switch between dark and
            light appearance.
          </small>

          <select
            value={settings.theme}
            onChange={(event) =>
              onUpdateSetting(
                "theme",
                event.target
                  .value as ThemeMode,
              )
            }
          >
            <option value="dark">
              Dark
            </option>

            <option value="light">
              Light
            </option>
          </select>
        </label>
      </div>
    </section>
  );
}

export default SettingsPage;