import { useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";

import {
  APP_INFO,
  LOG_LINE_LIMIT_OPTIONS,
  REFRESH_INTERVAL_OPTIONS,
} from "../config/constants";

import type {
  PageName,
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
  onOpenSection: (page: PageName) => void;
};

function SettingsPage({
  settings,
  cardStyle,
  onUpdateSetting,
  onReset,
  onOpenSection,
}: SettingsPageProps) {
  const [webSearchTrusted, setWebSearchTrusted] =
    useState<boolean | null>(null);
  const [webSearchSaving, setWebSearchSaving] = useState(false);
  const [webSearchError, setWebSearchError] = useState("");

  useEffect(() => {
    let active = true;

    void invoke<boolean>("get_web_search_trust")
      .then((enabled) => {
        if (active) {
          setWebSearchTrusted(enabled);
          setWebSearchError("");
        }
      })
      .catch(() => {
        if (active) {
          setWebSearchTrusted(null);
          setWebSearchError(
            "Unable to load Web Search permission."
          );
        }
      });

    return () => {
      active = false;
    };
  }, []);

  async function updateWebSearchTrust(enabled: boolean) {
    if (webSearchSaving) return;

    setWebSearchSaving(true);
    setWebSearchError("");

    try {
      const saved = await invoke<boolean>(
        "set_web_search_trust",
        { enabled },
      );

      // Read back the persisted value from Runtime.
      const verified = await invoke<boolean>(
        "get_web_search_trust",
      );

      if (saved !== enabled || verified !== enabled) {
        throw new Error("Permission verification failed");
      }

      setWebSearchTrusted(verified);
    } catch {
      setWebSearchError(
        "Unable to save permission. Please try again."
      );

      // Do not assume a write failed without checking Runtime.
      try {
        const actual = await invoke<boolean>(
          "get_web_search_trust",
        );
        setWebSearchTrusted(actual);
      } catch {
        setWebSearchTrusted(null);
      }
    } finally {
      setWebSearchSaving(false);
    }
  }

  return (
    <section className="page-section">
      <div className="page-heading settings-page-heading">
        <div>
          <p className="settings-kicker">Workspace controls</p>
          <h1>Settings</h1>

          <p>
            Manage your AI, Agents,
            tools, privacy and advanced
            system controls.
          </p>
        </div>

        <button
          type="button"
          className="secondary-button"
          onClick={onReset}
        >
          Reset defaults
        </button>
      </div>

      <div className="settings-management-grid">
        {[
          ["My AI", "Providers, accounts and models", "My AI"],
          ["Agents", "OpenClaw, Hermes and custom agents", "Agents"],
          ["Advanced · Skills", "MCP servers, permissions and tool connections", "MCP"],
          ["Local models", "Local models powered by oMLX", "Models"],
        ].map(([title, description, page]) => (
          <button
            key={title}
            type="button"
            className="settings-management-card"
            onClick={() => onOpenSection(page as PageName)}
          >
            <span>{title}</span>
            <small>{description}</small>
            <strong>→</strong>
          </button>
        ))}
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
        </div>
        <div className="settings-card" style={cardStyle}>
          <div className="settings-group-header">
            <div>
              <h3>Web Search Permissions</h3>
              <p>
                Control automatic access to public web search.
              </p>
            </div>
            <span aria-hidden="true">🔎</span>
          </div>

          <label className="setting-field">
            <span>Always allow public web search</span>
            <small>
              When enabled, AI-OS can run browser.search
              without asking every time. This does not
              authorize other browser actions, file changes,
              messages, payments, or computer control.
            </small>

            <select
              aria-label="Public web search permission"
              value={
                webSearchTrusted === null
                  ? "unknown"
                  : webSearchTrusted
                    ? "allowed"
                    : "ask"
              }
              disabled={
                webSearchSaving || webSearchTrusted === null
              }
              onChange={(event) => {
                void updateWebSearchTrust(
                  event.target.value === "allowed",
                );
              }}
            >
              {webSearchTrusted === null && (
                <option value="unknown">
                  Permission status unavailable
                </option>
              )}
              <option value="ask">Ask before searching</option>
              <option value="allowed">
                Always allow public web search
              </option>
            </select>
          </label>

          {webSearchSaving && (
            <p role="status">Saving permission…</p>
          )}

          {webSearchError && (
            <p role="alert">{webSearchError}</p>
          )}
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
