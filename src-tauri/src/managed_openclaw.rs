use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use crate::openclaw::{self, OpenClawServerInput};

pub(crate) const MANAGED_OPENCLAW_NAME: &str = "AI-OS Managed OpenClaw";
pub(crate) const MANAGED_OPENCLAW_URL: &str = "http://127.0.0.1:18789";
const OPENCLAW_INSTALLER_URL: &str = "https://openclaw.ai/install-cli.sh";
const OPENCLAW_INSTALLER_SHA256: &str =
    "7ad6910e76cf692eb60bed987a26637548cc5363164031ddf6ffec480ae1458b";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedOpenClawStatus {
    pub state: String,
    pub installed: bool,
    pub configured: bool,
    pub gateway_service_installed: bool,
    pub connected: bool,
    pub version: Option<String>,
    pub managed: bool,
    pub credential_required: bool,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedOpenClawInstallResult {
    pub success: bool,
    pub state: String,
    pub message: String,
    pub version: Option<String>,
}

fn managed_prefix() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|home| home.join(".openclaw"))
        .ok_or_else(|| "AI-OS could not determine the home directory.".to_string())
}

fn managed_cli() -> Result<PathBuf, String> {
    Ok(managed_prefix()?.join("bin/openclaw"))
}

pub(crate) fn command_text(program: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("AI-OS could not start {}: {error}", program.display()))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output.status.success() {
        Ok(stdout)
    } else if !stderr.is_empty() {
        Err(stderr)
    } else if !stdout.is_empty() {
        Err(stdout)
    } else {
        Err(format!(
            "{} exited with status {}.",
            program.display(),
            output.status
        ))
    }
}

pub(crate) fn discover_cli() -> Option<(PathBuf, String)> {
    let mut candidates = Vec::new();

    if let Ok(path) = managed_cli() {
        candidates.push(path);
    }

    candidates.push(PathBuf::from("/opt/homebrew/bin/openclaw"));
    candidates.push(PathBuf::from("/usr/local/bin/openclaw"));
    candidates.push(PathBuf::from("openclaw"));

    candidates.into_iter().find_map(|candidate| {
        command_text(&candidate, &["--version"])
            .ok()
            .filter(|version| !version.is_empty())
            .map(|version| (candidate, version))
    })
}

fn gateway_service_installed() -> bool {
    dirs::home_dir()
        .map(|home| {
            home.join("Library/LaunchAgents/ai.openclaw.gateway.plist")
                .exists()
        })
        .unwrap_or(false)
}

fn native_openclaw_config_exists() -> bool {
    managed_prefix()
        .map(|prefix| prefix.join("openclaw.json").is_file())
        .unwrap_or(false)
}

