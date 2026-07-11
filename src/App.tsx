import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type CSSProperties,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type ServiceStatus =
  | "Running"
  | "Stopped"
  | "Unknown";

type Service = {
  name: string;
  icon: string;
  description: string;
  status: ServiceStatus;
};

type PageName =
  | "Dashboard"
  | "Services"
  | "Settings";

type ThemeMode = "dark" | "light";

type Settings = {
  refreshInterval: number;
  openClawUrl: string;
  ollamaUrl: string;
  openWebUiUrl: string;
  theme: ThemeMode;
};

type Metrics = {
  cpu: number;
  memoryUsed: number;
  memoryTotal: number;
  diskUsed: number;
  diskTotal: number;
};

const INITIAL_SERVICES: Service[] = [
  {
    name: "OpenClaw",
    icon: "🤖",
    description: "Local AI gateway",
    status: "Unknown",
  },
  {
    name: "Ollama",
    icon: "🦙",
    description: "Local model runtime",
    status: "Unknown",
  },
  {
    name: "Docker",
    icon: "🐳",
    description: "Container runtime",
    status: "Unknown",
  },
  {
    name: "Open WebUI",
    icon: "🌐",
    description: "Browser AI workspace",
    status: "Unknown",
  },
  {
    name: "Cherry Studio",
    icon: "🍒",
    description: "Desktop AI client",
    status: "Unknown",
  },
];

const DEFAULT_SETTINGS: Settings = {
  refreshInterval: 5,
  openClawUrl: "http://localhost:18789",
  ollamaUrl: "http://localhost:11434",
  openWebUiUrl: "http://localhost:3000",
  theme: "dark",
};

const EMPTY_METRICS: Metrics = {
  cpu: 0,
  memoryUsed: 0,
  memoryTotal: 0,
  diskUsed: 0,
  diskTotal: 0,
};

function loadSettings(): Settings {
  try {
    const stored =
      localStorage.getItem("ai-os-settings");

    if (!stored) {
      return DEFAULT_SETTINGS;
    }

    return {
      ...DEFAULT_SETTINGS,
      ...(JSON.parse(stored) as Partial<Settings>),
    };
  } catch {
    return DEFAULT_SETTINGS;
  }
}

