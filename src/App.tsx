import {
  useCallback,
  useEffect,
  useState,
  type CSSProperties,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type ServiceStatus = "Running" | "Stopped" | "Unknown";

type Service = {
  name: string;
  icon: string;
  status: ServiceStatus;
};

type ButtonName = "start" | "stop" | "backup" | "health" | null;

function App() {
  const [services, setServices] = useState<Service[]>([
    { name: "OpenClaw", icon: "🤖", status: "Unknown" },
    { name: "Ollama", icon: "🦙", status: "Unknown" },
    { name: "Docker", icon: "🐳", status: "Unknown" },
    { name: "Cherry Studio", icon: "🍒", status: "Unknown" },
  ]);

  const [message, setMessage] = useState("");
  const [lastUpdated, setLastUpdated] = useState("Not checked");
  const [isChecking, setIsChecking] = useState(false);
  const [hoveredButton, setHoveredButton] =
    useState<ButtonName>(null);
  const [hoveredNav, setHoveredNav] = useState<string | null>(
    null,
  );

  const healthCheck = useCallback(async () => {
    try {
      setIsChecking(true);

      const result = await invoke<string>("health_check");

      setMessage(result);

      setServices([
        {
          name: "OpenClaw",
          icon: "🤖",
          status: result.includes("OpenClaw: 🟢")
            ? "Running"
            : "Stopped",
        },
        {
          name: "Ollama",
          icon: "🦙",
          status: result.includes("Ollama: 🟢")
            ? "Running"
            : "Stopped",
        },
        {
          name: "Docker",
          icon: "🐳",
          status: result.includes("Docker: 🟢")
            ? "Running"
            : "Stopped",
        },
        {
          name: "Cherry Studio",
          icon: "🍒",
          status: result.includes("Cherry Studio: 🟢")
            ? "Running"
            : "Stopped",
        },
      ]);

      setLastUpdated(
        new Date().toLocaleTimeString([], {
          hour: "2-digit",
          minute: "2-digit",
          second: "2-digit",
        }),
      );
    } catch (error) {
      setMessage(`Health Check failed: ${String(error)}`);
    } finally {
      setIsChecking(false);
    }
  }, []);

  async function startAll() {
    try {
      setMessage("🚀 Starting services...");

      const result = await invoke<string>("start_all");

      setMessage(result);

      window.setTimeout(() => {
        healthCheck();
      }, 5000);
    } catch (error) {
      setMessage(`Start All failed: ${String(error)}`);
    }
  }

  async function stopAll() {
    try {
      setMessage("🛑 Stopping services...");

      const result = await invoke<string>("stop_all");

      setMessage(result);
      alert(result);

      window.setTimeout(() => {
        healthCheck();
      }, 3000);
    } catch (error) {
      setMessage(`Stop All failed: ${String(error)}`);
    }
  }

  function backup() {
    setMessage("💾 Backup will be implemented in Sprint 3.");
  }

  useEffect(() => {
    healthCheck();

    const timer = window.setInterval(() => {
      healthCheck();
    }, 5000);

    return () => window.clearInterval(timer);
  }, [healthCheck]);

  const runningCount = services.filter(
    (service) => service.status === "Running",
  ).length;

  const stoppedCount = services.filter(
    (service) => service.status === "Stopped",
  ).length;

  const unknownCount = services.filter(
    (service) => service.status === "Unknown",
  ).length;

  const navItems = [
    { name: "Dashboard", icon: "🏠" },
    { name: "Services", icon: "🚀" },
    { name: "Backup", icon: "💾" },
    { name: "Logs", icon: "📜" },
    { name: "Settings", icon: "⚙️" },
  ];

  const buttonBaseStyle: CSSProperties = {
    width: "100%",
    minWidth: 0,
    height: "54px",
    padding: "0 14px",
    border: "1px solid rgba(255, 255, 255, 0.08)",
    borderRadius: "12px",
    color: "white",
    cursor: "pointer",
    fontSize: "clamp(13px, 1.5vw, 16px)",
    fontWeight: 700,
    whiteSpace: "nowrap",
    transition:
      "transform 160ms ease, box-shadow 160ms ease, filter 160ms ease",
  };

  function getButtonStyle(
    name: ButtonName,
    background: string,
  ): CSSProperties {
    const isHovered = hoveredButton === name;

    return {
      ...buttonBaseStyle,
      background,
      transform: isHovered
        ? "translateY(-2px) scale(1.015)"
        : "translateY(0) scale(1)",
      filter: isHovered ? "brightness(1.12)" : "brightness(1)",
      boxShadow: isHovered
        ? "0 10px 24px rgba(0, 0, 0, 0.35)"
        : "0 5px 14px rgba(0, 0, 0, 0.22)",
    };
  }

  function getStatusStyle(
    status: ServiceStatus,
  ): CSSProperties {
    if (status === "Running") {
      return {
        color: "#86efac",
        background: "rgba(34, 197, 94, 0.14)",
        border: "1px solid rgba(34, 197, 94, 0.38)",
      };
    }

    if (status === "Stopped") {
      return {
        color: "#fca5a5",
        background: "rgba(239, 68, 68, 0.14)",
        border: "1px solid rgba(239, 68, 68, 0.38)",
      };
    }

    return {
      color: "#fde047",
      background: "rgba(250, 204, 21, 0.14)",
      border: "1px solid rgba(250, 204, 21, 0.38)",
    };
  }

  return (
    <div
      style={{
        display: "flex",
        minHeight: "100vh",
        background:
          "radial-gradient(circle at top right, #172554 0%, #0f172a 38%, #020617 100%)",
        color: "white",
        fontFamily:
          "Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif",
      }}
    >
      {/* Sidebar */}
      <aside
        style={{
          width: "220px",
          flexShrink: 0,
          padding: "24px 18px",
          background: "rgba(15, 23, 42, 0.92)",
          borderRight: "1px solid rgba(148, 163, 184, 0.14)",
          backdropFilter: "blur(18px)",
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: "10px",
            padding: "4px 8px 24px",
          }}
        >
          <div
            style={{
              width: "40px",
              height: "40px",
              display: "grid",
              placeItems: "center",
              borderRadius: "12px",
              background:
                "linear-gradient(135deg, #2563eb, #7c3aed)",
              boxShadow: "0 8px 22px rgba(37, 99, 235, 0.3)",
              fontSize: "21px",
            }}
          >
            🤖
          </div>

          <div>
            <div style={{ fontSize: "18px", fontWeight: 800 }}>
              AI OS
            </div>
            <div
              style={{
                marginTop: "2px",
                color: "#64748b",
                fontSize: "11px",
                letterSpacing: "0.08em",
                textTransform: "uppercase",
              }}
            >
              Control Center
            </div>
          </div>
        </div>

        <nav
          style={{
            display: "flex",
            flexDirection: "column",
            gap: "8px",
          }}
        >
          {navItems.map((item) => {
            const isActive = item.name === "Dashboard";
            const isHovered = hoveredNav === item.name;

            return (
              <div
                key={item.name}
                onMouseEnter={() => setHoveredNav(item.name)}
                onMouseLeave={() => setHoveredNav(null)}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "11px",
                  padding: "12px 14px",
                  borderRadius: "11px",
                  cursor: "pointer",
                  color: isActive ? "#ffffff" : "#94a3b8",
                  fontWeight: isActive ? 700 : 500,
                  background: isActive
                    ? "linear-gradient(135deg, rgba(37, 99, 235, 0.88), rgba(79, 70, 229, 0.78))"
                    : isHovered
                      ? "rgba(148, 163, 184, 0.1)"
                      : "transparent",
                  transform: isHovered
                    ? "translateX(3px)"
                    : "translateX(0)",
                  boxShadow: isActive
                    ? "0 8px 22px rgba(37, 99, 235, 0.2)"
                    : "none",
                  transition:
                    "background 160ms ease, transform 160ms ease",
                }}
              >
                <span>{item.icon}</span>
                <span>{item.name}</span>
              </div>
            );
          })}
        </nav>

        <div
          style={{
            marginTop: "32px",
            padding: "14px",
            borderRadius: "12px",
            background: "rgba(30, 41, 59, 0.7)",
            border: "1px solid rgba(148, 163, 184, 0.12)",
          }}
        >
          <div
            style={{
              color: "#64748b",
              fontSize: "11px",
              textTransform: "uppercase",
              letterSpacing: "0.08em",
            }}
          >
            Auto Refresh
          </div>

          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: "7px",
              marginTop: "8px",
              color: "#86efac",
              fontSize: "13px",
              fontWeight: 700,
            }}
          >
            <span
              style={{
                width: "8px",
                height: "8px",
                borderRadius: "50%",
                background: "#22c55e",
                boxShadow: "0 0 10px #22c55e",
              }}
            />
            Every 5 seconds
          </div>
        </div>
      </aside>

      {/* Main */}
      <main
        style={{
          flex: 1,
          minWidth: 0,
          padding: "32px",
          overflow: "auto",
        }}
      >
        {/* Header */}
        <header
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "flex-start",
            gap: "24px",
            flexWrap: "wrap",
          }}
        >
          <div>
            <h1
              style={{
                margin: 0,
                fontSize: "clamp(28px, 4vw, 40px)",
                letterSpacing: "-0.03em",
              }}
            >
              Russell AI OS
            </h1>

            <p
              style={{
                margin: "8px 0 0",
                color: "#94a3b8",
                fontSize: "15px",
              }}
            >
              Your Personal AI Workspace
            </p>
          </div>

          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: "10px",
              padding: "10px 14px",
              borderRadius: "12px",
              color: "#cbd5e1",
              background: "rgba(30, 41, 59, 0.72)",
              border: "1px solid rgba(148, 163, 184, 0.14)",
              fontSize: "13px",
            }}
          >
            <span
              style={{
                display: "inline-block",
                width: "9px",
                height: "9px",
                borderRadius: "50%",
                background: isChecking ? "#facc15" : "#22c55e",
                boxShadow: isChecking
                  ? "0 0 10px #facc15"
                  : "0 0 10px #22c55e",
              }}
            />

            {isChecking
              ? "Checking services..."
              : `Updated ${lastUpdated}`}
          </div>
        </header>

        {/* Stats */}
        <section
          style={{
            display: "grid",
            gridTemplateColumns:
              "repeat(auto-fit, minmax(150px, 1fr))",
            gap: "14px",
            marginTop: "30px",
          }}
        >
          <StatCard
            title="Total Services"
            value={services.length}
            icon="🧩"
            accent="#60a5fa"
          />

          <StatCard
            title="Running"
            value={runningCount}
            icon="✅"
            accent="#22c55e"
          />

          <StatCard
            title="Stopped"
            value={stoppedCount}
            icon="⛔"
            accent="#ef4444"
          />

          <StatCard
            title="Unknown"
            value={unknownCount}
            icon="⚠️"
            accent="#facc15"
          />
        </section>

        <section style={{ marginTop: "34px" }}>
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              gap: "16px",
              marginBottom: "14px",
            }}
          >
            <h2 style={{ margin: 0, fontSize: "21px" }}>
              System Status
            </h2>

            <span style={{ color: "#64748b", fontSize: "13px" }}>
              {runningCount}/{services.length} services online
            </span>
          </div>

          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: "12px",
            }}
          >
            {services.map((item) => (
              <div
                key={item.name}
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  gap: "16px",
                  padding: "17px 18px",
                  borderRadius: "14px",
                  background: "rgba(30, 41, 59, 0.76)",
                  border:
                    "1px solid rgba(148, 163, 184, 0.12)",
                  boxShadow: "0 8px 24px rgba(0, 0, 0, 0.2)",
                  backdropFilter: "blur(12px)",
                }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "12px",
                    minWidth: 0,
                  }}
                >
                  <span
                    style={{
                      width: "38px",
                      height: "38px",
                      display: "grid",
                      placeItems: "center",
                      flexShrink: 0,
                      borderRadius: "11px",
                      background: "rgba(15, 23, 42, 0.76)",
                      fontSize: "19px",
                    }}
                  >
                    {item.icon}
                  </span>

                  <span
                    style={{
                      overflow: "hidden",
                      fontSize: "16px",
                      fontWeight: 650,
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {item.name}
                  </span>
                </div>

                <span
                  style={{
                    ...getStatusStyle(item.status),
                    display: "inline-flex",
                    alignItems: "center",
                    gap: "7px",
                    flexShrink: 0,
                    padding: "7px 12px",
                    borderRadius: "999px",
                    fontSize: "13px",
                    fontWeight: 750,
                  }}
                >
                  <span style={{ fontSize: "10px" }}>●</span>
                  {item.status}
                </span>
              </div>
            ))}
          </div>
        </section>

        {/* Buttons */}
        <section
          style={{
            display: "grid",
            gridTemplateColumns:
              "repeat(auto-fit, minmax(135px, 1fr))",
            gap: "12px",
            width: "100%",
            marginTop: "28px",
          }}
        >
          <button
            onClick={startAll}
            onMouseEnter={() => setHoveredButton("start")}
            onMouseLeave={() => setHoveredButton(null)}
            style={getButtonStyle(
              "start",
              "linear-gradient(135deg, #2563eb, #3b82f6)",
            )}
          >
            🚀 Start All
          </button>

          <button
            onClick={stopAll}
            onMouseEnter={() => setHoveredButton("stop")}
            onMouseLeave={() => setHoveredButton(null)}
            style={getButtonStyle(
              "stop",
              "linear-gradient(135deg, #dc2626, #ef4444)",
            )}
          >
            🛑 Stop All
          </button>

          <button
            onClick={backup}
            onMouseEnter={() => setHoveredButton("backup")}
            onMouseLeave={() => setHoveredButton(null)}
            style={getButtonStyle(
              "backup",
              "linear-gradient(135deg, #475569, #64748b)",
            )}
          >
            💾 Backup
          </button>

          <button
            onClick={healthCheck}
            onMouseEnter={() => setHoveredButton("health")}
            onMouseLeave={() => setHoveredButton(null)}
            style={getButtonStyle(
              "health",
              "linear-gradient(135deg, #16a34a, #22c55e)",
            )}
          >
            {isChecking ? "⏳ Checking..." : "🩺 Health Check"}
          </button>
        </section>

        {message && (
          <section
            style={{
              marginTop: "24px",
              padding: "17px 18px",
              borderRadius: "14px",
              color: "#cbd5e1",
              background: "rgba(15, 23, 42, 0.66)",
              border: "1px solid rgba(148, 163, 184, 0.12)",
              fontFamily:
                "ui-monospace, SFMono-Regular, Menlo, monospace",
              fontSize: "13px",
              lineHeight: 1.65,
              whiteSpace: "pre-line",
            }}
          >
            {message}
          </section>
        )}
      </main>
    </div>
  );
}

