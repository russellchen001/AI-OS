use super::provider::{OfficeProvider, OfficeProviderId};

const ALL_OFFICE_CAPABILITIES: &[&str] = &[
    "document.read",
    "document.create",
    "document.convert",
    "spreadsheet.read",
    "spreadsheet.create",
    "spreadsheet.edit",
    "presentation.read",
    "presentation.create",
];

const NATIVE_CAPABILITIES: &[&str] = &["document.read", "document.create", "document.convert"];
const GRAPH_CAPABILITIES: &[&str] = &[
    "document.read",
    "document.create",
    "spreadsheet.read",
    "spreadsheet.create",
    "spreadsheet.update",
];
const LOCAL_STRUCTURED_CAPABILITIES: &[&str] = &[
    "document.read",
    "document.create",
    "spreadsheet.read",
    "spreadsheet.create",
];

fn app_exists(path: &str) -> bool {
    std::path::Path::new(path).exists()
}

fn browser_provider_available() -> bool {
    crate::runtime::skills::resolver::resolve("browser.control").is_some()
}

pub(crate) fn office_providers() -> Vec<OfficeProvider> {
    vec![
        OfficeProvider {
            id: OfficeProviderId::MicrosoftGraph,
            name: "Microsoft Graph",
            local: false,
            priority: 0,
            capabilities: GRAPH_CAPABILITIES,
            available: false,
        },
        OfficeProvider {
            id: OfficeProviderId::LocalStructured,
            name: "Local Structured File",
            local: true,
            priority: 0,
            capabilities: LOCAL_STRUCTURED_CAPABILITIES,
            available: false,
        },
        OfficeProvider {
            id: OfficeProviderId::MacosNative,
            name: "macOS Native",
            local: true,
            priority: 0,
            capabilities: NATIVE_CAPABILITIES,
            available: cfg!(target_os = "macos"),
        },
        OfficeProvider {
            id: OfficeProviderId::MicrosoftOffice,
            name: "Microsoft Office",
            local: true,
            priority: 10,
            capabilities: ALL_OFFICE_CAPABILITIES,
            available: app_exists("/Applications/Microsoft Word.app")
                || app_exists("/Applications/Microsoft Excel.app")
                || app_exists("/Applications/Microsoft PowerPoint.app"),
        },
        OfficeProvider {
            id: OfficeProviderId::AppleIwork,
            name: "Apple iWork",
            local: true,
            priority: 20,
            capabilities: ALL_OFFICE_CAPABILITIES,
            available: crate::connections::local_application_installed("pages")
                || crate::connections::local_application_installed("numbers")
                || crate::connections::local_application_installed("keynote"),
        },
        OfficeProvider {
            id: OfficeProviderId::WpsOffice,
            name: "WPS Office",
            local: true,
            priority: 30,
            capabilities: ALL_OFFICE_CAPABILITIES,
            available: app_exists("/Applications/wpsoffice.app")
                || app_exists("/Applications/WPS Office.app"),
        },
        OfficeProvider {
            id: OfficeProviderId::GoogleWorkspace,
            name: "Google Workspace",
            local: false,
            priority: 100,
            capabilities: ALL_OFFICE_CAPABILITIES,
            available: browser_provider_available(),
        },
    ]
}

pub(crate) fn resolve_office_provider(capability: &str) -> Option<OfficeProvider> {
    let capability = capability.trim();

    office_providers()
        .into_iter()
        .filter(|provider| provider.available && provider.supports(capability))
        .min_by_key(|provider| provider.priority)
}

pub(crate) fn resolve_local_presentation_provider(capability: &str) -> Option<OfficeProvider> {
    if !matches!(capability, "presentation.read" | "presentation.create") {
        return None;
    }

    office_providers().into_iter().find(|provider| {
        provider.id == OfficeProviderId::AppleIwork
            && provider.available
            && provider.supports(capability)
            && crate::connections::local_application_installed("keynote")
    })
}

