use std::process::Command;

fn run_shell(command: &str) -> bool {
    Command::new("sh")
        .args(["-c", command])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[tauri::command]
pub fn stop_all() -> String {
    // 停止 launchctl 管理的 OpenClaw
    let openclaw = run_shell(
        "launchctl bootout gui/$(id -u)/ai.openclaw.gateway 2>/dev/null \
         || launchctl remove ai.openclaw.gateway 2>/dev/null"
    );

    // 停止 Homebrew 管理的 Ollama
    let ollama = run_shell(
        "brew services stop ollama >/dev/null 2>&1 \
         || pkill -f 'ollama serve'"
    );

    // 正常退出 Cherry Studio，避免“意外退出”提示
    let cherry = run_shell(
        "osascript -e 'tell application \"Cherry Studio\" to quit' 2>/dev/null"
    );

    format!(
        "Stop All completed\n\
OpenClaw: {}\n\
Ollama: {}\n\
Cherry Studio: {}\n\
Docker: kept running",
        if openclaw { "Stopped" } else { "Not running / stop failed" },
        if ollama { "Stopped" } else { "Not running / stop failed" },
        if cherry { "Stopped" } else { "Not running / stop failed" },
    )
}