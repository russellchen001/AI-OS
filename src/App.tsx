import { useState } from "react";
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
}

async function stopAll() {
  const result = await invoke<string>("stop_all");
  alert(result);
  await healthCheck();
}

  async function healthCheck() {
  setMessage("🩺 Checking health...");

  const result = await invoke<string>("health_check");

  setMessage(result);

  setServices([
  { name: "OpenClaw", status: result.includes("OpenClaw: 🟢") ? "Running" : "Stopped" },
  { name: "Ollama", status: result.includes("Ollama: 🟢") ? "Running" : "Stopped" },
  { name: "Docker", status: result.includes("Docker: 🟢") ? "Running" : "Stopped" },
  {
  name: "Cherry Studio",
  status: result.includes("Cherry Studio: 🟢")
    ? "Running"
    : "Stopped",
},
]);
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
      boxShadow: "0 4px 12px rgba(0, 0, 0, 0.25)", 
    }}
  >
            <span>{item.name}</span>

           <span
  style={{
    color:
      item.status === "Running"
        ? "#22c55e" // 🟢 绿色
        : item.status === "Stopped"
        ? "#ef4444" // 🔴 红色
        : "#facc15", // 🟡 黄色 (Unknown)
  }}
>
  {item.status}
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
    boxShadow: "0 4px 12px rgba(0, 0, 0, 0.25)",
  }}
>
  🚀 Start All
</button>

<button
  onClick={stopAll}
  style={{
    padding: "12px 24px",
    marginRight: "12px",
    background: "#dc2626",
    border: "none",
    borderRadius: "10px",
    color: "white",
    cursor: "pointer",
    boxShadow: "0 4px 12px rgba(0, 0, 0, 0.25)",
  }}
>
  🛑 Stop All
</button>

          <button
            style={{
              padding: "12px 24px",
              background: "#374151",
              border: "none",
              borderRadius: "10px",
              color: "white",
              cursor: "pointer",
              boxShadow: "0 4px 12px rgba(0, 0, 0, 0.25)",
            }}
          >
            💾 Backup
          </button>

          <button
  onClick={healthCheck}
  style={{
    padding: "12px 24px",
    marginLeft: "12px",
    background: "#16a34a",
    border: "none",
    borderRadius: "10px",
    color: "white",
    cursor: "pointer",
    boxShadow: "0 4px 12px rgba(0, 0, 0, 0.25)",
  }}
>
  🩺 Health Check
</button>

          {/* {message && (
  <p>{message}</p>
)} */}
        </div>
      </div>
    </div>
  );
}

export default App;