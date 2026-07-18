use std::path::Path;

use chrono::Utc;

use crate::{health, openclaw};

use super::{
    models::{
        NormalizedRuntimeError, RuntimeAvailability, RuntimeCapability, RuntimeDefinition,
        RuntimeErrorCode, RuntimeHealth, RuntimeLifecycle, RuntimeLocation, RuntimePlatform,
        RuntimeReadiness, RuntimeStatus, RuntimeStatusRequest,
    },
    registry,
};

fn observed_at() -> String {
    Utc::now().to_rfc3339()
}

fn status_from_running(
    definition: RuntimeDefinition,
    installed: bool,
    running: bool,
) -> RuntimeStatus {
    RuntimeStatus {
        id: definition.id,
        adapter_kind: definition.adapter_kind,
        supported_platform: RuntimePlatform::Macos,
        location: definition.location,
        dependencies: definition.dependencies,
        capabilities: definition.capabilities,
        availability: if running || installed {
            RuntimeAvailability::Available
        } else {
            RuntimeAvailability::NotInstalled
        },
        lifecycle: if running {
            RuntimeLifecycle::Running
        } else {
            RuntimeLifecycle::Stopped
        },
        health: if running {
            RuntimeHealth::Healthy
        } else {
            RuntimeHealth::Unknown
        },
        readiness: if running {
            RuntimeReadiness::Ready
        } else {
            RuntimeReadiness::NotReady
        },
        observed_at: observed_at(),
        error: None,
    }
}

fn definition(id: &str) -> RuntimeDefinition {
    registry::definitions()
        .into_iter()
        .find(|item| item.id == id)
        .expect("static runtime definition must exist")
}

fn openclaw_definition(remote: bool) -> RuntimeDefinition {
    let mut definition = definition("openclaw");
    definition.location = if remote {
        RuntimeLocation::Remote
    } else {
        RuntimeLocation::Local
    };

    if !remote {
        definition.capabilities.push(RuntimeCapability::Start);
        definition.capabilities.push(RuntimeCapability::Stop);
    }

    definition
}

fn openclaw_status() -> RuntimeStatus {
    match openclaw::runtime_snapshot() {
        Ok(snapshot) => map_openclaw_snapshot(openclaw_definition(snapshot.remote), snapshot),
        Err(_) => {
            let definition = definition("openclaw");

            RuntimeStatus {
                id: definition.id,
                adapter_kind: definition.adapter_kind,
                supported_platform: RuntimePlatform::Macos,
                location: definition.location,
                dependencies: definition.dependencies,
                capabilities: definition.capabilities,
                availability: RuntimeAvailability::Unavailable,
                lifecycle: RuntimeLifecycle::Failed,
                health: RuntimeHealth::Unhealthy,
                readiness: RuntimeReadiness::NotReady,
                observed_at: observed_at(),
                error: Some(NormalizedRuntimeError {
                    code: RuntimeErrorCode::ConfigurationUnavailable,
                    message: "OpenClaw configuration could not be read.".to_string(),
                    retryable: true,
                }),
            }
        }
    }
}

fn map_openclaw_snapshot(
    definition: RuntimeDefinition,
    snapshot: openclaw::OpenClawRuntimeSnapshot,
) -> RuntimeStatus {
    let connected = snapshot.connection_state == "connected";
    let unhealthy = matches!(
        snapshot.connection_state.as_str(),
        "unauthorized" | "unreachable" | "pairing-required" | "error"
    );

    RuntimeStatus {
        id: definition.id,
        adapter_kind: definition.adapter_kind,
        supported_platform: RuntimePlatform::Macos,
        location: definition.location,
        dependencies: definition.dependencies,
        capabilities: definition.capabilities,
        availability: if snapshot.configured {
            RuntimeAvailability::Available
        } else {
            RuntimeAvailability::Unavailable
        },
        lifecycle: if connected {
            RuntimeLifecycle::Running
        } else if unhealthy {
            RuntimeLifecycle::Failed
        } else if snapshot.configured {
            RuntimeLifecycle::Unknown
        } else {
            RuntimeLifecycle::Stopped
        },
        health: if connected {
            RuntimeHealth::Healthy
        } else if unhealthy {
            RuntimeHealth::Unhealthy
        } else {
            RuntimeHealth::Unknown
        },
        readiness: if connected {
            RuntimeReadiness::Ready
        } else if snapshot.configured && !unhealthy {
            RuntimeReadiness::Unknown
        } else {
            RuntimeReadiness::NotReady
        },
        observed_at: observed_at(),
        error: if snapshot.configured {
            None
        } else {
            Some(NormalizedRuntimeError {
                code: RuntimeErrorCode::ConfigurationUnavailable,
                message: "No active OpenClaw server is configured.".to_string(),
                retryable: false,
            })
        },
    }
}

