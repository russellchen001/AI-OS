mod stop;
mod health;

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

    let _ = Command::new("open")
        .args(["-a", "Ollama"])
        .spawn();

    "🚀 Docker and Ollama started!".to_string()
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