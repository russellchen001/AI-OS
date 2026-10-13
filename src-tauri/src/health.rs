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
    pub cherry_studio_running: bool,
}

pub(crate) fn probe_cherry_studio() -> bool {
    command_success(
        r#"
pgrep -x "Cherry Studio" >/dev/null 2>&1 ||
pgrep -f "/Cherry Studio.app/" >/dev/null 2>&1
"#,
    )
}

pub(crate) fn probe_services(openclaw_url: &str) -> ServiceHealthSnapshot {
    let openclaw_running = curl_success(openclaw_url)
        || command_success(
            r#"launchctl print gui/$(id -u)/ai.openclaw.gateway >/dev/null 2>&1 ||
pgrep -f "openclaw" >/dev/null 2>&1"#,
        );

    ServiceHealthSnapshot {
        openclaw_running,
        cherry_studio_running: probe_cherry_studio(),
    }
}

#[tauri::command]
pub fn health_check(openclaw_url: Option<String>) -> String {
    let openclaw_url = openclaw_url
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "http://localhost:18789".to_string());
    let snapshot = probe_services(&openclaw_url);
    format!(
        "OpenClaw: {}\nCherry Studio: {}",
        status_icon(snapshot.openclaw_running),
        status_icon(snapshot.cherry_studio_running),
    )
}
