use std::process::Command;

fn command_success(command: &str) -> bool {
    Command::new("/bin/sh")
        .args(["-c", command])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn curl_success(url: &str) -> bool {
    Command::new("curl")
        .args(["-fsS", "--max-time", "3", url])
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
pub fn health_check(
    openclaw_url: Option<String>,
    ollama_url: Option<String>,
    open_web_ui_url: Option<String>,
) -> String {
    let openclaw_url = openclaw_url
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "http://localhost:18789".to_string());

    let ollama_base = ollama_url
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "http://localhost:11434".to_string());

    let open_web_ui_url = open_web_ui_url
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "http://localhost:3000".to_string());

    let ollama_tags_url = format!("{}/api/tags", ollama_base.trim_end_matches('/'),);

    let openclaw_running = curl_success(&openclaw_url)
        || command_success(
            r#"
launchctl print gui/$(id -u)/ai.openclaw.gateway >/dev/null 2>&1 ||
pgrep -f "openclaw" >/dev/null 2>&1
"#,
        );

    let ollama_running = curl_success(&ollama_tags_url)
        || command_success(r#"pgrep -f "ollama serve" >/dev/null 2>&1"#);

    let docker_running = command_success(
        r#"
"/Applications/Docker.app/Contents/Resources/bin/docker" info \
>/dev/null 2>&1
"#,
    );

    let open_webui_running = curl_success(&open_web_ui_url)
        || command_success(
            r#"
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
