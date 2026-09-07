use super::domain::{
    LawfulContentCompatibility, MediaCapability, MediaError, MediaProgress, MediaProviderSource,
    MediaRequest, MediaResult,
};
use crate::provider_selection::{
    AuthorizationKind, AuthorizationRef, AuthorizationState, ProviderInterfaceKind,
};
use serde::{Deserialize, Serialize};

/// Stable local-environment state from the GM-0 Ready First contract.
///
/// `available` and `readiness` answer different questions:
///
/// - available: this provider adapter can participate on this build/environment
/// - readiness: the user's local generation environment is actually usable
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum LocalMediaReadiness {
    NotInstalled,
    InstalledNotConfigured,
    InstalledBroken,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalMediaReadinessEvidence {
    pub engine_installed: bool,
    pub engine_startable: bool,
    pub api_reachable: bool,
    pub workflow_ready: bool,
    pub required_assets_ready: bool,
    pub custom_nodes_ready: bool,
    pub integrity_ok: bool,
    pub smoke_generation_ok: bool,
    pub output_retrieval_ok: bool,
}

impl LocalMediaReadinessEvidence {
    pub(crate) fn classify(&self) -> LocalMediaReadiness {
        if !self.engine_installed {
            return LocalMediaReadiness::NotInstalled;
        }

        if !self.engine_startable || !self.api_reachable {
            return LocalMediaReadiness::InstalledBroken;
        }

        if !self.workflow_ready || !self.required_assets_ready || !self.custom_nodes_ready {
            return LocalMediaReadiness::InstalledNotConfigured;
        }

        if !self.integrity_ok || !self.smoke_generation_ok || !self.output_retrieval_ok {
            return LocalMediaReadiness::InstalledBroken;
        }

        LocalMediaReadiness::Ready
    }
}

/// Provider discovery and routing metadata.
///
/// Secrets never belong here. Cloud authorization is represented only through
/// the existing opaque authorization identity/state contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaProviderMetadata {
    pub provider_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_instance_id: Option<String>,

    pub source: MediaProviderSource,

    pub interface_kind: ProviderInterfaceKind,

    pub authorization_kind: AuthorizationKind,

    pub authorization_state: AuthorizationState,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_ref: Option<AuthorizationRef>,

    pub available: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_readiness: Option<LocalMediaReadiness>,

    pub priority: u16,

    #[serde(default)]
    pub capabilities: Vec<MediaCapability>,

    pub lawful_content_compatibility: LawfulContentCompatibility,

    /// Whether this provider is currently recommended for the generic
    /// user-facing "Use Cloud" choice.
    ///
    /// This metadata is never changed because another provider rejected a
    /// request on policy grounds.
    pub recommended_cloud: bool,

    pub supports_progress: bool,

    pub supports_cancellation: bool,

    pub supports_cost_estimation: bool,
}

impl MediaProviderMetadata {
    pub(crate) fn supports(&self, capability: MediaCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    pub(crate) fn is_authorized(&self) -> bool {
        self.authorization_state == AuthorizationState::Connected
    }

    pub(crate) fn is_local_ready(&self) -> bool {
        self.source == MediaProviderSource::Local
            && self.local_readiness == Some(LocalMediaReadiness::Ready)
    }
}

/// Replaceable media execution adapter.
///
/// GM-1 establishes the interface only. Real ComfyUI and cloud adapters are
/// admitted in later GM phases.
pub(crate) trait MediaProvider: Send + Sync {
    fn metadata(&self) -> &MediaProviderMetadata;

    fn execute(
        &self,
        request: &MediaRequest,
        report: &mut dyn FnMut(MediaProgress),
    ) -> Result<MediaResult, MediaError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_metadata(readiness: LocalMediaReadiness) -> MediaProviderMetadata {
        MediaProviderMetadata {
            provider_id: "local-fixture".to_owned(),
            provider_instance_id: None,
            source: MediaProviderSource::Local,
            interface_kind: ProviderInterfaceKind::NativeStructured,
            authorization_kind: AuthorizationKind::None,
            authorization_state: AuthorizationState::Connected,
            authorization_ref: None,
            available: true,
            local_readiness: Some(readiness),
            priority: 10,
            capabilities: vec![MediaCapability::TextToImage],
            lawful_content_compatibility: LawfulContentCompatibility::Unknown,
            recommended_cloud: false,
            supports_progress: true,
            supports_cancellation: true,
            supports_cost_estimation: false,
        }
    }

