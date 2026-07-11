use std::process::Command;

fn command_success(command: &str) -> bool {
    Command::new("/bin/sh")
        .args(["-c", command])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn status_icon(running: bool) -> &'static str {
    if running {
        "🟢"
    } else {
        "🔴"
    }
}

#[tauri::command]
pub fn health_check() -> String {
    let openclaw_running = command_success(
        r#"
curl -fsS http://localhost:18789 >/dev/null 2>&1 ||
launchctl print gui/$(id -u)/ai.openclaw.gateway >/dev/null 2>&1 ||
pgrep -f "openclaw" >/dev/null 2>&1
"#,
    );

    let ollama_running = command_success(
        r#"
curl -fsS http://localhost:11434/api/tags >/dev/null 2>&1 ||
pgrep -f "ollama serve" >/dev/null 2>&1
"#,
    );

    let docker_running = command_success(
        r#"
"/Applications/Docker.app/Contents/Resources/bin/docker" info \
>/dev/null 2>&1
"#,
    );

    let open_webui_running = command_success(
        r#"
curl -fsS http://localhost:3000 >/dev/null 2>&1 ||
curl -fsS http://localhost:8080 >/dev/null 2>&1 ||
"/Applications/Docker.app/Contents/Resources/bin/docker" ps \
--format '{{.Names}}' 2>/dev/null |
grep -Ei 'open[-_]?webui' >/dev/null
"#,
    );

    let cherry_running = command_success(
        r#"
pgrep -x "Cherry Studio" >/dev/null 2>&1 ||
pgrep -f "/Cherry Studio.app/" >/dev/null 2>&1
"#,
    );

    format!(
        "OpenClaw: {}\n\
Ollama: {}\n\
Docker: {}\n\
Open WebUI: {}\n\
Cherry Studio: {}",
        status_icon(openclaw_running),
        status_icon(ollama_running),
        status_icon(docker_running),
        status_icon(open_webui_running),
        status_icon(cherry_running),
    )
}