pub fn statuses(request: RuntimeStatusRequest) -> Vec<RuntimeStatus> {
    let ollama_url = request
        .ollama_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("http://localhost:11434");
    let open_web_ui_url = request
        .open_web_ui_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("http://localhost:3000");

    let ollama_running = health::probe_ollama(ollama_url);
    let docker_running = health::probe_docker();
    let open_webui_running = health::probe_open_webui(open_web_ui_url);
    let cherry_running = health::probe_cherry_studio();

    let ollama_installed = ollama_running
        || Path::new("/opt/homebrew/bin/ollama").exists()
        || Path::new("/usr/local/bin/ollama").exists()
        || health::command_success("command -v ollama >/dev/null 2>&1");
    let docker_installed = docker_running || Path::new("/Applications/Docker.app").exists();
    let cherry_installed = cherry_running || Path::new("/Applications/Cherry Studio.app").exists();
    let open_webui_installed = open_webui_running || health::open_webui_container_exists();

    vec![
        openclaw_status(),
        status_from_running(definition("ollama"), ollama_installed, ollama_running),
        status_from_running(
            definition("docker-desktop"),
            docker_installed,
            docker_running,
        ),
        status_from_running(
            definition("open-webui"),
            open_webui_installed,
            open_webui_running,
        ),
        status_from_running(
            definition("cherry-studio"),
            cherry_installed,
            cherry_running,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openclaw::OpenClawRuntimeSnapshot;

    #[test]
    fn running_and_stopped_statuses_keep_dimensions_separate() {
        let running = status_from_running(definition("ollama"), true, true);
        assert_eq!(running.availability, RuntimeAvailability::Available);
        assert_eq!(running.lifecycle, RuntimeLifecycle::Running);
        assert_eq!(running.health, RuntimeHealth::Healthy);
        assert_eq!(running.readiness, RuntimeReadiness::Ready);

        let stopped = status_from_running(definition("ollama"), true, false);
        assert_eq!(stopped.availability, RuntimeAvailability::Available);
        assert_eq!(stopped.lifecycle, RuntimeLifecycle::Stopped);
        assert_eq!(stopped.health, RuntimeHealth::Unknown);
        assert_eq!(stopped.readiness, RuntimeReadiness::NotReady);
    }

    #[test]
    fn remote_openclaw_does_not_advertise_local_lifecycle_capabilities() {
        let status = map_openclaw_snapshot(
            openclaw_definition(true),
            OpenClawRuntimeSnapshot {
                configured: true,
                remote: true,
                connection_state: "connected".to_string(),
            },
        );

        assert_eq!(status.location, RuntimeLocation::Remote);
        assert!(!status.capabilities.contains(&RuntimeCapability::Start));
        assert!(!status.capabilities.contains(&RuntimeCapability::Stop));
    }

    #[test]
    fn local_openclaw_advertises_supported_local_lifecycle_capabilities() {
        let status = map_openclaw_snapshot(
            openclaw_definition(false),
            OpenClawRuntimeSnapshot {
                configured: true,
                remote: false,
                connection_state: "connected".to_string(),
            },
        );

        assert_eq!(status.location, RuntimeLocation::Local);
        assert!(status.capabilities.contains(&RuntimeCapability::Start));
        assert!(status.capabilities.contains(&RuntimeCapability::Stop));
    }

    #[test]
    fn safe_configuration_error_does_not_expose_adapter_details() {
        let status = map_openclaw_snapshot(
            definition("openclaw"),
            OpenClawRuntimeSnapshot {
                configured: false,
                remote: false,
                connection_state: "unknown".to_string(),
            },
        );

        let error = status
            .error
            .expect("missing configuration should be normalized");
        assert_eq!(error.code, RuntimeErrorCode::ConfigurationUnavailable);
        assert!(!error.message.contains("sensitive"));
    }
}
