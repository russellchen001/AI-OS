use std::process::Command;

pub fn check_service(name: &str) -> bool {
    match name {
        // OpenClaw
        "openclaw" => {
            Command::new("launchctl")
                .args(["list"])
                .output()
                .map(|o| {
                    String::from_utf8_lossy(&o.stdout)
                        .contains("ai.openclaw.gateway")
                })
                .unwrap_or(false)
        }

        // Docker Desktop
        "docker" => {
            Command::new("osascript")
                .args([
                    "-e",
                    "application id \"com.docker.docker\" is running",
                ])
                .output()
                .map(|o| {
                    String::from_utf8_lossy(&o.stdout)
                        .trim()
                        == "true"
                })
                .unwrap_or(false)
        }

        // Ollama
        "ollama" => {
            Command::new("pgrep")
                .args(["-f", "ollama"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }

        // Cherry Studio
        "Cherry Studio" => {
            Command::new("pgrep")
                .args(["-f", "Cherry Studio"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }

        _ => false,
    }
}

#[tauri::command]
pub fn health_check() -> String {
    let docker = check_service("docker");
    let ollama = check_service("ollama");
    let openclaw = check_service("openclaw");
    let cherry = check_service("Cherry Studio");

    format!(
        "🩺 Health Check\n\n\
🐳 Docker: {}\n\
🦙 Ollama: {}\n\
🤖 OpenClaw: {}\n\
🍒 Cherry Studio: {}",
        if docker { "🟢 Running" } else { "🔴 Stopped" },
        if ollama { "🟢 Running" } else { "🔴 Stopped" },
        if openclaw { "🟢 Running" } else { "🔴 Stopped" },
        if cherry { "🟢 Running" } else { "🔴 Stopped" },
    )
}