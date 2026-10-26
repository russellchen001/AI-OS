use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{ffi::OsString, fs, path::PathBuf, process::Command};

const GROK_PACKAGE: &str = "@xai-official/grok";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CliInstallPlan {
    provider_id: String,
    package_name: String,
    version: String,
    integrity: String,
    installed_version: Option<String>,
    action: String,
    requires_approval: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CliInstallRequest {
    provider_id: String,
    version: String,
    integrity: String,
    user_approved: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CliInstallResult {
    success: bool,
    message: String,
    version: Option<String>,
    login_command: Option<String>,
}

fn npm_candidates(explicit_path: Option<OsString>) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = explicit_path
        .map(|value| {
            std::env::split_paths(&value)
                .map(|entry| entry.join("npm"))
                .collect()
        })
        .unwrap_or_default();
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/npm"),
        PathBuf::from("/usr/local/bin/npm"),
    ]);
    candidates
}

fn npm_binary() -> Result<PathBuf, String> {
    npm_candidates(std::env::var_os("PATH"))
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            "Node.js npm is required to install the official Grok CLI package.".to_string()
        })
}

fn package_metadata(spec: &str) -> Result<(String, String), String> {
    let output = Command::new(npm_binary()?)
        .args(["view", spec, "version", "dist.integrity", "--json"])
        .output()
        .map_err(|_| "AI-OS could not query the official npm registry.".to_string())?;
    if !output.status.success() {
        return Err("The official Grok package metadata could not be verified.".to_string());
    }
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| "The official Grok package returned invalid metadata.".to_string())?;
    let version = value
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let integrity = value
        .get("dist.integrity")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if version.is_empty() || !integrity.starts_with("sha512-") {
        return Err(
            "The official Grok package did not publish verifiable integrity metadata.".to_string(),
        );
    }
    Ok((version, integrity))
}

fn restore_previous_binary(canonical_binary: &PathBuf, previous_target: &Option<PathBuf>) -> bool {
    let Some(target) = previous_target else {
        return false;
    };
    let _ = fs::remove_file(canonical_binary);
    #[cfg(unix)]
    return std::os::unix::fs::symlink(target, canonical_binary).is_ok();
    #[cfg(not(unix))]
    false
}

#[tauri::command]
pub(crate) async fn prepare_cli_provider_install(
    provider_id: String,
) -> Result<CliInstallPlan, String> {
    if provider_id != "grok" {
        return Err(
            "This CLI Provider does not have an AI-OS managed installation adapter.".to_string(),
        );
    }
    tauri::async_runtime::spawn_blocking(|| {
        let (version, integrity) = package_metadata(&format!("{GROK_PACKAGE}@latest"))?;
        let installed_version = crate::grok_cli::installed_version();
        Ok(CliInstallPlan {
            provider_id: "grok".to_string(),
            package_name: GROK_PACKAGE.to_string(),
            action: if installed_version.is_some() {
                "upgrade"
            } else {
                "install"
            }
            .to_string(),
            version,
            integrity,
            installed_version,
            requires_approval: true,
        })
    })
    .await
    .map_err(|_| "AI-OS could not prepare the CLI installation plan.".to_string())?
}

#[tauri::command]
pub(crate) async fn install_cli_provider(request: CliInstallRequest) -> CliInstallResult {
    let result =
        tauri::async_runtime::spawn_blocking(move || install_cli_provider_blocking(request)).await;
    match result {
        Ok(Ok(version)) => CliInstallResult { success: true, message: "Official Grok CLI installed and health-checked. Sign in to confirm available models.".to_string(), version: Some(version), login_command: Some("grok login".to_string()) },
        Ok(Err(message)) => CliInstallResult { success: false, message, version: None, login_command: None },
        Err(_) => CliInstallResult { success: false, message: "AI-OS could not complete the CLI installation task.".to_string(), version: None, login_command: None },
    }
}

fn install_cli_provider_blocking(request: CliInstallRequest) -> Result<String, String> {
    if request.provider_id != "grok" || !request.user_approved {
        return Err("Grok CLI installation requires an explicit approved plan.".to_string());
    }
    if crate::grok_cli::has_active_requests() {
        return Err(
            "Grok is busy. Finish or cancel the active request before installing an update."
                .to_string(),
        );
    }
    let (version, integrity) = package_metadata(&format!("{GROK_PACKAGE}@{}", request.version))?;
    if version != request.version || integrity != request.integrity {
        return Err(
            "The Grok package changed after review. Prepare and approve a new installation plan."
                .to_string(),
        );
    }
    let prefix = dirs::home_dir()
        .ok_or_else(|| "AI-OS could not determine the home directory.".to_string())?
        .join(".ai-os/providers/grok");
    let canonical_binary = dirs::home_dir()
        .ok_or_else(|| "AI-OS could not determine the home directory.".to_string())?
        .join(".grok/bin/grok");
    let previous_target = fs::read_link(&canonical_binary).ok();
    crate::logs::append_ai_os_diagnostic("cli-provider id=grok event=install-started");
    let status = Command::new(npm_binary()?)
        .args([
            "install",
            "--prefix",
            prefix.to_string_lossy().as_ref(),
            &format!("{GROK_PACKAGE}@{version}"),
            "--no-audit",
            "--no-fund",
        ])
        .status()
        .map_err(|_| "AI-OS could not start npm for the approved Grok installation.".to_string())?;
    if !status.success() {
        let restored = restore_previous_binary(&canonical_binary, &previous_target);
        crate::logs::append_ai_os_diagnostic(if restored {
            "cli-provider id=grok event=install-failed rollback=restored"
        } else {
            "cli-provider id=grok event=install-failed rollback=unavailable"
        });
        return Err(if restored {
            "The approved Grok npm installation failed. AI-OS restored the previous version."
        } else {
            "The approved Grok npm installation failed and no previous version could be restored."
        }
        .to_string());
    }
    let installed = match crate::grok_cli::installed_version() {
        Some(installed) => installed,
        None => {
            if restore_previous_binary(&canonical_binary, &previous_target) {
                crate::logs::append_ai_os_diagnostic(
                    "cli-provider id=grok event=health-failed rollback=restored",
                );
                return Err(
                    "Grok installation failed its health check. AI-OS restored the previous version."
                        .to_string(),
                );
            }
            return Err(
                "Grok installation failed its health check and no previous version could be restored."
                    .to_string(),
            );
        }
    };
    crate::logs::append_ai_os_diagnostic("cli-provider id=grok event=health-verified");
    Ok(installed)
}

#[cfg(test)]
mod tests {
    use super::npm_candidates;

    #[test]
    fn npm_discovery_is_bounded_to_executable_name() {
        let paths = npm_candidates(Some("/one:/two".into()));
        assert!(paths.iter().all(|path| path.ends_with("npm")));
        assert!(paths.contains(&"/opt/homebrew/bin/npm".into()));
    }
}
