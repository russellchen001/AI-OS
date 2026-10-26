use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::PathBuf, process::Command};
use tauri::State;

const OPENCLAW_PACKAGE: &str = "openclaw";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentVersionPlan {
    agent_id: String,
    installed_version: Option<String>,
    latest_version: String,
    integrity: String,
    update_available: bool,
    requires_approval: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentUpgradeRequest {
    agent_id: String,
    version: String,
    integrity: String,
    user_approved: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentUpgradeResult {
    success: bool,
    message: String,
    version: Option<String>,
}

fn npm_binary() -> Result<PathBuf, String> {
    let mut candidates = std::env::var_os("PATH")
        .map(|value| {
            std::env::split_paths(&value)
                .map(|path| path.join("npm"))
                .collect()
        })
        .unwrap_or_else(Vec::new);
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/npm"),
        PathBuf::from("/usr/local/bin/npm"),
    ]);
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| "Node.js npm is required to check OpenClaw releases.".to_string())
}

fn package_metadata(spec: &str) -> Result<(String, String), String> {
    let output = Command::new(npm_binary()?)
        .args(["view", spec, "version", "dist.integrity", "--json"])
        .output()
        .map_err(|_| "AI-OS could not query the official npm registry.".to_string())?;
    if !output.status.success() {
        return Err("The official OpenClaw release metadata could not be verified.".to_string());
    }
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| "The official OpenClaw package returned invalid metadata.".to_string())?;
    let version = value
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let integrity = value
        .get("dist.integrity")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if version.is_empty() || !integrity.starts_with("sha512-") {
        return Err("OpenClaw did not publish verifiable integrity metadata.".to_string());
    }
    Ok((version.to_string(), integrity.to_string()))
}

#[tauri::command]
pub(crate) async fn check_agent_version(agent_id: String) -> Result<AgentVersionPlan, String> {
    if agent_id != "openclaw" {
        return Err("This Agent does not have a version adapter yet.".to_string());
    }
    tauri::async_runtime::spawn_blocking(|| {
        let (latest_version, integrity) = package_metadata(&format!("{OPENCLAW_PACKAGE}@latest"))?;
        let installed_version = crate::managed_openclaw::get_managed_openclaw_status()?.version;
        let update_available = installed_version
            .as_ref()
            .is_some_and(|installed| !installed.contains(&latest_version));
        Ok(AgentVersionPlan {
            agent_id: "openclaw".to_string(),
            installed_version,
            latest_version,
            integrity,
            update_available,
            requires_approval: true,
        })
    })
    .await
    .map_err(|_| "AI-OS could not complete the Agent version check.".to_string())?
}

#[tauri::command]
pub(crate) async fn upgrade_agent(
    request: AgentUpgradeRequest,
    runtime: State<'_, crate::runtime::executor::RuntimeExecutionState>,
) -> Result<AgentUpgradeResult, String> {
    if request.agent_id != "openclaw" || !request.user_approved {
        return Ok(failure(
            "OpenClaw upgrade requires an explicitly approved version plan.",
        ));
    }
    if runtime.manager().has_active_runtime("openclaw") {
        return Ok(failure(
            "OpenClaw is busy. Finish or cancel active tasks before upgrading.",
        ));
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        let (version, integrity) =
            package_metadata(&format!("{OPENCLAW_PACKAGE}@{}", request.version))?;
        if version != request.version || integrity != request.integrity {
            return Err(
                "The OpenClaw release changed after review. Check and approve it again."
                    .to_string(),
            );
        }
        crate::managed_openclaw::upgrade_managed_openclaw_blocking(&version, true)
    })
    .await;
    Ok(match result {
        Ok(Ok(version)) => AgentUpgradeResult {
            success: true,
            message: "OpenClaw was upgraded and its Gateway health check passed.".to_string(),
            version: Some(version),
        },
        Ok(Err(message)) => failure(&message),
        Err(_) => failure("AI-OS could not complete the OpenClaw upgrade task."),
    })
}

fn failure(message: &str) -> AgentUpgradeResult {
    AgentUpgradeResult {
        success: false,
        message: message.to_string(),
        version: None,
    }
}

#[cfg(test)]
mod tests {
    use super::failure;

    #[test]
    fn failures_never_claim_a_version() {
        let result = failure("blocked");
        assert!(!result.success);
        assert!(result.version.is_none());
    }
}
