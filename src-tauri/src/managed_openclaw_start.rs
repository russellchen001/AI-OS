use crate::managed_openclaw::{self, command_text, discover_cli};
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedOpenClawStartResult {
    pub success: bool,
    pub state: String,
    pub message: String,
    pub version: Option<String>,
}

fn start_policy(state: &str) -> &'static str {
    match state {
        "ready" => "noop",
        "gateway-stopped" => "start",
        _ => "reject",
    }
}

fn start_blocking() -> Result<Option<String>, String> {
    let before = managed_openclaw::get_managed_openclaw_status()?;
    match start_policy(&before.state) {
        "noop" => return Ok(before.version),
        "reject" => {
            return Err(format!(
                "OpenClaw cannot be started from state '{}'.",
                before.state
            ))
        }
        _ => {}
    }
    let (cli, version) = discover_cli().ok_or_else(|| "OpenClaw CLI was not found.".to_string())?;
    command_text(&cli, &["gateway", "start", "--json"])?;
    for _ in 0..20 {
        if managed_openclaw::get_managed_openclaw_status()?.state == "ready" {
            return Ok(Some(version.clone()));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Err("OpenClaw Gateway did not become ready after start.".to_string())
}

#[tauri::command]
pub async fn start_managed_openclaw() -> ManagedOpenClawStartResult {
    match tauri::async_runtime::spawn_blocking(start_blocking).await {
        Ok(Ok(version)) => ManagedOpenClawStartResult {
            success: true,
            state: "ready".into(),
            message: "OpenClaw Gateway is ready.".into(),
            version,
        },
        Ok(Err(message)) => ManagedOpenClawStartResult {
            success: false,
            state: "failed".into(),
            message,
            version: None,
        },
        Err(_) => ManagedOpenClawStartResult {
            success: false,
            state: "failed".into(),
            message: "AI-OS could not complete the OpenClaw start task.".into(),
            version: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::start_policy;
    #[test]
    fn start_policy_is_bounded() {
        assert_eq!(start_policy("ready"), "noop");
        assert_eq!(start_policy("gateway-stopped"), "start");
        assert_eq!(start_policy("broken"), "reject");
    }
}