type StatCardProps = {
  title: string;
  value: number;
  icon: string;
  accent: string;
};

function StatCard({
  title,
  value,
  icon,
  accent,
}: StatCardProps) {
  return (
    <div
      style={{
        position: "relative",
        overflow: "hidden",
        padding: "18px",
        borderRadius: "15px",
        background: "rgba(30, 41, 59, 0.7)",
        border: "1px solid rgba(148, 163, 184, 0.12)",
        boxShadow: "0 8px 24px rgba(0, 0, 0, 0.18)",
        backdropFilter: "blur(12px)",
      }}
    >
      <div
        style={{
          position: "absolute",
          top: "-28px",
          right: "-28px",
          width: "86px",
          height: "86px",
          borderRadius: "50%",
          background: accent,
          filter: "blur(42px)",
          opacity: 0.22,
        }}
      />

      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          gap: "12px",
        }}
      >
        <span style={{ color: "#94a3b8", fontSize: "13px" }}>
          {title}
        </span>

        <span style={{ fontSize: "19px" }}>{icon}</span>
      </div>

      <div
        style={{
          marginTop: "12px",
          color: accent,
          fontSize: "30px",
          fontWeight: 800,
          lineHeight: 1,
        }}
      >
        {value}
      </div>
    </div>
  );
}

export default App;