pub(crate) fn resolve_cloud_presentation_provider(capability: &str) -> Option<OfficeProvider> {
    if !matches!(capability, "presentation.read" | "presentation.create") {
        return None;
    }

    office_providers().into_iter().find(|provider| {
        provider.id == OfficeProviderId::GoogleWorkspace
            && provider.available
            && provider.supports(capability)
            && !provider.local
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_selection::{
        AuthorizationKind, AuthorizationState, ProviderInterfaceKind, SessionOwnership,
    };

    #[test]
    fn provider_order_and_local_first_policy_are_stable() {
        let providers = office_providers();

        assert_eq!(providers.len(), 7);
        assert_eq!(providers[0].id, OfficeProviderId::MicrosoftGraph);
        assert_eq!(providers[1].id, OfficeProviderId::LocalStructured);
        assert!(!providers[0].available);
        assert!(!providers[1].available);
    }

    #[test]
    fn native_and_google_provider_boundaries_are_correct() {
        let providers = office_providers();
        let native = providers
            .iter()
            .find(|provider| provider.id == OfficeProviderId::MacosNative)
            .unwrap();
        let google = providers
            .iter()
            .find(|provider| provider.id == OfficeProviderId::GoogleWorkspace)
            .unwrap();

        assert!(native.local);
        assert!(native.supports("document.read"));
        assert!(!native.supports("spreadsheet.read"));

        assert!(!google.local);
        assert!(google.supports("document.read"));
        assert!(google.supports("spreadsheet.read"));
        assert!(google.supports("presentation.create"));
    }

    #[test]
    fn graph_foundation_and_native_excel_ownership_are_explicit() {
        let providers = office_providers();
        let graph = providers[0].selection_metadata();
        let excel = providers
            .iter()
            .find(|provider| provider.id == OfficeProviderId::MicrosoftOffice)
            .unwrap()
            .selection_metadata();

        assert_eq!(graph.interface_kind, ProviderInterfaceKind::OfficialApi);
        assert_eq!(graph.authorization_kind, AuthorizationKind::OAuth);
        assert_eq!(
            graph.authorization_state,
            AuthorizationState::AuthorizationRequired
        );
        assert_eq!(
            excel.session_ownership,
            Some(SessionOwnership::ExternallyOwned)
        );
    }

    #[test]
    fn apple_iwork_metadata_is_local_external_deterministic_automation() {
        let metadata = office_providers()
            .into_iter()
            .find(|provider| provider.id == OfficeProviderId::AppleIwork)
            .unwrap()
            .selection_metadata();

        assert_eq!(
            metadata.interface_kind,
            ProviderInterfaceKind::DeterministicAutomation
        );
        assert_eq!(
            metadata.authorization_kind,
            AuthorizationKind::SystemPermission
        );
        assert_eq!(metadata.authorization_state, AuthorizationState::Connected);
        assert_eq!(
            metadata.resource_location,
            crate::provider_selection::ResourceLocation::Local
        );
        assert_eq!(
            metadata.session_ownership,
            Some(SessionOwnership::ExternallyOwned)
        );
    }

    #[test]
    fn presentation_resolvers_preserve_local_and_cloud_boundaries() {
        let providers = office_providers();
        let apple = providers
            .iter()
            .find(|provider| provider.id == OfficeProviderId::AppleIwork)
            .unwrap();
        let google = providers
            .iter()
            .find(|provider| provider.id == OfficeProviderId::GoogleWorkspace)
            .unwrap();

        assert!(apple.local);
        assert!(!google.local);

        if let Some(local) = resolve_local_presentation_provider("presentation.read") {
            assert!(local.local);
            assert_eq!(local.id, OfficeProviderId::AppleIwork);
        }
        if let Some(cloud) = resolve_cloud_presentation_provider("presentation.create") {
            assert!(!cloud.local);
            assert_eq!(cloud.id, OfficeProviderId::GoogleWorkspace);
        }

        assert!(resolve_local_presentation_provider("document.read").is_none());
        assert!(resolve_cloud_presentation_provider("spreadsheet.read").is_none());
        assert!(resolve_local_presentation_provider("unknown.capability").is_none());
        assert!(resolve_cloud_presentation_provider("unknown.capability").is_none());
    }

    #[test]
    fn unknown_office_capability_is_not_fabricated() {
        assert!(resolve_office_provider("unknown.office.capability").is_none());
        assert!(resolve_office_provider("   ").is_none());
    }

    #[test]
    fn spreadsheet_read_uses_first_available_office_provider() {
        let providers = office_providers();
        let expected = providers
            .iter()
            .filter(|provider| provider.available && provider.supports("spreadsheet.read"))
            .min_by_key(|provider| provider.priority)
            .expect("spreadsheet.read provider");
        let resolved =
            resolve_office_provider("spreadsheet.read").expect("spreadsheet.read should resolve");

        assert_ne!(resolved.id, OfficeProviderId::MacosNative);
        assert_eq!(resolved.id, expected.id);
        assert_eq!(resolved.priority, expected.priority);
        assert!(resolved.supports("spreadsheet.read"));
    }

    #[test]
    fn spreadsheet_create_uses_first_available_office_provider() {
        let providers = office_providers();
        let expected = providers
            .iter()
            .filter(|provider| provider.available && provider.supports("spreadsheet.create"))
            .min_by_key(|provider| provider.priority)
            .expect("spreadsheet.create provider");
        let resolved = resolve_office_provider("spreadsheet.create")
            .expect("spreadsheet.create should resolve");

        assert_ne!(resolved.id, OfficeProviderId::MacosNative);
        assert_eq!(resolved.id, expected.id);
        assert_eq!(resolved.priority, expected.priority);
        assert!(resolved.supports("spreadsheet.create"));
    }

    #[test]
    fn spreadsheet_edit_uses_first_available_office_provider() {
        let resolved =
            resolve_office_provider("spreadsheet.edit").expect("spreadsheet.edit should resolve");

        assert_eq!(resolved.id, OfficeProviderId::MicrosoftOffice);
        assert!(resolved.local);
        assert!(resolved.supports("spreadsheet.edit"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn document_convert_resolves_to_macos_native_first() {
        let provider =
            resolve_office_provider("document.convert").expect("document.convert provider");

        assert_eq!(provider.id, OfficeProviderId::MacosNative);
        assert!(provider.local);
        assert_eq!(provider.priority, 0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn document_create_resolves_to_macos_native_first() {
        let provider =
            resolve_office_provider("document.create").expect("document.create provider");

        assert_eq!(provider.id, OfficeProviderId::MacosNative);
        assert!(provider.local);
        assert_eq!(provider.priority, 0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn document_read_resolves_to_macos_native_first() {
        let provider = resolve_office_provider("document.read").expect("document.read provider");

        assert_eq!(provider.id, OfficeProviderId::MacosNative);
        assert!(provider.local);
        assert_eq!(provider.priority, 0);
    }
}
