import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import ConfirmDialog from "../components/ConfirmDialog";

import type {
  LogEntry,
  LogLevel,
  LogSource,
} from "../types/index";

type LogsPageProps = {
  logs: LogEntry[];
  selectedSource:
    | LogSource
    | "All";
  selectedLevel:
    | LogLevel
    | "All";
  searchText: string;
  isLoading: boolean;
  isAutoRefresh: boolean;
  error: string;
  cardStyle: CSSProperties;

  onSourceChange: (
    source:
      | LogSource
      | "All",
  ) => void;

  onLevelChange: (
    level:
      | LogLevel
      | "All",
  ) => void;

  onSearchChange: (
    value: string,
  ) => void;

  onAutoRefreshChange: (
    enabled: boolean,
  ) => void;

  onRefresh: () => void;
  onClear: () => void;
};

const sourceOptions: Array<
  LogSource | "All"
> = [
  "All",
  "AI OS",
  "OpenClaw",
  "Ollama",
  "Docker",
  "Open WebUI",
  "Cherry Studio",
];

const levelOptions: Array<
  LogLevel | "All"
> = [
  "All",
  "debug",
  "info",
  "warning",
  "error",
];

function formatTimestamp(
  value: string,
): string {
  const date =
    new Date(value);

  if (
    Number.isNaN(
      date.getTime(),
    )
  ) {
    return value;
  }

  return date.toLocaleString(
    [],
    {
      hour12: false,
    },
  );
}

function levelIcon(
  level: LogLevel,
): string {
  switch (level) {
    case "error":
      return "⛔";

    case "warning":
      return "⚠️";

    case "debug":
      return "🛠️";

    default:
      return "ℹ️";
  }
}

