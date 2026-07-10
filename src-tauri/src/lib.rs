mod health;
mod stop;

use std::process::Command;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn start_all() -> String {
    let _ = Command::new("open")
        .args(["-a", "Docker"])
        .spawn();

    let _ = Command::new("sh")
        .args([
            "-c",
            "brew services start ollama >/dev/null 2>&1 || \
             ollama serve >/dev/null 2>&1 &",
        ])
        .spawn();

    let _ = Command::new("sh")
        .args([
            "-c",
            "launchctl bootstrap gui/$(id -u) \
             \"$HOME/Library/LaunchAgents/ai.openclaw.gateway.plist\" \
             2>/dev/null || true; \
             launchctl kickstart -k \
             gui/$(id -u)/ai.openclaw.gateway",
        ])
        .spawn();

    let _ = Command::new("open")
        .args(["-a", "Cherry Studio"])
        .spawn();

    "🚀 Start All command sent!".to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            start_all,
            health::health_check,
            stop::stop_all,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}