#[derive(Debug, Clone, Copy)]
struct ManagedStateFacts {
    installed: bool,
    native_config: bool,
    service_installed: bool,
    ai_os_configured: bool,
    managed_record: bool,
    local: bool,
    connected: bool,
    keychain_unavailable: bool,
    unauthorized: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct ManagedStateDecision {
    state: &'static str,
    managed: bool,
    credential_required: bool,
    diagnostic: Option<&'static str>,
}

fn classify_managed_state(facts: ManagedStateFacts) -> ManagedStateDecision {
    if facts.ai_os_configured {
        if facts.connected {
            return ManagedStateDecision {
                state: "ready",
                managed: facts.managed_record,
                credential_required: false,
                diagnostic: None,
            };
        }

        if !facts.managed_record {
            return ManagedStateDecision {
                state: "existing-unmanaged",
                managed: false,
                credential_required: true,
                diagnostic: Some(
                    "AI-OS found an existing OpenClaw connection, but it could not be verified. Provide the existing Gateway token to adopt it without changing OpenClaw.",
                ),
            };
        }

        if !facts.installed {
            return ManagedStateDecision {
                state: "broken",
                managed: true,
                credential_required: false,
                diagnostic: Some(
                    "The AI-OS managed server record exists, but the OpenClaw CLI is missing. AI-OS will not reinstall it without explicit repair approval.",
                ),
            };
        }

        if !facts.local {
            return ManagedStateDecision {
                state: "broken",
                managed: true,
                credential_required: false,
                diagnostic: Some(
                    "The AI-OS managed server record no longer points to the local Gateway.",
                ),
            };
        }

        if facts.keychain_unavailable {
            return ManagedStateDecision {
                state: "broken",
                managed: true,
                credential_required: false,
                diagnostic: Some(
                    "The managed Gateway token reference exists, but its Keychain secret is unavailable. AI-OS will not rotate it automatically.",
                ),
            };
        }

        if !facts.service_installed {
            return ManagedStateDecision {
                state: "configured-no-gateway",
                managed: true,
                credential_required: false,
                diagnostic: Some(
                    "Secure managed configuration exists, but the local Gateway service is not installed.",
                ),
            };
        }

        if facts.unauthorized {
            return ManagedStateDecision {
                state: "broken",
                managed: true,
                credential_required: false,
                diagnostic: Some(
                    "The managed Gateway rejected its stored credential. AI-OS will not replace or rotate the token automatically.",
                ),
            };
        }

        return ManagedStateDecision {
            state: "gateway-stopped",
            managed: true,
            credential_required: false,
            diagnostic: Some(
                "The AI-OS managed Gateway service is installed but is not reachable.",
            ),
        };
    }

    if facts.native_config {
        if facts.installed {
            return ManagedStateDecision {
                state: "existing-unmanaged",
                managed: false,
                credential_required: true,
                diagnostic: Some(
                    "An existing OpenClaw configuration was found. AI-OS will leave it unchanged and requires the existing Gateway credential to adopt it.",
                ),
            };
        }

        return ManagedStateDecision {
            state: "broken",
            managed: false,
            credential_required: false,
            diagnostic: Some(
                "An OpenClaw configuration exists, but the CLI is missing. AI-OS will not overwrite or delete that configuration.",
            ),
        };
    }

    if facts.service_installed {
        return ManagedStateDecision {
            state: "broken",
            managed: false,
            credential_required: false,
            diagnostic: Some(
                "An OpenClaw Gateway service exists without a verifiable AI-OS managed configuration.",
            ),
        };
    }

    ManagedStateDecision {
        state: if facts.installed {
            "installed-not-configured"
        } else {
            "not-installed"
        },
        managed: false,
        credential_required: false,
        diagnostic: None,
    }
}

#[tauri::command]
pub fn get_managed_openclaw_status() -> Result<ManagedOpenClawStatus, String> {
    let discovered = discover_cli();
    let snapshot = openclaw::runtime_snapshot()?;
    let installed = discovered.is_some();
    let service_installed = gateway_service_installed();
    let local = matches!(snapshot.location, openclaw::OpenClawRuntimeLocation::Local);

    let managed_record = snapshot.active_server_name.as_deref() == Some(MANAGED_OPENCLAW_NAME)
        && snapshot.active_token_reference.as_deref()
            == Some(crate::managed_openclaw_secret::MANAGED_TOKEN_REF);

    let live_status = if snapshot.configured {
        Some(openclaw::get_active_openclaw_status())
    } else {
        None
    };

    let connected = matches!(live_status.as_ref(), Some(Ok(status)) if status.connected);
    let unauthorized = matches!(
        live_status.as_ref(),
        Some(Ok(status)) if status.gateway_status.as_deref() == Some("unauthorized")
    );
    let keychain_unavailable = matches!(
        live_status.as_ref(),
        Some(Err(message)) if message.contains("Keychain token")
            || message.contains("token from Keychain")
    );

    let decision = classify_managed_state(ManagedStateFacts {
        installed,
        native_config: native_openclaw_config_exists(),
        service_installed,
        ai_os_configured: snapshot.configured,
        managed_record,
        local,
        connected,
        keychain_unavailable,
        unauthorized,
    });

    Ok(ManagedOpenClawStatus {
        state: decision.state.to_string(),
        installed,
        configured: snapshot.configured,
        gateway_service_installed: service_installed,
        connected,
        version: discovered.map(|(_, version)| version),
        managed: decision.managed,
        credential_required: decision.credential_required,
        diagnostic: decision.diagnostic.map(str::to_string),
    })
}

fn download_official_installer(user_approved: bool) -> Result<tempfile::NamedTempFile, String> {
    crate::software_installation::download_verified(
        &crate::software_installation::VerifiedDownloadPolicy {
            id: "openclaw-cli-installer",
            url: OPENCLAW_INSTALLER_URL,
            sha256: OPENCLAW_INSTALLER_SHA256,
            max_bytes: 5_000_000,
            supported_os: &["macos"],
            supported_arches: &["aarch64", "x86_64"],
        },
        user_approved,
    )
}

pub(crate) fn upsert_managed_gateway(credential_reference: &str) -> Result<(), String> {
    let servers = openclaw::list_openclaw_servers()?;

    let input = OpenClawServerInput {
        name: MANAGED_OPENCLAW_NAME.to_string(),
        server_url: MANAGED_OPENCLAW_URL.to_string(),
        gateway_token: credential_reference.to_string(),
        enabled: true,
        auto_connect: true,
    };

    let server_id =
        if let Some(existing) = servers
            .iter()
            .find(|server| server.name == MANAGED_OPENCLAW_NAME)
        {
            let result = openclaw::update_openclaw_server(existing.id.clone(), input);

            if !result.success {
                return Err(result.message);
            }

            result.server.map(|server| server.id).ok_or_else(|| {
                "AI-OS updated OpenClaw but received no server record.".to_string()
            })?
        } else {
            let result = openclaw::save_openclaw_server(input);

            if !result.success {
                return Err(result.message);
            }

            result.server.map(|server| server.id).ok_or_else(|| {
                "AI-OS created OpenClaw but received no server record.".to_string()
            })?
        };

    let activation = openclaw::set_active_openclaw_server(server_id.clone());

    if !activation.success {
        return Err(activation.message);
    }

    let connection = openclaw::test_openclaw_connection(server_id);

    if !connection.success {
        return Err(connection.message);
    }

    Ok(())
}

pub(crate) fn gateway_install_args() -> [&'static str; 5] {
    ["gateway", "install", "--runtime", "node", "--json"]
}

pub(crate) fn wait_for_gateway(cli: &Path) -> Result<(), String> {
    let mut last_error = "OpenClaw Gateway did not become ready.".to_string();

    for _ in 0..20 {
        match command_text(cli, &["gateway", "status", "--require-rpc", "--json"]) {
            Ok(_) => return Ok(()),
            Err(error) => {
                last_error = error;
            }
        }

        std::thread::sleep(Duration::from_secs(1));
    }

    Err(last_error)
}

pub(crate) fn finish_secure_managed_gateway(cli: &Path) -> Result<(), String> {
    command_text(cli, &gateway_install_args())?;

    let _ = command_text(cli, &["gateway", "start", "--json"]);

    wait_for_gateway(cli)?;
    upsert_managed_gateway(crate::managed_openclaw_secret::MANAGED_TOKEN_REF)
}

pub(crate) fn complete_secure_managed_gateway(cli: &Path) -> Result<(), String> {
    prepare_secure_managed_gateway(cli)?;
    finish_secure_managed_gateway(cli)
}

fn install_managed_openclaw_blocking(user_approved: bool) -> Result<String, String> {
    let existing = get_managed_openclaw_status()?;

    if existing.state == "ready" {
        return existing
            .version
            .ok_or_else(|| "OpenClaw is ready but its version could not be read.".to_string());
    }

    if existing.state != "not-installed" {
        return Err(
            "An existing OpenClaw installation or configuration was detected. \
AI-OS will not overwrite it automatically."
                .to_string(),
        );
    }

    let prefix = managed_prefix()?;
    let prefix_text = prefix.to_string_lossy().to_string();

    let installer = download_official_installer(user_approved)?;

    command_text(
        Path::new("/bin/bash"),
        &[
            installer.path().to_string_lossy().as_ref(),
            "--runtime-only",
            "--json",
            "--prefix",
            &prefix_text,
        ],
    )?;

    let cli = managed_cli()?;

    if !cli.exists() {
        return Err("OpenClaw installation completed but the managed CLI is missing.".to_string());
    }

    complete_secure_managed_gateway(&cli)?;

    command_text(&cli, &["--version"])
}

#[tauri::command]
pub async fn install_managed_openclaw(user_approved: bool) -> ManagedOpenClawInstallResult {
    match tauri::async_runtime::spawn_blocking(move || {
        install_managed_openclaw_blocking(user_approved)
    })
    .await
    {
        Ok(Ok(version)) => ManagedOpenClawInstallResult {
            success: true,
            state: "ready".to_string(),
            message: "OpenClaw was installed, configured and verified by AI-OS.".to_string(),
            version: Some(version),
        },

        Ok(Err(message)) => ManagedOpenClawInstallResult {
            success: false,
            state: "failed".to_string(),
            message,
            version: None,
        },

        Err(_) => ManagedOpenClawInstallResult {
            success: false,
            state: "failed".to_string(),
            message: "AI-OS could not complete the OpenClaw setup task.".to_string(),
            version: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_managed_state, ManagedStateFacts};

    fn facts() -> ManagedStateFacts {
        ManagedStateFacts {
            installed: false,
            native_config: false,
            service_installed: false,
            ai_os_configured: false,
            managed_record: false,
            local: true,
            connected: false,
            keychain_unavailable: false,
            unauthorized: false,
        }
    }

    #[test]
    fn managed_state_matrix_is_deterministic() {
        assert_eq!(classify_managed_state(facts()).state, "not-installed");

        assert_eq!(
            classify_managed_state(ManagedStateFacts {
                installed: true,
                ..facts()
            })
            .state,
            "installed-not-configured"
        );

        assert_eq!(
            classify_managed_state(ManagedStateFacts {
                installed: true,
                ai_os_configured: true,
                managed_record: true,
                service_installed: false,
                ..facts()
            })
            .state,
            "configured-no-gateway"
        );
    }
}

#[cfg(test)]
mod managed_live_status_tests {
    use super::{classify_managed_state, ManagedStateFacts};

    #[test]
    fn managed_live_state_reflects_live_gateway() {
        let base = ManagedStateFacts {
            installed: true,
            native_config: true,
            service_installed: true,
            ai_os_configured: true,
            managed_record: true,
            local: true,
            connected: false,
            keychain_unavailable: false,
            unauthorized: false,
        };
        assert_eq!(classify_managed_state(base).state, "gateway-stopped");
        assert_eq!(
            classify_managed_state(ManagedStateFacts {
                connected: true,
                ..base
            })
            .state,
            "ready"
        );
        assert_eq!(
            classify_managed_state(ManagedStateFacts {
                keychain_unavailable: true,
                ..base
            })
            .state,
            "broken"
        );
        assert_eq!(
            classify_managed_state(ManagedStateFacts {
                unauthorized: true,
                ..base
            })
            .state,
            "broken"
        );
    }
}

#[cfg(test)]
mod managed_existing_installation_tests {
    use super::{classify_managed_state, ManagedStateFacts};

    #[test]
    fn managed_unconfigured_state_distinguishes_existing_config() {
        let base = ManagedStateFacts {
            installed: true,
            native_config: true,
            service_installed: false,
            ai_os_configured: false,
            managed_record: false,
            local: true,
            connected: false,
            keychain_unavailable: false,
            unauthorized: false,
        };
        let existing = classify_managed_state(base);
        assert_eq!(existing.state, "existing-unmanaged");
        assert!(existing.credential_required);

        let missing_cli = classify_managed_state(ManagedStateFacts {
            installed: false,
            ..base
        });
        assert_eq!(missing_cli.state, "broken");
        assert!(missing_cli.diagnostic.is_some());
    }
}

#[cfg(test)]
mod managed_fresh_install_security_tests {
    use super::{gateway_install_args, managed_secret_store_args};

    #[test]
    fn fresh_install_plan_never_places_token_in_argv() {
        let install = gateway_install_args();
        let secret = managed_secret_store_args();
        assert!(!install.contains(&"--token"));
        assert!(!secret.contains(&"--token"));
        assert!(secret.contains(&"--value-file"));
    }
}

// AI_OS_MANAGED_OPENCLAW_SECURE_SETUP_V1
fn command_text_with_stdin(program: &Path, args: &[&str], input: &str) -> Result<String, String> {
    use std::io::Write as _;
    let mut child = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|_| "AI-OS could not start OpenClaw.".to_string())?;
    child
        .stdin
        .take()
        .ok_or_else(|| "AI-OS could not open OpenClaw stdin.".to_string())?
        .write_all(input.as_bytes())
        .map_err(|_| "AI-OS could not send the OpenClaw secret.".to_string())?;
    let output = child
        .wait_with_output()
        .map_err(|_| "AI-OS could not read OpenClaw output.".to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err("OpenClaw rejected the secure secret input.".to_string())
    }
}

fn managed_secret_store_args() -> [&'static str; 7] {
    [
        "secrets",
        "store",
        "set",
        "AI_OS_GATEWAY_TOKEN",
        "--kind",
        "secret",
        "--value-file",
    ]
}

pub(crate) fn prepare_secure_managed_gateway(cli: &Path) -> Result<(), String> {
    command_text(cli, &["setup", "--baseline"])?;
    let token = format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
    crate::managed_openclaw_secret::store_managed_gateway_token(&token)?;
    let mut args = managed_secret_store_args().to_vec();
    args.push("-");
    command_text_with_stdin(cli, &args, &token)?;
    command_text(cli, &["config", "set", "gateway.mode", "local"])?;
    command_text(cli, &["config", "set", "gateway.bind", "loopback"])?;
    command_text(cli, &["config", "set", "gateway.auth.mode", "token"])?;
    command_text(
        cli,
        &[
            "config",
            "set",
            "gateway.auth.token",
            "--ref-provider",
            "default",
            "--ref-source",
            "store",
            "--ref-id",
            "AI_OS_GATEWAY_TOKEN",
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod managed_secure_setup_tests {
    #[test]
    fn managed_secret_plan_never_places_token_in_argv() {
        let args = super::managed_secret_store_args();
        assert!(!args.contains(&"--token"));
        assert!(args.contains(&"--value-file"));
    }
}