function LogsPage({
  logs,
  selectedSource,
  selectedLevel,
  searchText,
  isLoading,
  isAutoRefresh,
  error,
  cardStyle,
  onSourceChange,
  onLevelChange,
  onSearchChange,
  onAutoRefreshChange,
  onRefresh,
  onClear,
}: LogsPageProps) {
  const logContainerRef =
    useRef<HTMLDivElement | null>(
      null,
    );

  const [
    autoScroll,
    setAutoScroll,
  ] = useState(true);

  const [
    confirmClear,
    setConfirmClear,
  ] = useState(false);

  useEffect(() => {
    if (
      !autoScroll ||
      !logContainerRef.current
    ) {
      return;
    }

    logContainerRef.current
      .scrollTo({
        top:
          logContainerRef
            .current
            .scrollHeight,

        behavior: "smooth",
      });
  }, [
    autoScroll,
    logs,
  ]);

  const summary =
    useMemo(() => {
      const counts: Record<
        LogLevel,
        number
      > = {
        debug: 0,
        info: 0,
        warning: 0,
        error: 0,
      };

      for (const log of logs) {
        counts[log.level] += 1;
      }

      return counts;
    }, [logs]);

  return (
    <section className="page-section">
      <div className="section-header">
        <div>
          <h2>
            System Logs
          </h2>

          <p>
            Monitor AI OS and local
            service activity.
          </p>
        </div>

        <div className="logs-header-actions">
          <label className="logs-toggle-option">
            <input
              type="checkbox"
              checked={
                isAutoRefresh
              }
              onChange={(
                event,
              ) =>
                onAutoRefreshChange(
                  event.target
                    .checked,
                )
              }
            />

            Auto refresh
          </label>

          <button
            type="button"
            className="secondary-button"
            disabled={isLoading}
            onClick={onRefresh}
          >
            {isLoading
              ? "Refreshing..."
              : "↻ Refresh"}
          </button>
        </div>
      </div>

      <div className="logs-summary-grid">
        <div
          className="logs-summary-card"
          style={cardStyle}
        >
          <span>
            Total
          </span>

          <strong>
            {logs.length}
          </strong>
        </div>

        <div
          className="logs-summary-card logs-summary-info"
          style={cardStyle}
        >
          <span>
            Info
          </span>

          <strong>
            {summary.info}
          </strong>
        </div>

        <div
          className="logs-summary-card logs-summary-warning"
          style={cardStyle}
        >
          <span>
            Warnings
          </span>

          <strong>
            {summary.warning}
          </strong>
        </div>

        <div
          className="logs-summary-card logs-summary-error"
          style={cardStyle}
        >
          <span>
            Errors
          </span>

          <strong>
            {summary.error}
          </strong>
        </div>
      </div>

      <div
        className="logs-panel"
        style={cardStyle}
      >
        <div className="logs-toolbar">
          <label className="logs-filter-field">
            <span>
              Source
            </span>

            <select
              value={
                selectedSource
              }
              onChange={(
                event,
              ) =>
                onSourceChange(
                  event.target
                    .value as
                    | LogSource
                    | "All",
                )
              }
            >
              {sourceOptions.map(
                (source) => (
                  <option
                    key={source}
                    value={source}
                  >
                    {source}
                  </option>
                ),
              )}
            </select>
          </label>

          <label className="logs-filter-field">
            <span>
              Level
            </span>

            <select
              value={
                selectedLevel
              }
              onChange={(
                event,
              ) =>
                onLevelChange(
                  event.target
                    .value as
                    | LogLevel
                    | "All",
                )
              }
            >
              {levelOptions.map(
                (level) => (
                  <option
                    key={level}
                    value={level}
                  >
                    {level ===
                    "All"
                      ? "All levels"
                      : level}
                  </option>
                ),
              )}
            </select>
          </label>

          <label className="logs-search-field">
            <span>
              Search
            </span>

            <input
              type="search"
              value={searchText}
              placeholder="Search logs..."
              onChange={(
                event,
              ) =>
                onSearchChange(
                  event.target
                    .value,
                )
              }
            />
          </label>

          <label className="logs-toggle-option logs-auto-scroll">
            <input
              type="checkbox"
              checked={autoScroll}
              onChange={(
                event,
              ) =>
                setAutoScroll(
                  event.target
                    .checked,
                )
              }
            />

            Auto scroll
          </label>

          <button
            type="button"
            className="danger-button"
            disabled={
              isLoading ||
              logs.length === 0
            }
            onClick={() =>
              setConfirmClear(
                true,
              )
            }
          >
            Clear Logs
          </button>
        </div>

        {error && (
          <div
            className="logs-error"
            role="alert"
          >
            {error}
          </div>
        )}

        <div
          ref={logContainerRef}
          className="logs-console"
        >
          {isLoading &&
          logs.length === 0 ? (
            <div className="logs-empty-state">
              <span>
                ⏳
              </span>

              <p>
                Loading logs...
              </p>
            </div>
          ) : logs.length === 0 ? (
            <div className="logs-empty-state">
              <span>
                📜
              </span>

              <h3>
                No log entries
              </h3>

              <p>
                Logs will appear when
                AI OS or a managed
                service produces
                activity.
              </p>
            </div>
          ) : (
            logs.map(
              (entry) => (
                <article
                  key={entry.id}
                  className={[
                    "log-entry",
                    `log-entry-${entry.level}`,
                  ].join(" ")}
                >
                  <span className="log-level-icon">
                    {levelIcon(
                      entry.level,
                    )}
                  </span>

                  <time className="log-time">
                    {formatTimestamp(
                      entry.timestamp,
                    )}
                  </time>

                  <span className="log-source">
                    {entry.source}
                  </span>

                  <span className="log-level">
                    {entry.level}
                  </span>

                  <pre className="log-message">
                    {entry.message}
                  </pre>
                </article>
              ),
            )
          )}
        </div>
      </div>
      <ConfirmDialog
        open={confirmClear}
        title="Clear all logs?"
        message="This will permanently remove all current log entries. This action cannot be undone."
        confirmLabel="Confirm Clear"
        busy={isLoading}
        onCancel={() =>
          setConfirmClear(false)
        }
        onConfirm={() => {
          onClear();
          setConfirmClear(false);
        }}
      />
    </section>
  );
}

export default LogsPage;