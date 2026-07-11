import type {
  LogEntry,
  LogFilter,
} from "../types";

type LogsPageProps = {
  logs: LogEntry[];
  filter: LogFilter;
  onFilterChange: (
    filter: LogFilter,
  ) => void;
  onClear: () => void;
};

function LogsPage({
  logs,
  filter,
  onFilterChange,
  onClear,
}: LogsPageProps) {
  const filters: LogFilter[] = [
    "all",
    "info",
    "success",
    "warning",
    "error",
  ];

  return (
    <section className="page-card">
      <div className="page-title page-title-row">
        <div>
          <h2>Operation Logs</h2>
          <p>Review recent service events.</p>
        </div>

        <button
          className="clear-button"
          onClick={onClear}
        >
          🧹 Clear Logs
        </button>
      </div>

      <div className="log-filters">
        {filters.map((item) => (
          <button
            key={item}
            className={`filter-button ${
              filter === item
                ? "filter-button-active"
                : ""
            }`}
            onClick={() =>
              onFilterChange(item)
            }
          >
            {item}
          </button>
        ))}
      </div>

      <div className="log-list">
        {logs.length === 0 ? (
          <div className="empty-state">
            <h3>No Logs</h3>
            <p>
              Operations will appear here.
            </p>
          </div>
        ) : (
          logs.map((log) => (
            <div
              key={log.id}
              className={`log-entry log-${log.level}`}
            >
              <span>{log.timestamp}</span>

              <strong>{log.level}</strong>

              <span>{log.message}</span>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

export default LogsPage;