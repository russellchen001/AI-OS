mod adapters;
pub(crate) mod agent_execution;
pub(crate) mod agent_skill_transport;
pub(crate) mod bulk;
pub(crate) mod capability_permission;
pub(crate) mod executor;
pub(crate) mod ipc;
pub(crate) mod lifecycle;
pub(crate) mod mano_fallback;
pub mod models;
pub(crate) mod openclaw_execution;
pub(crate) mod openclaw_gateway_adapter;
pub(crate) mod openclaw_permission;
pub mod operations;
pub(crate) mod plan_runtime_bridge;
pub(crate) mod recovery;
mod registry;
pub(crate) mod scheduler;
pub(crate) mod skill_invocation;
pub(crate) mod skills;
pub(crate) mod speech;
pub(crate) mod trusted_automation;
pub(crate) mod web_search;

use models::{RuntimeDefinition, RuntimeStatus};

#[tauri::command]
pub fn list_runtimes() -> Vec<RuntimeDefinition> {
    registry::definitions()
}

#[tauri::command]
pub async fn get_runtime_statuses() -> Result<Vec<RuntimeStatus>, String> {
    tauri::async_runtime::spawn_blocking(adapters::statuses)
        .await
        .map_err(|_| "Runtime status collection could not be completed.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use models::{
        NormalizedRuntimeError, RuntimeAdapterKind, RuntimeCapability, RuntimeErrorCode,
        RuntimeLocation,
    };

    #[test]
    fn registry_has_two_stable_ids() {
        let ids = list_runtimes()
            .into_iter()
            .map(|runtime| runtime.id)
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["openclaw", "cherry-studio"]);
    }

    #[test]
    fn serialization_uses_typescript_friendly_names() {
        assert_eq!(
            serde_json::to_string(&RuntimeAdapterKind::CherryStudio).unwrap(),
            "\"cherry-studio\""
        );
        assert_eq!(
            serde_json::to_string(&RuntimeErrorCode::ConfigurationUnavailable).unwrap(),
            "\"configuration-unavailable\""
        );
    }

    #[test]
    fn capabilities_and_location_are_separate_fields() {
        let openclaw = list_runtimes()
            .into_iter()
            .find(|runtime| runtime.id == "openclaw")
            .unwrap();
        let json = serde_json::to_value(openclaw).unwrap();

        assert_eq!(json["location"], "hybrid");
        assert!(json["capabilities"].is_array());
        assert!(!json["capabilities"]
            .as_array()
            .unwrap()
            .contains(&serde_json::Value::String("local".to_string())));
    }

    #[test]
    fn normalized_error_serializes_safely() {
        let error = NormalizedRuntimeError {
            code: RuntimeErrorCode::ProbeFailed,
            message: "Runtime probe failed.".to_string(),
            retryable: true,
        };
        let json = serde_json::to_value(error).unwrap();

        assert_eq!(json["code"], "probe-failed");
        assert_eq!(json["retryable"], true);
        assert!(json.get("details").is_none());
    }

    #[test]
    fn registry_does_not_encode_location_as_capability() {
        for runtime in list_runtimes() {
            assert!(matches!(
                runtime.location,
                RuntimeLocation::Local | RuntimeLocation::Remote | RuntimeLocation::Hybrid
            ));
            assert!(runtime.capabilities.iter().all(|capability| matches!(
                capability,
                RuntimeCapability::Discover
                    | RuntimeCapability::Health
                    | RuntimeCapability::Start
                    | RuntimeCapability::Stop
                    | RuntimeCapability::Restart
                    | RuntimeCapability::Open
                    | RuntimeCapability::Progress
                    | RuntimeCapability::Cancel
            )));
        }
    }
}