function App() {
  const [activePage, setActivePage] =
    useState<PageName>("Dashboard");

  const [services, setServices] =
    useState<Service[]>(INITIAL_SERVICES);

  const [settings, setSettings] =
    useState<Settings>(loadSettings);

  const [metrics, setMetrics] =
    useState<Metrics>(EMPTY_METRICS);

  const [message, setMessage] =
    useState("");

  const [lastUpdated, setLastUpdated] =
    useState("Not checked");

  const [isChecking, setIsChecking] =
    useState(false);

  const [globalAction, setGlobalAction] =
    useState<"start" | "stop" | null>(null);

  const [serviceAction, setServiceAction] =
    useState<string | null>(null);

  const [openAction, setOpenAction] =
    useState<string | null>(null);

  const isBusy =
    globalAction !== null ||
    serviceAction !== null;

  const healthCheck = useCallback(
    async (showMessage = true) => {
      try {
        setIsChecking(true);

        const result =
          await invoke<string>("health_check");

        if (showMessage) {
          setMessage(result);
        }

        setServices((current) =>
          current.map((service) => ({
            ...service,
            status: result.includes(
              `${service.name}: 🟢`,
            )
              ? "Running"
              : "Stopped",
          })),
        );

        setLastUpdated(
          new Date().toLocaleTimeString([], {
            hour: "2-digit",
            minute: "2-digit",
            second: "2-digit",
          }),
        );
      } catch (error) {
        setMessage(
          `Health Check failed: ${String(error)}`,
        );
      } finally {
        setIsChecking(false);
      }
    },
    [],
  );

  const refreshMetrics =
    useCallback(async () => {
      try {
        const result =
          await invoke<string>(
            "system_metrics",
          );

        const [
          cpu,
          memoryUsed,
          memoryTotal,
          diskUsed,
          diskTotal,
        ] = result.split("|").map(Number);

        setMetrics({
          cpu:
            Number.isFinite(cpu)
              ? cpu
              : 0,
          memoryUsed:
            Number.isFinite(memoryUsed)
              ? memoryUsed
              : 0,
          memoryTotal:
            Number.isFinite(memoryTotal)
              ? memoryTotal
              : 0,
          diskUsed:
            Number.isFinite(diskUsed)
              ? diskUsed
              : 0,
          diskTotal:
            Number.isFinite(diskTotal)
              ? diskTotal
              : 0,
        });
      } catch (error) {
        console.error(
          "Metrics refresh failed:",
          error,
        );
      }
    }, []);

  async function startAll() {
    try {
      setGlobalAction("start");
      setMessage("🚀 Starting services...");

      const result =
        await invoke<string>("start_all");

      setMessage(result);

      window.setTimeout(() => {
        healthCheck(false);
      }, 5000);

      window.setTimeout(() => {
        healthCheck(false);
      }, 20000);

      window.setTimeout(() => {
        healthCheck(false);
      }, 30000);
    } catch (error) {
      setMessage(
        `Start All failed: ${String(error)}`,
      );
    } finally {
      setGlobalAction(null);
    }
  }

  async function stopAll() {
    try {
      setGlobalAction("stop");
      setMessage("🛑 Stopping services...");

      const result =
        await invoke<string>("stop_all");

      setMessage(result);

      window.setTimeout(() => {
        healthCheck(false);
      }, 8000);
    } catch (error) {
      setMessage(
        `Stop All failed: ${String(error)}`,
      );
    } finally {
      setGlobalAction(null);
    }
  }

  async function startService(
    service: string,
  ) {
    try {
      setServiceAction(`start:${service}`);

      setMessage(
        `🚀 Starting ${service}...`,
      );

      const result =
        await invoke<string>(
          "start_service",
          { service },
        );

      setMessage(result);

      const delay =
        service === "Docker"
          ? 20000
          : service === "Open WebUI"
            ? 8000
            : 3000;

      window.setTimeout(() => {
        healthCheck(false);
      }, delay);
    } catch (error) {
      setMessage(
        `Failed to start ${service}: ${String(
          error,
        )}`,
      );
    } finally {
      setServiceAction(null);
    }
  }

  async function stopService(
    service: string,
  ) {
    try {
      setServiceAction(`stop:${service}`);

      setMessage(
        `🛑 Stopping ${service}...`,
      );

      const result =
        await invoke<string>(
          "stop_service",
          { service },
        );

      setMessage(result);

      const delay =
        service === "Docker"
          ? 8000
          : service === "Open WebUI"
            ? 4000
            : 2500;

      window.setTimeout(() => {
        healthCheck(false);
      }, delay);
    } catch (error) {
      setMessage(
        `Failed to stop ${service}: ${String(
          error,
        )}`,
      );
    } finally {
      setServiceAction(null);
    }
  }

  async function openService(
    service: string,
  ) {
    try {
      setOpenAction(service);

      const result =
        await invoke<string>(
          "open_service",
          {
            service,
            openclawUrl:
              settings.openClawUrl,
            ollamaUrl:
              settings.ollamaUrl,
            openWebUiUrl:
              settings.openWebUiUrl,
          },
        );

      setMessage(result);
    } catch (error) {
      setMessage(
        `Failed to open ${service}: ${String(
          error,
        )}`,
      );
    } finally {
      setOpenAction(null);
    }
  }

  useEffect(() => {
    localStorage.setItem(
      "ai-os-settings",
      JSON.stringify(settings),
    );

    document.documentElement.dataset.theme =
      settings.theme;
  }, [settings]);

  useEffect(() => {
    healthCheck(false);
    refreshMetrics();

    const interval =
      window.setInterval(() => {
        healthCheck(false);
        refreshMetrics();
      }, Math.max(
        settings.refreshInterval,
        2,
      ) * 1000);

    return () =>
      window.clearInterval(interval);
  }, [
    healthCheck,
    refreshMetrics,
    settings.refreshInterval,
  ]);

  const runningCount = useMemo(
    () =>
      services.filter(
        (service) =>
          service.status === "Running",
      ).length,
    [services],
  );

  const stoppedCount = useMemo(
    () =>
      services.filter(
        (service) =>
          service.status === "Stopped",
      ).length,
    [services],
  );

  const unknownCount = useMemo(
    () =>
      services.filter(
        (service) =>
          service.status === "Unknown",
      ).length,
    [services],
  );

  const allRunning =
    services.length > 0 &&
    runningCount === services.length;

  function handleGlobalToggle() {
    if (isBusy) {
      return;
    }

    if (allRunning) {
      stopAll();
    } else {
      startAll();
    }
  }

  function updateSetting<K extends keyof Settings>(
    key: K,
    value: Settings[K],
  ) {
    setSettings((current) => ({
      ...current,
      [key]: value,
    }));
  }

  function resetSettings() {
    setSettings(DEFAULT_SETTINGS);
    setMessage(
      "⚙️ Settings restored to defaults.",
    );
  }

  const navItems: Array<{
    name: PageName;
    icon: string;
  }> = [
    {
      name: "Dashboard",
      icon: "🏠",
    },
    {
      name: "Services",
      icon: "🚀",
    },
    {
      name: "Settings",
      icon: "⚙️",
    },
  ];

  const appStyle: CSSProperties = {
    display: "flex",
    minHeight: "100vh",
    background:
      settings.theme === "dark"
        ? "radial-gradient(circle at top right,#172554 0%,#0f172a 38%,#020617 100%)"
        : "linear-gradient(135deg,#f8fafc,#e2e8f0)",
    color:
      settings.theme === "dark"
        ? "#ffffff"
        : "#0f172a",
    fontFamily:
      "Inter,-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif",
  };

  const cardStyle: CSSProperties = {
    borderRadius: "16px",
    background:
      settings.theme === "dark"
        ? "rgba(30,41,59,.76)"
        : "rgba(255,255,255,.82)",
    border:
      settings.theme === "dark"
        ? "1px solid rgba(148,163,184,.12)"
        : "1px solid rgba(148,163,184,.24)",
    boxShadow:
      "0 10px 30px rgba(15,23,42,.14)",
    backdropFilter: "blur(14px)",
  };

  function renderServiceRows() {
    return (
      <div className="service-list">
        {services.map((service) => {
          const running =
            service.status === "Running";

          const starting =
            serviceAction ===
            `start:${service.name}`;

          const stopping =
            serviceAction ===
            `stop:${service.name}`;

          const loading =
            starting || stopping;

          return (
            <div
              key={service.name}
              className="service-row"
              style={cardStyle}
            >
              <div className="service-info">
                <span className="service-icon">
                  {service.icon}
                </span>

                <div>
                  <div className="service-name">
                    {service.name}
                  </div>

                  <div className="service-description">
                    {service.description}
                  </div>
                </div>
              </div>

              <div className="service-actions">
                <span
                  className={[
                    "status-badge",
                    running
                      ? "status-running"
                      : service.status ===
                          "Stopped"
                        ? "status-stopped"
                        : "status-unknown",
                  ].join(" ")}
                >
                  ● {service.status}
                </span>

                <ServiceToggle
                  checked={running}
                  disabled={isBusy}
                  loading={loading}
                  label={
                    starting
                      ? "Starting..."
                      : stopping
                        ? "Stopping..."
                        : running
                          ? "Running"
                          : "Stopped"
                  }
                  onChange={() => {
                    if (running) {
                      stopService(
                        service.name,
                      );
                    } else {
                      startService(
                        service.name,
                      );
                    }
                  }}
                />

                <button
                  type="button"
                  className="open-button"
                  disabled={
                    openAction ===
                    service.name
                  }
                  onClick={() =>
                    openService(
                      service.name,
                    )
                  }
                >
                  {openAction ===
                  service.name
                    ? "Opening..."
                    : "↗ Open"}
                </button>
              </div>
            </div>
          );
        })}
      </div>
    );
  }
    return (
    <div style={appStyle}>
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-icon">
            🤖
          </div>

          <div>
            <div className="brand-title">
              AI OS
            </div>

            <div className="brand-subtitle">
              Control Center
            </div>
          </div>
        </div>

        <nav className="nav-list">
          {navItems.map((item) => (
            <button
              key={item.name}
              type="button"
              className={[
                "nav-item",
                activePage === item.name
                  ? "nav-item-active"
                  : "",
              ]
                .filter(Boolean)
                .join(" ")}
              onClick={() =>
                setActivePage(item.name)
              }
            >
              <span>{item.icon}</span>
              <span>{item.name}</span>
            </button>
          ))}
        </nav>

        <div className="refresh-card">
          <div className="refresh-label">
            Auto Refresh
          </div>

          <div className="refresh-value">
            <span className="online-dot" />
            Every{" "}
            {settings.refreshInterval}{" "}
            seconds
          </div>
        </div>
      </aside>

      <main className="main-content">
        <header className="top-header">
          <div>
            <h1>Russell AI OS</h1>

            <p>
              Your Personal AI Workspace
            </p>
          </div>

          <div className="updated-badge">
            <span
              className={[
                "updated-dot",
                isChecking
                  ? "updated-dot-checking"
                  : "",
              ]
                .filter(Boolean)
                .join(" ")}
            />

            {isChecking
              ? "Checking services..."
              : `Updated ${lastUpdated}`}
          </div>
        </header>

        {activePage === "Dashboard" && (
          <>
            <section className="stats-grid">
              <StatCard
                title="Total Services"
                value={services.length}
                icon="🧩"
                accent="#60a5fa"
                cardStyle={cardStyle}
              />

              <StatCard
                title="Running"
                value={runningCount}
                icon="✅"
                accent="#22c55e"
                cardStyle={cardStyle}
              />

              <StatCard
                title="Stopped"
                value={stoppedCount}
                icon="⛔"
                accent="#ef4444"
                cardStyle={cardStyle}
              />

              <StatCard
                title="Unknown"
                value={unknownCount}
                icon="⚠️"
                accent="#facc15"
                cardStyle={cardStyle}
              />
            </section>

            <section className="section-block">
              <div className="section-header">
                <div>
                  <h2>
                    System Performance
                  </h2>

                  <p>
                    Live macOS resource usage
                  </p>
                </div>

                <button
                  type="button"
                  className="secondary-button"
                  onClick={refreshMetrics}
                >
                  ↻ Refresh
                </button>
              </div>

              <div className="metrics-grid">
                <MetricCard
                  title="CPU Usage"
                  icon="🧠"
                  value={`${metrics.cpu.toFixed(
                    1,
                  )}%`}
                  progress={metrics.cpu}
                  accent="#3b82f6"
                  cardStyle={cardStyle}
                />

                <MetricCard
                  title="Memory"
                  icon="💾"
                  value={`${metrics.memoryUsed.toFixed(
                    1,
                  )} / ${metrics.memoryTotal.toFixed(
                    1,
                  )} GB`}
                  progress={
                    metrics.memoryTotal > 0
                      ? (metrics.memoryUsed /
                          metrics.memoryTotal) *
                        100
                      : 0
                  }
                  accent="#8b5cf6"
                  cardStyle={cardStyle}
                />

                <MetricCard
                  title="Disk"
                  icon="🗄️"
                  value={`${metrics.diskUsed.toFixed(
                    1,
                  )} / ${metrics.diskTotal.toFixed(
                    1,
                  )} GB`}
                  progress={
                    metrics.diskTotal > 0
                      ? (metrics.diskUsed /
                          metrics.diskTotal) *
                        100
                      : 0
                  }
                  accent="#f59e0b"
                  cardStyle={cardStyle}
                />
              </div>
            </section>

            <section className="section-block">
              <div className="section-header">
                <div>
                  <h2>System Status</h2>

                  <p>
                    Control your local AI
                    services
                  </p>
                </div>

                <span className="online-count">
                  {runningCount}/
                  {services.length} services
                  online
                </span>
              </div>

              {renderServiceRows()}
            </section>

            <section className="bottom-actions">
              <ServiceToggle
                checked={allRunning}
                disabled={isBusy}
                loading={
                  globalAction !== null
                }
                large
                label={
                  globalAction === "start"
                    ? "Starting All..."
                    : globalAction ===
                        "stop"
                      ? "Stopping All..."
                      : allRunning
                        ? "All Services Running"
                        : "Start All Services"
                }
                onChange={
                  handleGlobalToggle
                }
              />

              <button
                type="button"
                className="action-button backup-button"
                onClick={() =>
                  setMessage(
                    "💾 Backup will be implemented in the next step.",
                  )
                }
              >
                💾 Backup
              </button>

              <button
                type="button"
                className="action-button health-button"
                disabled={
                  isBusy || isChecking
                }
                onClick={() =>
                  healthCheck(true)
                }
              >
                {isChecking
                  ? "⏳ Checking..."
                  : "🩺 Health Check"}
              </button>
            </section>
          </>
        )}

        {activePage === "Services" && (
          <section className="page-section">
            <div className="section-header">
              <div>
                <h2>Services</h2>

                <p>
                  Start, stop and open each
                  local service
                </p>
              </div>

              <ServiceToggle
                checked={allRunning}
                disabled={isBusy}
                loading={
                  globalAction !== null
                }
                large
                label={
                  globalAction === "start"
                    ? "Starting All..."
                    : globalAction ===
                        "stop"
                      ? "Stopping All..."
                      : allRunning
                        ? "Stop All Services"
                        : "Start All Services"
                }
                onChange={
                  handleGlobalToggle
                }
              />
            </div>

            {renderServiceRows()}
          </section>
        )}

        {activePage === "Settings" && (
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
                onClick={resetSettings}
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
                    updateSetting(
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
                    updateSetting(
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
                    updateSetting(
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
                    updateSetting(
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
                    updateSetting(
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
        )}

        {message && (
          <section className="message-panel">
            {message}
          </section>
        )}
      </main>
    </div>
  );
}

type ServiceToggleProps = {
  checked: boolean;
  disabled?: boolean;
  loading?: boolean;
  label: string;
  large?: boolean;
  onChange: () => void;
};

function ServiceToggle({
  checked,
  disabled = false,
  loading = false,
  label,
  large = false,
  onChange,
}: ServiceToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      className={[
        "service-toggle",
        checked
          ? "service-toggle-on"
          : "service-toggle-off",
        large
          ? "service-toggle-large"
          : "",
        loading
          ? "service-toggle-loading"
          : "",
      ]
        .filter(Boolean)
        .join(" ")}
      onClick={onChange}
    >
      <span className="service-toggle-track">
        <span className="service-toggle-thumb">
          {loading && (
            <span className="toggle-spinner" />
          )}
        </span>
      </span>

      <span className="service-toggle-label">
        {label}
      </span>
    </button>
  );
}

type StatCardProps = {
  title: string;
  value: number;
  icon: string;
  accent: string;
  cardStyle: CSSProperties;
};

function StatCard({
  title,
  value,
  icon,
  accent,
  cardStyle,
}: StatCardProps) {
  return (
    <div
      className="stat-card"
      style={cardStyle}
    >
      <div
        className="stat-card-glow"
        style={{
          background: accent,
        }}
      />

      <div className="stat-card-header">
        <span>{title}</span>
        <span>{icon}</span>
      </div>

      <div
        className="stat-card-value"
        style={{ color: accent }}
      >
        {value}
      </div>
    </div>
  );
}

type MetricCardProps = {
  title: string;
  icon: string;
  value: string;
  progress: number;
  accent: string;
  cardStyle: CSSProperties;
};

function MetricCard({
  title,
  icon,
  value,
  progress,
  accent,
  cardStyle,
}: MetricCardProps) {
  const safeProgress = Math.min(
    Math.max(progress, 0),
    100,
  );

  return (
    <div
      className="metric-card"
      style={cardStyle}
    >
      <div className="metric-header">
        <div>
          <span className="metric-title">
            {title}
          </span>

          <div className="metric-value">
            {value}
          </div>
        </div>

        <span className="metric-icon">
          {icon}
        </span>
      </div>

      <div className="metric-track">
        <div
          className="metric-progress"
          style={{
            width: `${safeProgress}%`,
            background: accent,
            boxShadow: `0 0 14px ${accent}55`,
          }}
        />
      </div>

      <div className="metric-percent">
        {safeProgress.toFixed(1)}%
      </div>
    </div>
  );
}

export default App;