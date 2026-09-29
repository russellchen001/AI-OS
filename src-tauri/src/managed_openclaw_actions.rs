use crate::{
    managed_openclaw::{self, discover_cli, MANAGED_OPENCLAW_URL},
    openclaw::{self, OpenClawServerInput},
};
use serde::{Deserialize, Serialize};

const ADOPTED_OPENCLAW_NAME: &str = "Existing OpenClaw";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptExistingOpenClawInput {
    pub gateway_token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedOpenClawActionResult {
    pub success: bool,
    pub state: String,
    pub message: String,
    pub version: Option<String>,
}

fn success(message: &str, version: Option<String>) -> ManagedOpenClawActionResult {
    ManagedOpenClawActionResult {
        success: true,
        state: "ready".to_string(),
        message: message.to_string(),
        version,
    }
}

fn failure(state: &str, message: String) -> ManagedOpenClawActionResult {
    ManagedOpenClawActionResult {
        success: false,
        state: state.to_string(),
        message,
        version: None,
    }
}

fn adopt_policy(state: &str) -> &'static str {
    match state {
        "ready" => "noop",
        "existing-unmanaged" => "adopt",
        _ => "reject",
    }
}

fn adopt_existing_blocking(input: AdoptExistingOpenClawInput) -> Result<Option<String>, String> {
    let before = managed_openclaw::get_managed_openclaw_status()?;

    match adopt_policy(&before.state) {
        "noop" => return Ok(before.version),
        "reject" => {
            return Err(format!(
                "OpenClaw cannot be adopted from state '{}'.",
                before.state
            ))
        }
        _ => {}
    }

    if let Ok(status) = openclaw::get_active_openclaw_status() {
        if status.connected {
            return Ok(status.version.or(before.version));
        }
    }

    let token = input.gateway_token.trim();
    if token.is_empty() {
        return Err(
            "AI-OS needs the existing Gateway token to adopt this OpenClaw installation. The token will be stored only through the existing user-managed server compatibility path."
                .to_string(),
        );
    }

    let server = OpenClawServerInput {
        name: ADOPTED_OPENCLAW_NAME.to_string(),
        server_url: MANAGED_OPENCLAW_URL.to_string(),
        gateway_token: token.to_string(),
        enabled: true,
        auto_connect: true,
    };

    let probe = openclaw::test_openclaw_connection_input(server.clone());
    if !probe.success {
        return Err(
            "The existing OpenClaw Gateway could not be verified with the provided credential. No OpenClaw configuration was changed."
                .to_string(),
        );
    }

    let created = openclaw::save_openclaw_server(server);
    if !created.success {
        return Err(created.message);
    }

    let server_id = created
        .server
        .map(|server| server.id)
        .ok_or_else(|| "AI-OS created no adoptable server record.".to_string())?;

    let activation = openclaw::set_active_openclaw_server(server_id.clone());
    if !activation.success {
        return Err(activation.message);
    }

    let verified = openclaw::test_openclaw_connection(server_id);
    if !verified.success {
        return Err(
            "The adopted OpenClaw server record could not be verified. The native OpenClaw configuration was not changed."
                .to_string(),
        );
    }

    Ok(verified.version.or(probe.version).or(before.version))
}

#[tauri::command]
pub async fn adopt_existing_openclaw(
    input: AdoptExistingOpenClawInput,
) -> ManagedOpenClawActionResult {
    match tauri::async_runtime::spawn_blocking(move || adopt_existing_blocking(input)).await {
        Ok(Ok(version)) => success(
            "Existing OpenClaw was verified and adopted without changing its native configuration or Gateway service.",
            version,
        ),
        Ok(Err(message)) => failure("existing-unmanaged", message),
        Err(_) => failure(
            "existing-unmanaged",
            "AI-OS could not complete the OpenClaw adoption task.".to_string(),
        ),
    }
}

fn finish_policy(state: &str, managed: bool) -> &'static str {
    match (state, managed) {
        ("ready", _) => "noop",
        ("configured-no-gateway", true) => "finish",
        _ => "reject",
    }
}

fn finish_blocking() -> Result<String, String> {
    let before = managed_openclaw::get_managed_openclaw_status()?;

    match finish_policy(&before.state, before.managed) {
        "noop" => {
            return before
                .version
                .ok_or_else(|| "OpenClaw version is unavailable.".to_string())
        }
        "reject" => {
            return Err(
                "Finish Setup is only available for an AI-OS managed local configuration whose Gateway service is missing."
                    .to_string(),
            )
        }
        _ => {}
    }

    let (cli, version) = discover_cli().ok_or_else(|| "OpenClaw CLI was not found.".to_string())?;
    managed_openclaw::finish_secure_managed_gateway(&cli)?;

    if managed_openclaw::get_managed_openclaw_status()?.state != "ready" {
        return Err("OpenClaw Gateway setup finished but verification failed.".to_string());
    }

    Ok(version)
}

#[tauri::command]
pub async fn finish_managed_openclaw_setup() -> ManagedOpenClawActionResult {
    match tauri::async_runtime::spawn_blocking(finish_blocking).await {
        Ok(Ok(version)) => success(
            "The managed OpenClaw Gateway service is installed, running and verified.",
            Some(version),
        ),
        Ok(Err(message)) => failure("configured-no-gateway", message),
        Err(_) => failure(
            "configured-no-gateway",
            "AI-OS could not complete the managed Gateway service setup.".to_string(),
        ),
    }
}

fn repair_policy(state: &str) -> &'static str {
    if state == "broken" {
        "diagnose-only"
    } else {
        "reject"
    }
}

fn repair_blocking() -> Result<ManagedOpenClawActionResult, String> {
    let status = managed_openclaw::get_managed_openclaw_status()?;

    if repair_policy(&status.state) == "reject" {
        return Ok(failure(
            &status.state,
            "OpenClaw does not currently require the bounded Repair flow.".to_string(),
        ));
    }

    Ok(failure(
        "broken",
        status.diagnostic.unwrap_or_else(|| {
            "AI-OS found a malformed OpenClaw state. Automatic reinstall, token rotation, service replacement and configuration overwrite are intentionally disabled."
                .to_string()
        }),
    ))
}

#[tauri::command]
pub async fn repair_managed_openclaw() -> ManagedOpenClawActionResult {
    match tauri::async_runtime::spawn_blocking(repair_blocking).await {
        Ok(Ok(result)) => result,
        Ok(Err(message)) => failure("broken", message),
        Err(_) => failure(
            "broken",
            "AI-OS could not inspect the OpenClaw repair state.".to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{adopt_policy, finish_policy, repair_policy};

    #[test]
    fn adopt_policy_preserves_existing_openclaw() {
        assert_eq!(adopt_policy("existing-unmanaged"), "adopt");
        assert_eq!(adopt_policy("ready"), "noop");
        assert_eq!(adopt_policy("not-installed"), "reject");
    }

    #[test]
    fn finish_policy_only_allows_managed_gap() {
        assert_eq!(finish_policy("configured-no-gateway", true), "finish");
        assert_eq!(finish_policy("configured-no-gateway", false), "reject");
        assert_eq!(finish_policy("existing-unmanaged", false), "reject");
    }

    #[test]
    fn repair_policy_never_auto_overwrites() {
        assert_eq!(repair_policy("broken"), "diagnose-only");
        assert_eq!(repair_policy("gateway-stopped"), "reject");
        assert_eq!(repair_policy("existing-unmanaged"), "reject");
    }
}
