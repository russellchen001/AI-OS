mod mounted_share;

use crate::runtime::skill_invocation::{
    SkillBackend, SkillInvocationContext, SkillInvocationError, SkillInvocationErrorKind,
    SkillInvocationRequest, SkillInvocationResult,
};
use mounted_share::{MountedNetworkShareProvider, NetworkStorageProvider};
use serde_json::{json, Value};
use std::sync::Arc;

pub(crate) const CAPABILITIES: &[&str] = &[
    "nas.discover",
    "nas.list",
    "nas.status",
    "nas.resolve",
    "nas.capacity",
];

pub(crate) struct NasSkillBackend {
    provider: Arc<dyn NetworkStorageProvider>,
}

impl NasSkillBackend {
    pub(crate) fn production() -> Self {
        Self {
            provider: Arc::new(MountedNetworkShareProvider),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_provider(provider: Arc<dyn NetworkStorageProvider>) -> Self {
        Self { provider }
    }

    #[cfg(test)]
    pub(crate) fn with_test_target(mount_point: &str) -> Self {
        Self::with_provider(Arc::new(TestNetworkStorageProvider {
            targets: vec![mounted_share::NetworkStorageTarget {
                id: "fixture-target".to_owned(),
                provider: "fixture-network-storage".to_owned(),
                protocol: "smb".to_owned(),
                display_name: "Fixture Share".to_owned(),
                mount_point: mount_point.to_owned(),
                writable: true,
                capacity_bytes: Some(100),
                used_bytes: Some(40),
                available_bytes: Some(60),
            }],
        }))
    }

    fn invoke_capability(
        &self,
        capability: &str,
        input: &Value,
    ) -> Result<Value, SkillInvocationError> {
        match capability {
            "nas.discover" | "nas.list" => {
                let targets = self.provider.discover().map_err(provider_error)?;
                Ok(json!({
                    "provider": self.provider.id(),
                    "targets": targets,
                }))
            }

            "nas.status" => {
                let targets = self.provider.discover().map_err(provider_error)?;

                if let Some(selector) = selector_from_input(input) {
                    let target = resolve_target(&targets, selector)?;
                    Ok(json!({
                        "provider": self.provider.id(),
                        "connected": true,
                        "target": target,
                    }))
                } else {
                    Ok(json!({
                        "provider": self.provider.id(),
                        "connected": !targets.is_empty(),
                        "count": targets.len(),
                        "targets": targets,
                    }))
                }
            }

            "nas.resolve" => {
                let targets = self.provider.discover().map_err(provider_error)?;

                let target = match selector_from_input(input) {
                    Some(selector) => resolve_target(&targets, selector)?,
                    None if targets.len() == 1 => targets.into_iter().next().unwrap(),
                    None if targets.is_empty() => {
                        return Err(SkillInvocationError::new(
                            SkillInvocationErrorKind::BackendUnavailable,
                            "No mounted network storage target is currently available.",
                            true,
                        ));
                    }
                    None => {
                        return Err(SkillInvocationError::new(
                            SkillInvocationErrorKind::InvalidRequest,
                            "Multiple network storage targets are available; provide targetId, mountPoint, displayName, or protocol.",
                            false,
                        ));
                    }
                };

                Ok(json!({
                    "provider": self.provider.id(),
                    "target": target,
                    "filesystemCapabilities": [
                        "filesystem.scan",
                        "filesystem.read",
                        "filesystem.write",
                        "filesystem.move"
                    ]
                }))
            }

            "nas.capacity" => {
                let targets = self.provider.discover().map_err(provider_error)?;

                let target = match selector_from_input(input) {
                    Some(selector) => resolve_target(&targets, selector)?,
                    None if targets.len() == 1 => targets.into_iter().next().unwrap(),
                    None if targets.is_empty() => {
                        return Err(SkillInvocationError::new(
                            SkillInvocationErrorKind::BackendUnavailable,
                            "No mounted network storage target is currently available.",
                            true,
                        ));
                    }
                    None => {
                        return Err(SkillInvocationError::new(
                            SkillInvocationErrorKind::InvalidRequest,
                            "Multiple network storage targets are available; provide targetId, mountPoint, displayName, or protocol.",
                            false,
                        ));
                    }
                };

                Ok(json!({
                    "provider": self.provider.id(),
                    "targetId": target.id,
                    "displayName": target.display_name,
                    "mountPoint": target.mount_point,
                    "protocol": target.protocol,
                    "capacityBytes": target.capacity_bytes,
                    "usedBytes": target.used_bytes,
                    "availableBytes": target.available_bytes,
                }))
            }

            _ => Err(SkillInvocationError::new(
                SkillInvocationErrorKind::InvalidRequest,
                "NAS backend does not support the requested capability.",
                false,
            )),
        }
    }
}

#[cfg(test)]
struct TestNetworkStorageProvider {
    targets: Vec<mounted_share::NetworkStorageTarget>,
}

#[cfg(test)]
impl NetworkStorageProvider for TestNetworkStorageProvider {
    fn id(&self) -> &'static str {
        "fixture-network-storage"
    }

    fn discover(&self) -> Result<Vec<mounted_share::NetworkStorageTarget>, String> {
        Ok(self.targets.clone())
    }
}

impl SkillBackend for NasSkillBackend {
    fn invoke(
        &self,
        _context: &SkillInvocationContext,
        request: &SkillInvocationRequest,
    ) -> Result<SkillInvocationResult, SkillInvocationError> {
        if !CAPABILITIES.contains(&request.capability.as_str()) {
            return Err(SkillInvocationError::new(
                SkillInvocationErrorKind::InvalidRequest,
                "NAS backend received a capability outside the NAS contract.",
                false,
            ));
        }

        let output = self.invoke_capability(&request.capability, &request.input)?;

        Ok(SkillInvocationResult {
            invocation_id: request.invocation_id.clone(),
            capability: request.capability.clone(),
            backend: "nas".to_owned(),
            provider: Some(self.provider.id().to_owned()),
            output,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NasSelector<'a> {
    TargetId(&'a str),
    MountPoint(&'a str),
    DisplayName(&'a str),
    Protocol(&'a str),
}

fn selector_from_input(input: &Value) -> Option<NasSelector<'_>> {
    let value = |key| {
        input
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };

    value("targetId")
        .map(NasSelector::TargetId)
        .or_else(|| value("mountPoint").map(NasSelector::MountPoint))
        .or_else(|| value("displayName").map(NasSelector::DisplayName))
        .or_else(|| value("protocol").map(NasSelector::Protocol))
}

fn resolve_target(
    targets: &[mounted_share::NetworkStorageTarget],
    selector: NasSelector<'_>,
) -> Result<mounted_share::NetworkStorageTarget, SkillInvocationError> {
    let mut matches = targets
        .iter()
        .filter(|target| match selector {
            NasSelector::TargetId(value) => target.id == value,
            NasSelector::MountPoint(value) => target.mount_point == value,
            NasSelector::DisplayName(value) => target.display_name.eq_ignore_ascii_case(value),
            NasSelector::Protocol(value) => target.protocol.eq_ignore_ascii_case(value),
        })
        .cloned();

    let Some(first) = matches.next() else {
        return Err(SkillInvocationError::new(
            SkillInvocationErrorKind::BackendUnavailable,
            "Requested network storage target is not currently mounted.",
            true,
        ));
    };

    if matches.next().is_some() {
        return Err(SkillInvocationError::new(
            SkillInvocationErrorKind::InvalidRequest,
            "The NAS selector matches more than one mounted network storage target.",
            false,
        ));
    }

    Ok(first)
}

fn provider_error(message: String) -> SkillInvocationError {
    SkillInvocationError::new(SkillInvocationErrorKind::BackendUnavailable, message, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::skill_invocation::SkillInvocationContext;

    fn target(id: &str, display_name: &str, protocol: &str) -> mounted_share::NetworkStorageTarget {
        mounted_share::NetworkStorageTarget {
            id: id.to_owned(),
            provider: "fixture".to_owned(),
            protocol: protocol.to_owned(),
            display_name: display_name.to_owned(),
            mount_point: format!("/Volumes/{id}"),
            writable: true,
            capacity_bytes: Some(100),
            used_bytes: Some(40),
            available_bytes: Some(60),
        }
    }

    fn context(capability: &str) -> SkillInvocationContext {
        SkillInvocationContext::new(
            "task-nas",
            "plan-nas",
            "agent-execution-nas",
            "openclaw",
            vec![capability.to_owned()],
            true,
        )
        .unwrap()
    }

    #[test]
    fn nas_contract_is_discovery_and_resolution_not_file_crud() {
        assert_eq!(
            CAPABILITIES,
            &[
                "nas.discover",
                "nas.list",
                "nas.status",
                "nas.resolve",
                "nas.capacity"
            ]
        );

        for forbidden in [
            "nas.read",
            "nas.write",
            "nas.copy",
            "nas.move",
            "nas.delete",
            "nas.mkdir",
        ] {
            assert!(!CAPABILITIES.contains(&forbidden));
        }
    }

    #[test]
    fn backend_rejects_non_nas_capability() {
        let backend = NasSkillBackend::with_provider(Arc::new(MountedNetworkShareProvider));
        let request =
            SkillInvocationRequest::new("invocation-nas", "filesystem.read", json!({})).unwrap();

        let error = backend
            .invoke(&context("filesystem.read"), &request)
            .unwrap_err();

        assert_eq!(error.kind, SkillInvocationErrorKind::InvalidRequest);
    }

    #[test]
    fn selector_kind_is_preserved_during_resolution() {
        let targets = vec![
            target("target-1", "Shared", "smb"),
            target("Shared", "Archive", "nfs"),
        ];

        assert_eq!(
            resolve_target(&targets, NasSelector::TargetId("Shared"))
                .unwrap()
                .display_name,
            "Archive"
        );
        assert_eq!(
            resolve_target(&targets, NasSelector::DisplayName("Shared"))
                .unwrap()
                .id,
            "target-1"
        );
    }

    #[test]
    fn protocol_selector_rejects_ambiguous_targets() {
        let targets = vec![
            target("target-1", "One", "smb"),
            target("target-2", "Two", "smb"),
        ];

        let error = resolve_target(&targets, NasSelector::Protocol("smb")).unwrap_err();
        assert_eq!(error.kind, SkillInvocationErrorKind::InvalidRequest);
    }

    #[test]
    fn list_and_status_have_explicit_normalized_contracts() {
        let backend = NasSkillBackend::with_test_target("/Volumes/Fixture");

        let listed = backend
            .invoke(
                &context("nas.list"),
                &SkillInvocationRequest::new("list-1", "nas.list", json!({})).unwrap(),
            )
            .unwrap();
        assert_eq!(listed.output["provider"], "fixture-network-storage");
        assert_eq!(
            listed.output["targets"][0]["mountPoint"],
            "/Volumes/Fixture"
        );

        let status = backend
            .invoke(
                &context("nas.status"),
                &SkillInvocationRequest::new(
                    "status-1",
                    "nas.status",
                    json!({"targetId": "fixture-target"}),
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(status.output["connected"], true);
        assert_eq!(status.output["target"]["id"], "fixture-target");
    }
}
