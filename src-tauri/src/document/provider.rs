use crate::provider_selection::{
    AuthorizationKind, AuthorizationState, ProviderInterfaceKind, ProviderSelectionMetadata,
    ResourceLocation, SessionOwnership,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum OfficeProviderId {
    MicrosoftGraph,
    LocalStructured,
    MacosNative,
    MicrosoftOffice,
    AppleIwork,
    WpsOffice,
    GoogleWorkspace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OfficeProvider {
    pub id: OfficeProviderId,
    pub name: &'static str,
    pub local: bool,
    pub priority: u16,
    pub capabilities: &'static [&'static str],
    pub available: bool,
}

impl OfficeProvider {
    pub(crate) fn supports(&self, capability: &str) -> bool {
        self.capabilities.contains(&capability)
    }

    pub(crate) fn selection_metadata(&self) -> ProviderSelectionMetadata {
        let (interface_kind, authorization_kind, authorization_state, location, ownership) =
            match self.id {
                OfficeProviderId::MicrosoftGraph => (
                    ProviderInterfaceKind::OfficialApi,
                    AuthorizationKind::OAuth,
                    AuthorizationState::AuthorizationRequired,
                    ResourceLocation::Cloud,
                    None,
                ),
                OfficeProviderId::LocalStructured | OfficeProviderId::MacosNative => (
                    ProviderInterfaceKind::NativeStructured,
                    AuthorizationKind::SystemPermission,
                    AuthorizationState::Connected,
                    ResourceLocation::Local,
                    None,
                ),
                OfficeProviderId::MicrosoftOffice | OfficeProviderId::AppleIwork => (
                    ProviderInterfaceKind::DeterministicAutomation,
                    AuthorizationKind::SystemPermission,
                    AuthorizationState::Connected,
                    ResourceLocation::Local,
                    Some(SessionOwnership::ExternallyOwned),
                ),
                _ => (
                    ProviderInterfaceKind::AuthenticatedSession,
                    AuthorizationKind::AuthenticatedSession,
                    AuthorizationState::AuthorizationRequired,
                    ResourceLocation::Cloud,
                    Some(SessionOwnership::ExternallyOwned),
                ),
            };
        ProviderSelectionMetadata {
            provider_id: format!("{:?}", self.id),
            interface_kind,
            authorization_kind,
            authorization_state,
            authorization_ref: None,
            available: self.available,
            capabilities: self
                .capabilities
                .iter()
                .map(|item| (*item).to_owned())
                .collect(),
            resource_location: location,
            session_ownership: ownership,
        }
    }
}
