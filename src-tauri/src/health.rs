use std::process::Command;

pub(crate) fn command_success(command: &str) -> bool {
    Command::new("/bin/sh")
        .args(["-c", command])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(crate) fn curl_success(url: &str) -> bool {
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

#[derive(Debug, Clone, Copy)]
pub(crate) struct ServiceHealthSnapshot {
    pub openclaw_running: bool,
    pub ollama_running: bool,
    pub docker_running: bool,
    pub open_webui_running: bool,
    pub cherry_studio_running: bool,
}

pub(crate) fn probe_ollama(base_url: &str) -> bool {
    let tags_url = format!("{}/api/tags", base_url.trim_end_matches('/'));

    curl_success(&tags_url) || command_success(r#"pgrep -f "ollama serve" >/dev/null 2>&1"#)
}

pub(crate) fn probe_docker() -> bool {
    command_success(
        r#"
"/Applications/Docker.app/Contents/Resources/bin/docker" info \
>/dev/null 2>&1
"#,
    )
}

pub(crate) fn open_webui_container_exists() -> bool {
    command_success(
        r#"
"/Applications/Docker.app/Contents/Resources/bin/docker" ps -a \
--format '{{.Names}}|{{.Image}}' 2>/dev/null |
grep -Ei 'open[-_]?webui|openwebui|ghcr.io/open-webui/open-webui' >/dev/null
"#,
    )
}

pub(crate) fn probe_open_webui(url: &str) -> bool {
    curl_success(url)
        || command_success(
            r#"
"/Applications/Docker.app/Contents/Resources/bin/docker" ps \
--format '{{.Names}}' 2>/dev/null |
grep -Ei 'open[-_]?webui' >/dev/null
"#,
        )
}

pub(crate) fn probe_cherry_studio() -> bool {
    command_success(
        r#"
pgrep -x "Cherry Studio" >/dev/null 2>&1 ||
pgrep -f "/Cherry Studio.app/" >/dev/null 2>&1
"#,
    )
}

pub(crate) fn probe_services(
    openclaw_url: &str,
    ollama_url: &str,
    open_web_ui_url: &str,
) -> ServiceHealthSnapshot {
    let openclaw_running = curl_success(openclaw_url)
        || command_success(
            r#"
launchctl print gui/$(id -u)/ai.openclaw.gateway >/dev/null 2>&1 ||
pgrep -f "openclaw" >/dev/null 2>&1
"#,
        );

    ServiceHealthSnapshot {
        openclaw_running,
        ollama_running: probe_ollama(ollama_url),
        docker_running: probe_docker(),
        open_webui_running: probe_open_webui(open_web_ui_url),
        cherry_studio_running: probe_cherry_studio(),
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

    let snapshot = probe_services(&openclaw_url, &ollama_base, &open_web_ui_url);

    format!(
        "OpenClaw: {}\n\
Ollama: {}\n\
Docker: {}\n\
Open WebUI: {}\n\
Cherry Studio: {}",
        status_icon(snapshot.openclaw_running),
        status_icon(snapshot.ollama_running),
        status_icon(snapshot.docker_running),
        status_icon(snapshot.open_webui_running),
        status_icon(snapshot.cherry_studio_running),
    )
}
