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

fn observed_at() -> Option<String> {
    Some(Utc::now().to_rfc3339())
}

#[derive(Debug, Clone, Copy)]
struct RuntimeProbeFacts {
    installed: bool,
    process_running: bool,
    endpoint_healthy: bool,
}

#[derive(Debug, Clone, Copy)]
enum RuntimeProbeKind {
    EndpointService,
    ProcessApplication,
}

fn map_probe_facts(
    definition: RuntimeDefinition,
    facts: RuntimeProbeFacts,
    kind: RuntimeProbeKind,
) -> RuntimeStatus {
    let (availability, lifecycle, health, readiness) = if facts.endpoint_healthy {
        (
            RuntimeAvailability::Available,
            RuntimeLifecycle::Running,
            RuntimeHealth::Healthy,
            RuntimeReadiness::Ready,
        )
    } else if !facts.installed {
        (
            RuntimeAvailability::NotInstalled,
            RuntimeLifecycle::Unknown,
            RuntimeHealth::Unknown,
            RuntimeReadiness::NotReady,
        )
    } else if facts.process_running {
        let (health, readiness) = match kind {
            RuntimeProbeKind::EndpointService => {
                (RuntimeHealth::Degraded, RuntimeReadiness::NotReady)
            }
            RuntimeProbeKind::ProcessApplication => {
                (RuntimeHealth::Unknown, RuntimeReadiness::Ready)
            }
        };
        (
            RuntimeAvailability::Available,
            RuntimeLifecycle::Running,
            health,
            readiness,
        )
    } else {
        (
            RuntimeAvailability::Available,
            RuntimeLifecycle::Stopped,
            RuntimeHealth::Unknown,
            RuntimeReadiness::NotReady,
        )
    };

    RuntimeStatus {
        id: definition.id,
        adapter_kind: definition.adapter_kind,
        supported_platform: RuntimePlatform::Macos,
        location: definition.location,
        dependencies: definition.dependencies,
        capabilities: definition.capabilities,
        availability,
        lifecycle,
        health,
        readiness,
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

fn openclaw_definition(location: openclaw::OpenClawRuntimeLocation) -> RuntimeDefinition {
    let mut definition = definition("openclaw");
    definition.location = match location {
        openclaw::OpenClawRuntimeLocation::Local => RuntimeLocation::Local,
        openclaw::OpenClawRuntimeLocation::Remote => RuntimeLocation::Remote,
        openclaw::OpenClawRuntimeLocation::Invalid => RuntimeLocation::Hybrid,
    };

    if location == openclaw::OpenClawRuntimeLocation::Local {
        definition.capabilities.push(RuntimeCapability::Start);
        definition.capabilities.push(RuntimeCapability::Stop);
    }

    definition
}

fn openclaw_status() -> RuntimeStatus {
    match openclaw::runtime_snapshot() {
        Ok(snapshot) => map_openclaw_snapshot(openclaw_definition(snapshot.location), snapshot),
        Err(_) => openclaw_config_failure_status(),
    }
}

fn openclaw_config_failure_status() -> RuntimeStatus {
    let definition = definition("openclaw");

    RuntimeStatus {
        id: definition.id,
        adapter_kind: definition.adapter_kind,
        supported_platform: RuntimePlatform::Macos,
        location: definition.location,
        dependencies: definition.dependencies,
        capabilities: definition.capabilities,
        availability: RuntimeAvailability::Unavailable,
        lifecycle: RuntimeLifecycle::Unknown,
        health: RuntimeHealth::Unknown,
        readiness: RuntimeReadiness::NotReady,
        observed_at: observed_at(),
        error: Some(runtime_error(
            RuntimeErrorCode::ConfigurationUnavailable,
            "OpenClaw configuration could not be read.",
            true,
        )),
    }
}

fn map_openclaw_snapshot(
    definition: RuntimeDefinition,
    snapshot: openclaw::OpenClawRuntimeSnapshot,
) -> RuntimeStatus {
    let valid_observation = snapshot
        .last_checked_at
        .as_deref()
        .filter(|value| chrono::DateTime::parse_from_rfc3339(value).is_ok());
    let connection_state = if valid_observation.is_some() {
        snapshot.connection_state.as_str()
    } else {
        "unknown"
    };
    let (lifecycle, health, readiness, error) = match connection_state {
        "connected" => (
            RuntimeLifecycle::Running,
            RuntimeHealth::Healthy,
            RuntimeReadiness::Ready,
            None,
        ),
        "testing" => (
            RuntimeLifecycle::Unknown,
            RuntimeHealth::Checking,
            RuntimeReadiness::Unknown,
            None,
        ),
        "unauthorized" => (
            RuntimeLifecycle::Running,
            RuntimeHealth::Degraded,
            RuntimeReadiness::NotReady,
            Some(runtime_error(
                RuntimeErrorCode::AuthenticationRequired,
                "OpenClaw authentication is required.",
                false,
            )),
        ),
        "pairing-required" => (
            RuntimeLifecycle::Running,
            RuntimeHealth::Degraded,
            RuntimeReadiness::NotReady,
            Some(runtime_error(
                RuntimeErrorCode::PairingRequired,
                "OpenClaw pairing is required.",
                false,
            )),
        ),
        "unreachable" => (
            RuntimeLifecycle::Unknown,
            RuntimeHealth::Unhealthy,
            RuntimeReadiness::NotReady,
            Some(runtime_error(
                RuntimeErrorCode::ConnectionUnavailable,
                "OpenClaw is unreachable.",
                true,
            )),
        ),
        "error" => (
            RuntimeLifecycle::Unknown,
            RuntimeHealth::Unhealthy,
            RuntimeReadiness::NotReady,
            Some(runtime_error(
                RuntimeErrorCode::ProbeFailed,
                "OpenClaw status could not be determined.",
                true,
            )),
        ),
        _ => (
            RuntimeLifecycle::Unknown,
            RuntimeHealth::Unknown,
            RuntimeReadiness::Unknown,
            None,
        ),
    };

    let (availability, lifecycle, health, readiness, error) = if !snapshot.configured {
        (
            RuntimeAvailability::Unavailable,
            RuntimeLifecycle::Unknown,
            RuntimeHealth::Unknown,
            RuntimeReadiness::NotReady,
            Some(runtime_error(
                RuntimeErrorCode::ConfigurationUnavailable,
                "No active OpenClaw server is configured.",
                false,
            )),
        )
    } else if snapshot.location == openclaw::OpenClawRuntimeLocation::Invalid {
        (
            RuntimeAvailability::Unavailable,
            RuntimeLifecycle::Unknown,
            RuntimeHealth::Unknown,
            RuntimeReadiness::NotReady,
            Some(runtime_error(
                RuntimeErrorCode::InvalidConfiguration,
                "The active OpenClaw server URL is invalid.",
                false,
            )),
        )
    } else {
        (
            RuntimeAvailability::Available,
            lifecycle,
            health,
            readiness,
            error,
        )
    };

    RuntimeStatus {
        id: definition.id,
        adapter_kind: definition.adapter_kind,
        supported_platform: RuntimePlatform::Macos,
        location: definition.location,
        dependencies: definition.dependencies,
        capabilities: definition.capabilities,
        availability,
        lifecycle,
        health,
        readiness,
        observed_at: if !snapshot.configured
            || snapshot.location == openclaw::OpenClawRuntimeLocation::Invalid
        {
            observed_at()
        } else {
            valid_observation.map(str::to_string)
        },
        error,
    }
}

fn runtime_error(code: RuntimeErrorCode, message: &str, retryable: bool) -> NormalizedRuntimeError {
    NormalizedRuntimeError {
        code,
        message: message.to_string(),
        retryable,
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

    let ollama_endpoint_healthy = health::probe_ollama_endpoint(ollama_url);
    let ollama_process_running = health::probe_ollama_process();
    let docker_daemon_ready = health::probe_docker();
    let docker_process_running = health::probe_docker_process();
    let open_webui_endpoint_healthy = health::probe_open_webui_endpoint(open_web_ui_url);
    let open_webui_container_running = health::probe_open_webui_container();
    let open_webui_container_exists = health::open_webui_container_exists();
    let cherry_process_running = health::probe_cherry_studio();

    let ollama_installed = ollama_process_running
        || Path::new("/opt/homebrew/bin/ollama").exists()
        || Path::new("/usr/local/bin/ollama").exists()
        || health::command_success("command -v ollama >/dev/null 2>&1");
    let docker_installed = docker_process_running || Path::new("/Applications/Docker.app").exists();
    let cherry_installed =
        cherry_process_running || Path::new("/Applications/Cherry Studio.app").exists();

    vec![
        openclaw_status(),
        map_probe_facts(
            definition("ollama"),
            RuntimeProbeFacts {
                installed: ollama_installed,
                process_running: ollama_process_running,
                endpoint_healthy: ollama_endpoint_healthy,
            },
            RuntimeProbeKind::EndpointService,
        ),
        map_probe_facts(
            definition("docker-desktop"),
            RuntimeProbeFacts {
                installed: docker_installed,
                process_running: docker_process_running,
                endpoint_healthy: docker_daemon_ready,
            },
            RuntimeProbeKind::EndpointService,
        ),
        map_probe_facts(
            definition("open-webui"),
            RuntimeProbeFacts {
                installed: open_webui_container_exists,
                process_running: open_webui_container_running,
                endpoint_healthy: open_webui_endpoint_healthy,
            },
            RuntimeProbeKind::EndpointService,
        ),
        map_probe_facts(
            definition("cherry-studio"),
            RuntimeProbeFacts {
                installed: cherry_installed,
                process_running: cherry_process_running,
                endpoint_healthy: false,
            },
            RuntimeProbeKind::ProcessApplication,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openclaw::{OpenClawRuntimeLocation, OpenClawRuntimeSnapshot};

    const CHECKED_AT: &str = "2026-07-18T03:15:00Z";

    fn snapshot(
        configured: bool,
        location: OpenClawRuntimeLocation,
        state: &str,
        last_checked_at: Option<&str>,
    ) -> OpenClawRuntimeSnapshot {
        OpenClawRuntimeSnapshot {
            configured,
            location,
            connection_state: state.to_string(),
            last_checked_at: last_checked_at.map(str::to_string),
        }
    }

    fn mapped(state: &str) -> RuntimeStatus {
        map_openclaw_snapshot(
            openclaw_definition(OpenClawRuntimeLocation::Remote),
            snapshot(
                true,
                OpenClawRuntimeLocation::Remote,
                state,
                Some(CHECKED_AT),
            ),
        )
    }

    #[test]
    fn endpoint_process_and_installation_facts_map_independently() {
        let endpoint_ready = map_probe_facts(
            definition("ollama"),
            RuntimeProbeFacts {
                installed: false,
                process_running: false,
                endpoint_healthy: true,
            },
            RuntimeProbeKind::EndpointService,
        );
        assert_eq!(endpoint_ready.availability, RuntimeAvailability::Available);
        assert_eq!(endpoint_ready.lifecycle, RuntimeLifecycle::Running);
        assert_eq!(endpoint_ready.health, RuntimeHealth::Healthy);
        assert_eq!(endpoint_ready.readiness, RuntimeReadiness::Ready);

        let process_only = map_probe_facts(
            definition("ollama"),
            RuntimeProbeFacts {
                installed: true,
                process_running: true,
                endpoint_healthy: false,
            },
            RuntimeProbeKind::EndpointService,
        );
        assert_eq!(process_only.lifecycle, RuntimeLifecycle::Running);
        assert_eq!(process_only.health, RuntimeHealth::Degraded);
        assert_eq!(process_only.readiness, RuntimeReadiness::NotReady);

        let stopped = map_probe_facts(
            definition("ollama"),
            RuntimeProbeFacts {
                installed: true,
                process_running: false,
                endpoint_healthy: false,
            },
            RuntimeProbeKind::EndpointService,
        );
        assert_eq!(stopped.availability, RuntimeAvailability::Available);
        assert_eq!(stopped.lifecycle, RuntimeLifecycle::Stopped);
        assert_eq!(stopped.health, RuntimeHealth::Unknown);
        assert_eq!(stopped.readiness, RuntimeReadiness::NotReady);
    }

    #[test]
    fn not_installed_never_claims_a_stopped_lifecycle() {
        let status = map_probe_facts(
            definition("docker-desktop"),
            RuntimeProbeFacts {
                installed: false,
                process_running: false,
                endpoint_healthy: false,
            },
            RuntimeProbeKind::EndpointService,
        );

        assert_eq!(status.availability, RuntimeAvailability::NotInstalled);
        assert_eq!(status.lifecycle, RuntimeLifecycle::Unknown);
        assert_eq!(status.health, RuntimeHealth::Unknown);
        assert_eq!(status.readiness, RuntimeReadiness::NotReady);
    }

    #[test]
    fn cherry_process_presence_does_not_claim_health() {
        let status = map_probe_facts(
            definition("cherry-studio"),
            RuntimeProbeFacts {
                installed: true,
                process_running: true,
                endpoint_healthy: false,
            },
            RuntimeProbeKind::ProcessApplication,
        );

        assert_eq!(status.lifecycle, RuntimeLifecycle::Running);
        assert_eq!(status.health, RuntimeHealth::Unknown);
        assert_eq!(status.readiness, RuntimeReadiness::Ready);
    }

    #[test]
    fn remote_openclaw_does_not_advertise_local_lifecycle_capabilities() {
        let status = map_openclaw_snapshot(
            openclaw_definition(OpenClawRuntimeLocation::Remote),
            snapshot(
                true,
                OpenClawRuntimeLocation::Remote,
                "connected",
                Some(CHECKED_AT),
            ),
        );

        assert_eq!(status.location, RuntimeLocation::Remote);
        assert!(!status.capabilities.contains(&RuntimeCapability::Start));
        assert!(!status.capabilities.contains(&RuntimeCapability::Stop));
    }

    #[test]
    fn local_openclaw_advertises_supported_local_lifecycle_capabilities() {
        let status = map_openclaw_snapshot(
            openclaw_definition(OpenClawRuntimeLocation::Local),
            snapshot(
                true,
                OpenClawRuntimeLocation::Local,
                "connected",
                Some(CHECKED_AT),
            ),
        );

        assert_eq!(status.location, RuntimeLocation::Local);
        assert!(status.capabilities.contains(&RuntimeCapability::Start));
        assert!(status.capabilities.contains(&RuntimeCapability::Stop));
    }

    #[test]
    fn invalid_location_never_receives_local_lifecycle_capabilities() {
        let status = map_openclaw_snapshot(
            openclaw_definition(OpenClawRuntimeLocation::Invalid),
            snapshot(
                true,
                OpenClawRuntimeLocation::Invalid,
                "connected",
                Some(CHECKED_AT),
            ),
        );

        assert_eq!(status.location, RuntimeLocation::Hybrid);
        assert!(!status.capabilities.contains(&RuntimeCapability::Start));
        assert!(!status.capabilities.contains(&RuntimeCapability::Stop));
        assert_eq!(
            status.error.unwrap().code,
            RuntimeErrorCode::InvalidConfiguration
        );
    }

    #[test]
    fn unauthorized_keeps_status_dimensions_independent() {
        let status = mapped("unauthorized");
        assert_eq!(status.lifecycle, RuntimeLifecycle::Running);
        assert_eq!(status.health, RuntimeHealth::Degraded);
        assert_eq!(status.readiness, RuntimeReadiness::NotReady);
        assert_eq!(
            status.error.unwrap().code,
            RuntimeErrorCode::AuthenticationRequired
        );
    }

    #[test]
    fn pairing_required_keeps_status_dimensions_independent() {
        let status = mapped("pairing-required");
        assert_eq!(status.lifecycle, RuntimeLifecycle::Running);
        assert_eq!(status.health, RuntimeHealth::Degraded);
        assert_eq!(status.readiness, RuntimeReadiness::NotReady);
        assert_eq!(
            status.error.unwrap().code,
            RuntimeErrorCode::PairingRequired
        );
    }

    #[test]
    fn unreachable_does_not_claim_a_failed_lifecycle() {
        let status = mapped("unreachable");
        assert_eq!(status.lifecycle, RuntimeLifecycle::Unknown);
        assert_eq!(status.health, RuntimeHealth::Unhealthy);
        assert_eq!(status.readiness, RuntimeReadiness::NotReady);
        assert_eq!(
            status.error.unwrap().code,
            RuntimeErrorCode::ConnectionUnavailable
        );
    }

    #[test]
    fn testing_error_and_unknown_states_map_without_fabricated_lifecycle() {
        let testing = mapped("testing");
        assert_eq!(testing.lifecycle, RuntimeLifecycle::Unknown);
        assert_eq!(testing.health, RuntimeHealth::Checking);
        assert_eq!(testing.readiness, RuntimeReadiness::Unknown);
        assert!(testing.error.is_none());

        let error = mapped("error");
        assert_eq!(error.lifecycle, RuntimeLifecycle::Unknown);
        assert_eq!(error.health, RuntimeHealth::Unhealthy);
        assert_eq!(error.readiness, RuntimeReadiness::NotReady);
        assert_eq!(error.error.unwrap().code, RuntimeErrorCode::ProbeFailed);

        let unknown = mapped("");
        assert_eq!(unknown.lifecycle, RuntimeLifecycle::Unknown);
        assert_eq!(unknown.health, RuntimeHealth::Unknown);
        assert_eq!(unknown.readiness, RuntimeReadiness::Unknown);
        assert!(unknown.error.is_none());
    }

    #[test]
    fn no_active_configuration_has_unknown_lifecycle() {
        let status = map_openclaw_snapshot(
            openclaw_definition(OpenClawRuntimeLocation::Invalid),
            snapshot(false, OpenClawRuntimeLocation::Invalid, "unknown", None),
        );

        assert_eq!(status.lifecycle, RuntimeLifecycle::Unknown);
        let error = status
            .error
            .expect("missing configuration should be normalized");
        assert_eq!(error.code, RuntimeErrorCode::ConfigurationUnavailable);
    }

    #[test]
    fn configuration_read_failure_does_not_claim_failed_lifecycle() {
        let status = openclaw_config_failure_status();
        assert_eq!(status.lifecycle, RuntimeLifecycle::Unknown);
        assert_eq!(status.health, RuntimeHealth::Unknown);
        assert_eq!(
            status.error.unwrap().code,
            RuntimeErrorCode::ConfigurationUnavailable
        );
        assert!(status.observed_at.is_some());
    }

    #[test]
    fn stored_observation_uses_its_real_timestamp() {
        let status = mapped("connected");
        assert_eq!(status.observed_at.as_deref(), Some(CHECKED_AT));
        assert_eq!(status.health, RuntimeHealth::Healthy);
        assert_eq!(status.readiness, RuntimeReadiness::Ready);
    }

    #[test]
    fn connected_without_timestamp_has_no_observation_or_derived_status() {
        let status = map_openclaw_snapshot(
            openclaw_definition(OpenClawRuntimeLocation::Remote),
            snapshot(true, OpenClawRuntimeLocation::Remote, "connected", None),
        );

        assert_eq!(status.lifecycle, RuntimeLifecycle::Unknown);
        assert_eq!(status.health, RuntimeHealth::Unknown);
        assert_eq!(status.readiness, RuntimeReadiness::Unknown);
        assert!(status.observed_at.is_none());
    }

    #[test]
    fn unverified_error_states_have_no_observation_or_derived_status() {
        for (state, timestamp) in [
            ("unauthorized", None),
            ("unreachable", Some("not-a-timestamp")),
        ] {
            let status = map_openclaw_snapshot(
                openclaw_definition(OpenClawRuntimeLocation::Remote),
                snapshot(true, OpenClawRuntimeLocation::Remote, state, timestamp),
            );

            assert_eq!(status.lifecycle, RuntimeLifecycle::Unknown);
            assert_eq!(status.health, RuntimeHealth::Unknown);
            assert_eq!(status.readiness, RuntimeReadiness::Unknown);
            assert!(status.observed_at.is_none());
            assert!(status.error.is_none());
        }
    }

    #[test]
    fn normalized_errors_are_sanitized() {
        for state in ["unauthorized", "pairing-required", "unreachable", "error"] {
            let error = mapped(state).error.expect("state should map to an error");
            assert!(!error.message.contains("token"));
            assert!(!error.message.contains("http"));
            assert!(!error.message.contains('{'));
        }
    }
}
