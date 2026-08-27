use super::provider::{OfficeProvider, OfficeProviderId};

const ALL_OFFICE_CAPABILITIES: &[&str] = &[
    "document.read",
    "document.create",
    "document.convert",
    "spreadsheet.read",
    "spreadsheet.create",
    "presentation.read",
    "presentation.create",
];

const NATIVE_CAPABILITIES: &[&str] = &["document.read", "document.create", "document.convert"];

fn app_exists(path: &str) -> bool {
    std::path::Path::new(path).exists()
}

fn browser_provider_available() -> bool {
    crate::runtime::skills::resolver::resolve("browser.control").is_some()
}

pub(crate) fn office_providers() -> Vec<OfficeProvider> {
    vec![
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
            available: app_exists("/Applications/Pages.app")
                || app_exists("/Applications/Numbers.app")
                || app_exists("/Applications/Keynote.app"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_order_and_local_first_policy_are_stable() {
        let providers = office_providers();

        assert_eq!(providers.len(), 5);
        assert_eq!(providers[0].id, OfficeProviderId::MacosNative);
        assert_eq!(providers[0].priority, 0);
        assert_eq!(providers[1].id, OfficeProviderId::MicrosoftOffice);
        assert_eq!(providers[1].priority, 10);
        assert_eq!(providers[2].id, OfficeProviderId::AppleIwork);
        assert_eq!(providers[2].priority, 20);
        assert_eq!(providers[3].id, OfficeProviderId::WpsOffice);
        assert_eq!(providers[3].priority, 30);
        assert_eq!(providers[4].id, OfficeProviderId::GoogleWorkspace);
        assert_eq!(providers[4].priority, 100);
    }

    #[test]
    fn native_and_google_provider_boundaries_are_correct() {
        let providers = office_providers();
        let native = &providers[0];
        let google = &providers[4];

        assert!(native.local);
        assert!(native.supports("document.read"));
        assert!(!native.supports("spreadsheet.read"));

        assert!(!google.local);
        assert!(google.supports("document.read"));
        assert!(google.supports("spreadsheet.read"));
        assert!(google.supports("presentation.create"));
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
