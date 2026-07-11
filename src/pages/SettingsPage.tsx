import type {
  CSSProperties,
} from "react";

import {
  APP_INFO,
  LOG_LINE_LIMIT_OPTIONS,
  REFRESH_INTERVAL_OPTIONS,
} from "../config/constants";

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
            Configure service URLs,
            refresh behavior, logs,
            backups and appearance.
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

      <div className="settings-layout">
        <div
          className="settings-card"
          style={cardStyle}
        >
          <div className="settings-group-header">
            <div>
              <h3>
                General
              </h3>

              <p>
                Application behavior
                and appearance.
              </p>
            </div>

            <span>⚙️</span>
          </div>

          <label className="setting-field">
            <span>
              Auto Refresh Interval
            </span>

            <small>
              Controls service,
              metrics, model and log
              refresh frequency.
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
              {REFRESH_INTERVAL_OPTIONS.map(
                (value) => (
                  <option
                    key={value}
                    value={value}
                  >
                    {value} seconds
                  </option>
                ),
              )}
            </select>
          </label>

          <label className="setting-field">
            <span>
              Theme
            </span>

            <small>
              Select the AI OS color
              appearance.
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

          <label className="setting-field">
            <span>
              Log Line Limit
            </span>

            <small>
              Maximum number of entries
              loaded on the Logs page.
            </small>

            <select
              value={
                settings.logLineLimit
              }
              onChange={(event) =>
                onUpdateSetting(
                  "logLineLimit",
                  Number(
                    event.target.value,
                  ),
                )
              }
            >
              {LOG_LINE_LIMIT_OPTIONS.map(
                (value) => (
                  <option
                    key={value}
                    value={value}
                  >
                    {value} lines
                  </option>
                ),
              )}
            </select>
          </label>
        </div>

        <div
          className="settings-card"
          style={cardStyle}
        >
          <div className="settings-group-header">
            <div>
              <h3>
                Service Addresses
              </h3>

              <p>
                URLs used by service
                controls and Open
                buttons.
              </p>
            </div>

            <span>🌐</span>
          </div>

          <label className="setting-field">
            <span>
              OpenClaw URL
            </span>

            <input
              type="url"
              value={
                settings.openClawUrl
              }
              placeholder="http://localhost:18789"
              onChange={(event) =>
                onUpdateSetting(
                  "openClawUrl",
                  event.target.value,
                )
              }
            />
          </label>

          <label className="setting-field">
            <span>
              Ollama URL
            </span>

            <input
              type="url"
              value={
                settings.ollamaUrl
              }
              placeholder="http://localhost:11434"
              onChange={(event) =>
                onUpdateSetting(
                  "ollamaUrl",
                  event.target.value,
                )
              }
            />
          </label>

          <label className="setting-field">
            <span>
              Open WebUI URL
            </span>

            <input
              type="url"
              value={
                settings.openWebUiUrl
              }
              placeholder="http://localhost:3000"
              onChange={(event) =>
                onUpdateSetting(
                  "openWebUiUrl",
                  event.target.value,
                )
              }
            />
          </label>
        </div>

        <div
          className="settings-card"
          style={cardStyle}
        >
          <div className="settings-group-header">
            <div>
              <h3>
                Backup Defaults
              </h3>

              <p>
                Default destination
                and included data.
              </p>
            </div>

            <span>💾</span>
          </div>

          <label className="setting-field">
            <span>
              Backup Directory
            </span>

            <small>
              Use an absolute local
              directory path.
            </small>

            <input
              type="text"
              value={
                settings.backupDirectory
              }
              placeholder="/Users/your-name/Backups"
              onChange={(event) =>
                onUpdateSetting(
                  "backupDirectory",
                  event.target.value,
                )
              }
            />
          </label>

          <label className="settings-checkbox-row">
            <input
              type="checkbox"
              checked={
                settings
                  .includeOpenClawConfig
              }
              onChange={(event) =>
                onUpdateSetting(
                  "includeOpenClawConfig",
                  event.target.checked,
                )
              }
            />

            <span>
              Include OpenClaw
              configuration
            </span>
          </label>

          <label className="settings-checkbox-row">
            <input
              type="checkbox"
              checked={
                settings
                  .includeAiOsSettings
              }
              onChange={(event) =>
                onUpdateSetting(
                  "includeAiOsSettings",
                  event.target.checked,
                )
              }
            />

            <span>
              Include AI OS settings
            </span>
          </label>
        </div>

        <div
          className="settings-card settings-about-card"
          style={cardStyle}
        >
          <div className="settings-group-header">
            <div>
              <h3>
                About
              </h3>

              <p>
                Application identity
                and release details.
              </p>
            </div>

            <span>🤖</span>
          </div>

          <div className="settings-about-grid">
            <div>
              <span>
                Name
              </span>

              <strong>
                {APP_INFO.name}
              </strong>
            </div>

            <div>
              <span>
                Version
              </span>

              <strong>
                {APP_INFO.version}
              </strong>
            </div>

            <div>
              <span>
                Identifier
              </span>

              <strong>
                {APP_INFO.identifier}
              </strong>
            </div>

            <div>
              <span>
                Description
              </span>

              <strong>
                {APP_INFO.description}
              </strong>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}

export default SettingsPage;