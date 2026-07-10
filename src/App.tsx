import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

function App() {
  const [services, setServices] = useState([
    { name: "OpenClaw", status: "Unknown" },
    { name: "Ollama", status: "Unknown" },
    { name: "Docker", status: "Unknown" },
    { name: "Cherry Studio", status: "Unknown" },
  ]);

  const [message, setMessage] = useState("");

  async function startAll() {
    setMessage("🚀 Starting services...");

    const result = await invoke<string>("start_all");

    setMessage(result);
    await healthCheck();
  }

  async function stopAll() {
    const result = await invoke<string>("stop_all");

    setMessage(result);
    alert(result);

    await healthCheck();
  }

  async function healthCheck() {
    setMessage("🩺 Checking health...");

    const result = await invoke<string>("health_check");

    setMessage(result);

    setServices([
      {
        name: "OpenClaw",
        status: result.includes("OpenClaw: 🟢")
          ? "Running"
          : "Stopped",
      },
      {
        name: "Ollama",
        status: result.includes("Ollama: 🟢")
          ? "Running"
          : "Stopped",
      },
      {
        name: "Docker",
        status: result.includes("Docker: 🟢")
          ? "Running"
          : "Stopped",
      },
      {
        name: "Cherry Studio",
        status: result.includes("Cherry Studio: 🟢")
          ? "Running"
          : "Stopped",
      },
    ]);
  }

  useEffect(() => {
    healthCheck();

    const timer = setInterval(() => {
      healthCheck();
    }, 5000);

    return () => clearInterval(timer);
  }, []);

  const buttonStyle = {
    width: "100%",
    minWidth: 0,
    height: "52px",
    padding: "0 10px",
    border: "none",
    borderRadius: "10px",
    color: "white",
    cursor: "pointer",
    fontSize: "clamp(13px, 1.5vw, 16px)",
    fontWeight: "bold",
    whiteSpace: "nowrap" as const,
    boxShadow: "0 4px 12px rgba(0, 0, 0, 0.25)",
  };

  return (
    <div
      style={{
        display: "flex",
        minHeight: "100vh",
        background: "#0f172a",
        color: "white",
        fontFamily: "Arial, sans-serif",
      }}
    >
      {/* Sidebar */}
      <div
        style={{
          width: "220px",
          flexShrink: 0,
          background: "#111827",
          padding: "24px",
          borderRight: "1px solid #374151",
        }}
      >
        <h2>🤖 AI OS</h2>

        <p>🏠 Dashboard</p>
        <p>🚀 Services</p>
        <p>💾 Backup</p>
        <p>📜 Logs</p>
        <p>⚙️ Settings</p>
      </div>

      {/* Main */}
      <div
        style={{
          flex: 1,
          minWidth: 0,
          padding: "32px",
          overflow: "auto",
        }}
      >
        <h1>Russell AI OS</h1>

        <p style={{ color: "#9ca3af" }}>
          Your Personal AI Workspace
        </p>

        <h2 style={{ marginTop: "40px" }}>
          System Status
        </h2>

        {services.map((item) => (
          <div
            key={item.name}
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              gap: "16px",
              background: "#1f2937",
              padding: "16px",
              borderRadius: "12px",
              marginTop: "12px",
              boxShadow: "0 4px 12px rgba(0, 0, 0, 0.25)",
            }}
          >
            <span>{item.name}</span>

            <span
              style={{
                color:
                  item.status === "Running"
                    ? "#22c55e"
                    : item.status === "Stopped"
                      ? "#ef4444"
                      : "#facc15",
                fontWeight: "bold",
              }}
            >
              {item.status}
            </span>
          </div>
        ))}

        {/* Responsive Buttons */}
        <div
          style={{
            marginTop: "32px",
            display: "grid",
            gridTemplateColumns:
              "repeat(auto-fit, minmax(130px, 1fr))",
            gap: "12px",
            width: "100%",
          }}
        >
          <button
            onClick={startAll}
            style={{
              ...buttonStyle,
              background: "#2563eb",
            }}
          >
            🚀 Start All
          </button>

          <button
            onClick={stopAll}
            style={{
              ...buttonStyle,
              background: "#dc2626",
            }}
          >
            🛑 Stop All
          </button>

          <button
            style={{
              ...buttonStyle,
              background: "#6b7280",
            }}
          >
            💾 Backup
          </button>

          <button
            onClick={healthCheck}
            style={{
              ...buttonStyle,
              background: "#16a34a",
            }}
          >
            🩺 Health Check
          </button>
        </div>

        {message && (
          <p
            style={{
              marginTop: "24px",
              color: "#d1d5db",
              whiteSpace: "pre-line",
            }}
          >
            {message}
          </p>
        )}
      </div>
    </div>
  );
}

export default App;