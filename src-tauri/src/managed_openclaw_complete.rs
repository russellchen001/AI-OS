use crate::managed_openclaw::{self, discover_cli};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedOpenClawCompleteResult {
    pub success: bool,
    pub state: String,
    pub message: String,
    pub version: Option<String>,
}

fn complete_policy(state: &str) -> &'static str {
    match state {
        "ready" => "noop",
        "installed-not-configured" => "complete",
        _ => "reject",
    }
}

fn complete_blocking() -> Result<String, String> {
    let before = managed_openclaw::get_managed_openclaw_status()?;

    match complete_policy(&before.state) {
        "noop" => {
            return before
                .version
                .ok_or_else(|| "OpenClaw version is unavailable.".to_string())
        }
        "reject" => {
            return Err(format!(
                "OpenClaw cannot complete setup from state '{}'.",
                before.state
            ))
        }
        _ => {}
    }

    let (cli, version) = discover_cli().ok_or_else(|| "OpenClaw CLI was not found.".to_string())?;

    managed_openclaw::complete_secure_managed_gateway(&cli)?;

    if managed_openclaw::get_managed_openclaw_status()?.state != "ready" {
        return Err(
            "OpenClaw setup completed, but the managed Gateway could not be verified.".to_string(),
        );
    }

    Ok(version)
}

#[tauri::command]
pub async fn complete_managed_openclaw_setup() -> ManagedOpenClawCompleteResult {
    match tauri::async_runtime::spawn_blocking(complete_blocking).await {
        Ok(Ok(version)) => ManagedOpenClawCompleteResult {
            success: true,
            state: "ready".into(),
            message: "OpenClaw setup is complete and verified.".into(),
            version: Some(version),
        },
        Ok(Err(message)) => ManagedOpenClawCompleteResult {
            success: false,
            state: "failed".into(),
            message,
            version: None,
        },
        Err(_) => ManagedOpenClawCompleteResult {
            success: false,
            state: "failed".into(),
            message: "AI-OS could not complete OpenClaw setup.".into(),
            version: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::complete_policy;
    use crate::managed_openclaw::gateway_install_args;

    #[test]
    fn complete_policy_is_bounded() {
        assert_eq!(complete_policy("installed-not-configured"), "complete");
        assert_eq!(complete_policy("ready"), "noop");
        assert_eq!(complete_policy("existing-unmanaged"), "reject");
        assert!(!gateway_install_args().contains(&"--token"));
    }
}
