use crate::planner::{EvidenceState, StepInput};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum ProviderInterfaceKind {
    OfficialApi,
    NativeStructured,
    AuthenticatedSession,
    DeterministicAutomation,
    ComputerUse,
}

impl ProviderInterfaceKind {
    fn priority(self) -> u8 {
        match self {
            Self::OfficialApi => 0,
            Self::NativeStructured => 1,
            Self::AuthenticatedSession => 2,
            Self::DeterministicAutomation => 3,
            Self::ComputerUse => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum AuthorizationKind {
    None,
    #[serde(rename = "OAUTH")]
    OAuth,
    ApiKey,
    SystemPermission,
    AuthenticatedSession,
    UserConfirmation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum AuthorizationState {
    Disconnected,
    AuthorizationRequired,
    Authorizing,
    Connected,
    Expired,
    Error,
}

impl AuthorizationState {
    fn is_authorized(self) -> bool {
        matches!(self, Self::Connected)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct AuthorizationRef(String);

impl AuthorizationRef {
    pub(crate) fn opaque(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum ResourceLocation {
    Local,
    Cloud,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum SessionOwnership {
    ExternallyOwned,
    ProviderOwned,
}

pub(crate) fn provider_may_close_session(
    ownership: SessionOwnership,
    session_was_running: bool,
) -> bool {
    ownership == SessionOwnership::ProviderOwned && !session_was_running
}

pub(crate) fn provider_evidence_state(
    authorization_kind: AuthorizationKind,
    authorization_state: AuthorizationState,
    executable: bool,
    completed: bool,
) -> EvidenceState {
    if completed {
        EvidenceState::Completed
    } else if executable {
        EvidenceState::Executable
    } else if authorization_state.is_authorized()
        && matches!(
            authorization_kind,
            AuthorizationKind::OAuth | AuthorizationKind::AuthenticatedSession
        )
    {
        EvidenceState::Authenticated
    } else {
        EvidenceState::Verified
    }
}

pub(crate) fn provider_step_input(
    provider_id: impl Into<String>,
    authorization_ref: AuthorizationRef,
    resource_location: ResourceLocation,
) -> StepInput {
    [
        (
            "providerId".to_owned(),
            serde_json::json!(provider_id.into()),
        ),
        (
            "authorizationRef".to_owned(),
            serde_json::to_value(authorization_ref).expect("authorization ref serializes"),
        ),
        (
            "resourceLocation".to_owned(),
            serde_json::to_value(resource_location).expect("resource location serializes"),
        ),
    ]
    .into_iter()
    .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProviderSelectionMetadata {
    pub provider_id: String,
    pub interface_kind: ProviderInterfaceKind,
    pub authorization_kind: AuthorizationKind,
    pub authorization_state: AuthorizationState,
    pub authorization_ref: Option<AuthorizationRef>,
    pub available: bool,
    pub capabilities: Vec<String>,
    pub resource_location: ResourceLocation,
    pub session_ownership: Option<SessionOwnership>,
}

impl ProviderSelectionMetadata {
    fn supports(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|item| item == capability)
    }

    fn resource_compatible(&self, location: ResourceLocation) -> bool {
        matches!(location, ResourceLocation::Any)
            || matches!(self.resource_location, ResourceLocation::Any)
            || self.resource_location == location
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProviderSelectionRequest<'a> {
    pub capability: &'a str,
    pub resource_location: ResourceLocation,
    pub preferred_provider: Option<&'a str>,
    pub allow_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProviderSelectionError {
    NoCompatibleProvider,
    AuthorizationRequired { provider_id: String },
}

pub(crate) fn select_provider(
    providers: &[ProviderSelectionMetadata],
    request: ProviderSelectionRequest<'_>,
) -> Result<ProviderSelectionMetadata, ProviderSelectionError> {
    let mut candidates = providers
        .iter()
        .filter(|provider| {
            provider.available
                && provider.supports(request.capability)
                && provider.resource_compatible(request.resource_location)
        })
        .cloned()
        .collect::<Vec<_>>();

    candidates.sort_by_key(|provider| {
        let explicitly_preferred = request
            .preferred_provider
            .is_some_and(|preferred| preferred == provider.provider_id);
        (!explicitly_preferred, provider.interface_kind.priority())
    });

    let first = candidates
        .first()
        .cloned()
        .ok_or(ProviderSelectionError::NoCompatibleProvider)?;
    if first.authorization_state.is_authorized() {
        return Ok(first);
    }
    if !request.allow_fallback {
        return Err(ProviderSelectionError::AuthorizationRequired {
            provider_id: first.provider_id,
        });
    }
    candidates
        .into_iter()
        .skip(1)
        .find(|provider| provider.authorization_state.is_authorized())
        .ok_or(ProviderSelectionError::AuthorizationRequired {
            provider_id: first.provider_id,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(
        id: &str,
        interface_kind: ProviderInterfaceKind,
        state: AuthorizationState,
        location: ResourceLocation,
    ) -> ProviderSelectionMetadata {
        ProviderSelectionMetadata {
            provider_id: id.to_owned(),
            interface_kind,
            authorization_kind: AuthorizationKind::OAuth,
            authorization_state: state,
            authorization_ref: Some(AuthorizationRef::opaque(format!("auth:{id}"))),
            available: true,
            capabilities: vec!["spreadsheet.read".to_owned()],
            resource_location: location,
            session_ownership: None,
        }
    }

    #[test]
    fn provider_contract_serializes_without_secret_fields() {
        let metadata = provider(
            "microsoft-graph",
            ProviderInterfaceKind::OfficialApi,
            AuthorizationState::Connected,
            ResourceLocation::Cloud,
        );
        let value = serde_json::to_value(metadata).unwrap();

        assert_eq!(value["interfaceKind"], "OFFICIAL_API");
        assert_eq!(value["authorizationKind"], "OAUTH");
        assert!(value.get("accessToken").is_none());
        assert!(value.get("refreshToken").is_none());
        assert!(value.get("password").is_none());
        assert!(value.get("cookie").is_none());
    }

    #[test]
    fn authorization_kinds_and_existing_evidence_states_remain_distinct() {
        assert_ne!(
            AuthorizationKind::OAuth,
            AuthorizationKind::SystemPermission
        );
        assert_ne!(
            AuthorizationKind::OAuth,
            AuthorizationKind::UserConfirmation
        );
        assert_eq!(
            provider_evidence_state(
                AuthorizationKind::OAuth,
                AuthorizationState::Connected,
                false,
                false
            ),
            EvidenceState::Authenticated
        );
        assert_eq!(
            provider_evidence_state(
                AuthorizationKind::OAuth,
                AuthorizationState::Connected,
                false,
                true
            ),
            EvidenceState::Completed
        );
    }

    #[test]
    fn planner_provider_input_contains_only_opaque_authorization_reference() {
        let input = provider_step_input(
            "microsoft-graph",
            AuthorizationRef::opaque("keychain:graph:primary"),
            ResourceLocation::Cloud,
        );
        let value = serde_json::to_value(input).unwrap();

        assert_eq!(value["providerId"], "microsoft-graph");
        assert_eq!(value["authorizationRef"], "keychain:graph:primary");
        assert!(value.get("accessToken").is_none());
        assert!(value.get("refreshToken").is_none());
        assert!(value.get("password").is_none());
        assert!(value.get("cookie").is_none());
    }

    #[test]
    fn selection_honors_interface_priority_locality_and_authorization_policy() {
        let graph = provider(
            "microsoft-graph",
            ProviderInterfaceKind::OfficialApi,
            AuthorizationState::AuthorizationRequired,
            ResourceLocation::Cloud,
        );
        let local = provider(
            "local-structured",
            ProviderInterfaceKind::NativeStructured,
            AuthorizationState::Connected,
            ResourceLocation::Local,
        );
        let browser = provider(
            "authenticated-browser",
            ProviderInterfaceKind::AuthenticatedSession,
            AuthorizationState::Connected,
            ResourceLocation::Cloud,
        );
        let providers = vec![browser.clone(), local.clone(), graph.clone()];

        let local_result = select_provider(
            &providers,
            ProviderSelectionRequest {
                capability: "spreadsheet.read",
                resource_location: ResourceLocation::Local,
                preferred_provider: None,
                allow_fallback: false,
            },
        )
        .unwrap();
        assert_eq!(local_result.provider_id, "local-structured");

        let cloud_request = ProviderSelectionRequest {
            capability: "spreadsheet.read",
            resource_location: ResourceLocation::Cloud,
            preferred_provider: None,
            allow_fallback: false,
        };
        assert_eq!(
            select_provider(&providers, cloud_request),
            Err(ProviderSelectionError::AuthorizationRequired {
                provider_id: "microsoft-graph".to_owned()
            })
        );

        assert_eq!(
            select_provider(
                &providers,
                ProviderSelectionRequest {
                    allow_fallback: true,
                    ..cloud_request
                }
            )
            .unwrap()
            .provider_id,
            browser.provider_id
        );
    }

    #[test]
    fn external_native_application_sessions_are_never_closed_by_provider() {
        assert!(!provider_may_close_session(
            SessionOwnership::ExternallyOwned,
            true
        ));
        assert!(!provider_may_close_session(
            SessionOwnership::ExternallyOwned,
            false
        ));
        assert!(provider_may_close_session(
            SessionOwnership::ProviderOwned,
            false
        ));
    }
}
