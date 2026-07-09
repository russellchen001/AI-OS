import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

const services = [
  { name: "OpenClaw", status: "Running" },
  { name: "Ollama", status: "Running" },
  { name: "Docker", status: "Running" },
  { name: "Cherry Studio", status: "Running" },
];

function App() {
  const [message, setMessage] = useState("");

  async function startAll() {
  setMessage("🚀 Starting services...");

  const result = await invoke<string>("start_all");

  setMessage(result);
}
  return (
    <div
      style={{
        display: "flex",
        height: "100vh",
        background: "#0f172a",
        color: "white",
        fontFamily: "Arial, sans-serif",
      }}
    >
      {/* Sidebar */}
      <div
        style={{
          width: "220px",
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
          padding: "32px",
        }}
      >
        <h1>Russell AI OS</h1>
        <p style={{ color: "#9ca3af" }}>
          Your Personal AI Workspace
        </p>

        <h2 style={{ marginTop: 40 }}>System Status</h2>

        {services.map((item) => (
          <div
            key={item.name}
            style={{
              display: "flex",
              justifyContent: "space-between",
              background: "#1f2937",
              padding: "16px",
              borderRadius: "12px",
              marginTop: "12px",
            }}
          >
            <span>{item.name}</span>

            <span style={{ color: "#22c55e" }}>
              ● {item.status}
            </span>
          </div>
        ))}

        <div style={{ marginTop: "32px" }}>
          <button
            onClick={startAll}
            style={{
              padding: "12px 24px",
              marginRight: "12px",
              background: "#2563eb",
              border: "none",
              borderRadius: "10px",
              color: "white",
              cursor: "pointer",
            }}
          >
            🚀 Start All
          </button>

          <button
            style={{
              padding: "12px 24px",
              background: "#374151",
              border: "none",
              borderRadius: "10px",
              color: "white",
              cursor: "pointer",
            }}
          >
            💾 Backup
          </button>
          {message && (
  <p
    style={{
      marginTop: "20px",
      color: "#22c55e",
      fontWeight: "bold",
    }}
  >
    {message}
  </p>
)}
        </div>
      </div>
    </div>
  );
}

export default App;