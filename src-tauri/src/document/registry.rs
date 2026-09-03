use super::provider::{OfficeProvider, OfficeProviderId};

/// What Apple iWork can actually execute.
///
/// `spreadsheet.edit` is deliberately absent: Numbers has a read and a create
/// adapter but no edit one. Declaring a capability the provider cannot execute
/// is what made the Provider Matrix overstate iWork for as long as it did, and
/// a registry declaration is not executable evidence.
const IWORK_CAPABILITIES: &[&str] = &[
    "document.read",
    "document.create",
    "document.convert",
    "spreadsheet.read",
    "spreadsheet.create",
    "spreadsheet.convert",
    "presentation.read",
    "presentation.create",
    "presentation.convert",
];

/// What Microsoft Office can actually execute.
///
/// It is the widest of these lists because it is the only provider with an
/// editing adapter for all three document kinds. `document.edit`,
/// `presentation.edit` and `presentation.convert` were implemented and verified
/// long before any capability declared them, so nothing could call them.
const MICROSOFT_OFFICE_CAPABILITIES: &[&str] = &[
    "document.read",
    "document.create",
    "document.convert",
    "document.edit",
    "spreadsheet.read",
    "spreadsheet.create",
    "spreadsheet.edit",
    "spreadsheet.convert",
    "presentation.read",
    "presentation.create",
    "presentation.edit",
    "presentation.convert",
];

/// What Google Workspace can actually execute.
///
/// Its Docs, Sheets and Slides adapters are real and are proven by their own
/// E2Es, so this is not the WPS case. It is the reachability case: they are
/// exposed as commands the frontend calls directly, and the provider-neutral
/// Office capabilities are local-only, so `document.read` cannot reach them.
/// That is the fifth instance of the same pattern in this work and it is
/// recorded rather than papered over -- routing a cloud resource needs a
/// request shape (a Drive file id is not a path) and a way to call an async
/// adapter from the synchronous gateway, which are decisions, not omissions.
///
/// It declares neither conversion nor editing, which it does not implement.
const GOOGLE_WORKSPACE_CAPABILITIES: &[&str] = &[
    "document.read",
    "document.create",
    "spreadsheet.read",
    "spreadsheet.create",
    "spreadsheet.edit",
    "presentation.read",
    "presentation.create",
];

/// What WPS Office can actually execute: nothing.
///
/// It reads and writes all three Microsoft formats, and it is detected when
/// installed, but macOS publishes no deterministic automation contract for it
/// and AirScript is a cloud API, so there is no local adapter to route to.
/// Declaring the eight capabilities it cannot execute is the same mistake the
/// iWork and LocalStructured entries used to make. WPS *files* are supported
/// through the structured layer, which is a different statement and a true one.
const WPS_CAPABILITIES: &[&str] = &[];

const NATIVE_CAPABILITIES: &[&str] = &["document.read", "document.create", "document.convert"];
const GRAPH_CAPABILITIES: &[&str] = &[
    "document.read",
    "document.create",
    "spreadsheet.read",
    "spreadsheet.create",
    "spreadsheet.update",
];
/// What the structured file layer can actually execute today.
///
/// This provider reads the file itself -- .xlsx, .docx and .pptx are all ZIP
/// archives of XML -- so it needs no application and is available on every
/// machine. That is what makes Office a capability rather than a set of
/// per-application integrations: when no Office application is installed, this
/// still answers.
///
/// It declares only what is implemented. It previously declared four
/// capabilities with no implementation at all.
const LOCAL_STRUCTURED_CAPABILITIES: &[&str] = &[
    "document.read",
    "presentation.read",
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
            // Lowest preference of the local providers: an installed
            // application reads its own format with higher fidelity. This is
            // the floor that keeps the capability from disappearing when none
            // of them is installed.
            priority: 900,
            capabilities: LOCAL_STRUCTURED_CAPABILITIES,
            available: true,
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
            capabilities: MICROSOFT_OFFICE_CAPABILITIES,
            available: app_exists("/Applications/Microsoft Word.app")
                || app_exists("/Applications/Microsoft Excel.app")
                || app_exists("/Applications/Microsoft PowerPoint.app"),
        },
        OfficeProvider {
            id: OfficeProviderId::AppleIwork,
            name: "Apple iWork",
            local: true,
            priority: 20,
            capabilities: IWORK_CAPABILITIES,
            available: crate::connections::local_application_installed("pages")
                || crate::connections::local_application_installed("numbers")
                || crate::connections::local_application_installed("keynote"),
        },
        OfficeProvider {
            id: OfficeProviderId::WpsOffice,
            name: "WPS Office",
            local: true,
            priority: 30,
            capabilities: WPS_CAPABILITIES,
            available: app_exists("/Applications/wpsoffice.app")
                || app_exists("/Applications/WPS Office.app"),
        },
        OfficeProvider {
            id: OfficeProviderId::GoogleWorkspace,
            name: "Google Workspace",
            local: false,
            priority: 100,
            capabilities: GOOGLE_WORKSPACE_CAPABILITIES,
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

        let structured = &providers[1];

        // The structured layer reads the file itself, so it is available on
        // every machine -- this is the floor that keeps a spreadsheet capability
        // from disappearing when no spreadsheet application is installed.
        assert!(structured.available);
        assert!(structured.local);
        assert!(structured.supports("spreadsheet.read"));
        assert!(structured.supports("spreadsheet.create"));
        assert!(structured.supports("document.read"));
        assert!(structured.supports("presentation.read"));

        // It must never outrank an installed application, which reads its own
        // format with higher fidelity.
        for application in [
            OfficeProviderId::MicrosoftOffice,
            OfficeProviderId::AppleIwork,
            OfficeProviderId::MacosNative,
        ] {
            let candidate = providers
                .iter()
                .find(|provider| provider.id == application)
                .unwrap();
            assert!(
                candidate.priority < structured.priority,
                "{application:?} must be preferred over the structured layer"
            );
        }

        // And it declares nothing it cannot execute. Writing a document or a
        // presentation without the application, and editing a workbook in
        // place, are not implemented -- so they are not claimed.
        for unimplemented in [
            "document.create",
            "document.convert",
            "presentation.create",
            "spreadsheet.edit",
        ] {
            assert!(
                !structured.supports(unimplemented),
                "the structured layer does not implement {unimplemented} yet"
            );
        }
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

        // And it declares nothing it cannot execute: there is no Google
        // conversion or document-editing adapter.
        for unimplemented in ["document.convert", "document.edit", "presentation.edit"] {
            assert!(
                !google.supports(unimplemented),
                "Google Workspace does not implement {unimplemented}"
            );
        }
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