    #[test]
    fn available_is_not_the_same_as_ready() {
        let metadata = local_metadata(LocalMediaReadiness::InstalledNotConfigured);

        assert!(metadata.available);
        assert!(!metadata.is_local_ready());
    }

    #[test]
    fn ready_local_provider_is_recognized_explicitly() {
        let metadata = local_metadata(LocalMediaReadiness::Ready);

        assert!(metadata.is_local_ready());
        assert!(metadata.is_authorized());
        assert!(metadata.supports(MediaCapability::TextToImage));
        assert!(!metadata.supports(MediaCapability::TextToVideo));
    }

    #[test]
    fn metadata_serialization_contains_no_provider_secret_material() {
        let mut metadata = local_metadata(LocalMediaReadiness::Ready);

        metadata.provider_id = "cloud-fixture".to_owned();
        metadata.provider_instance_id = Some("cloud-primary".to_owned());
        metadata.source = MediaProviderSource::Cloud;
        metadata.interface_kind = ProviderInterfaceKind::OfficialApi;
        metadata.authorization_kind = AuthorizationKind::ApiKey;
        metadata.authorization_ref =
            Some(AuthorizationRef::opaque("provider-instance:cloud-primary"));
        metadata.local_readiness = None;
        metadata.lawful_content_compatibility = LawfulContentCompatibility::Broad;
        metadata.recommended_cloud = true;
        metadata.supports_cost_estimation = true;

        let serialized = serde_json::to_string(&metadata).unwrap();
        let value = serde_json::to_value(&metadata).unwrap();

        assert_eq!(value["providerId"], "cloud-fixture");
        assert_eq!(value["providerInstanceId"], "cloud-primary");
        assert_eq!(value["source"], "cloud");
        assert_eq!(value["recommendedCloud"], true);

        for forbidden in [
            "apiKey",
            "accessToken",
            "refreshToken",
            "password",
            "clientSecret",
        ] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn all_gm0_stable_readiness_states_serialize_canonically() {
        assert_eq!(
            serde_json::to_string(&LocalMediaReadiness::NotInstalled).unwrap(),
            "\"not-installed\""
        );
        assert_eq!(
            serde_json::to_string(&LocalMediaReadiness::InstalledNotConfigured).unwrap(),
            "\"installed-not-configured\""
        );
        assert_eq!(
            serde_json::to_string(&LocalMediaReadiness::InstalledBroken).unwrap(),
            "\"installed-broken\""
        );
        assert_eq!(
            serde_json::to_string(&LocalMediaReadiness::Ready).unwrap(),
            "\"ready\""
        );
    }

    #[test]
    fn readiness_evidence_classifies_all_stable_states() {
        let ready = LocalMediaReadinessEvidence {
            engine_installed: true,
            engine_startable: true,
            api_reachable: true,
            workflow_ready: true,
            required_assets_ready: true,
            custom_nodes_ready: true,
            integrity_ok: true,
            smoke_generation_ok: true,
            output_retrieval_ok: true,
        };

        assert_eq!(ready.classify(), LocalMediaReadiness::Ready);

        let mut not_installed = ready.clone();
        not_installed.engine_installed = false;
        assert_eq!(not_installed.classify(), LocalMediaReadiness::NotInstalled);

        let mut not_configured = ready.clone();
        not_configured.workflow_ready = false;
        assert_eq!(
            not_configured.classify(),
            LocalMediaReadiness::InstalledNotConfigured
        );

        let mut broken = ready;
        broken.smoke_generation_ok = false;
        assert_eq!(broken.classify(), LocalMediaReadiness::InstalledBroken);
    }
}
