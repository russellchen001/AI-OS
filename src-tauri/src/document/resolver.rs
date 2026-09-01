use super::provider::OfficeProviderId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OfficeResourceLocation {
    Local,
    GoogleCloud,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OfficeApplication {
    MacosNative,
    MicrosoftWord,
    MicrosoftExcel,
    MicrosoftPowerPoint,
    ApplePages,
    AppleNumbers,
    AppleKeynote,
    WpsWriter,
    WpsSpreadsheet,
    WpsPresentation,
    GoogleDocs,
    GoogleSheets,
    GoogleSlides,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OfficeRouteRequest<'a> {
    pub capability: &'a str,
    pub location: OfficeResourceLocation,
    pub format: Option<&'a str>,
    pub preferred_application: Option<OfficeApplication>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OfficeRoute {
    pub provider: OfficeProviderId,
    pub application: OfficeApplication,
    pub local: bool,
    pub fidelity_warning: Option<&'static str>,
}

#[derive(Debug, Clone)]
pub(crate) struct OfficeCandidate {
    pub provider: OfficeProviderId,
    pub application: OfficeApplication,
    pub local: bool,
    pub available: bool,
    pub authorized: bool,
    pub executable: bool,
    pub priority: u16,
    pub capabilities: &'static [&'static str],
    pub native_formats: &'static [&'static str],
    pub import_formats: &'static [&'static str],
}

impl OfficeCandidate {
    fn supports(&self, capability: &str) -> bool {
        self.capabilities.contains(&capability)
    }

    fn format_rank(&self, format: Option<&str>) -> Option<(u8, Option<&'static str>)> {
        let Some(format) = format.map(normalize_format) else {
            return Some((2, None));
        };
        if self
            .native_formats
            .iter()
            .any(|candidate| *candidate == format)
        {
            return Some((0, None));
        }
        if self
            .import_formats
            .iter()
            .any(|candidate| *candidate == format)
        {
            return Some((
                1,
                Some("Provider import/export may change document fidelity."),
            ));
        }
        None
    }
}

fn normalize_format(value: &str) -> String {
    value.trim().trim_start_matches('.').to_ascii_lowercase()
}

pub(crate) fn resolve_office_route(
    request: &OfficeRouteRequest<'_>,
    candidates: &[OfficeCandidate],
) -> Option<OfficeRoute> {
    let capability = request.capability.trim();
    if capability.is_empty() {
        return None;
    }

    candidates
        .iter()
        .filter(|candidate| {
            candidate.available
                && candidate.authorized
                && candidate.executable
                && candidate.supports(capability)
                && match request.location {
                    OfficeResourceLocation::Local => candidate.local,
                    OfficeResourceLocation::GoogleCloud => {
                        !candidate.local
                            && matches!(
                                candidate.application,
                                OfficeApplication::GoogleDocs
                                    | OfficeApplication::GoogleSheets
                                    | OfficeApplication::GoogleSlides
                            )
                    }
                }
        })
        .filter_map(|candidate| {
            candidate
                .format_rank(request.format)
                .map(|rank| (candidate, rank))
        })
        .min_by_key(|(candidate, (format_rank, _))| {
            let preference_rank = if request.preferred_application == Some(candidate.application) {
                0
            } else {
                1
            };
            (preference_rank, *format_rank, candidate.priority)
        })
        .map(|(candidate, (_, fidelity_warning))| OfficeRoute {
            provider: candidate.provider,
            application: candidate.application,
            local: candidate.local,
            fidelity_warning,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const READ: &[&str] = &["document.read"];
    const PRESENTATION_COMMON: &[&str] = &[
        "presentation.read",
        "presentation.create",
        "presentation.edit",
        "presentation.export",
    ];

    fn candidate(
        provider: OfficeProviderId,
        application: OfficeApplication,
        priority: u16,
        native_formats: &'static [&'static str],
        import_formats: &'static [&'static str],
    ) -> OfficeCandidate {
        OfficeCandidate {
            provider,
            application,
            local: !matches!(provider, OfficeProviderId::GoogleWorkspace),
            available: true,
            authorized: true,
            executable: true,
            priority,
            capabilities: READ,
            native_formats,
            import_formats,
        }
    }

    #[test]
    fn executable_and_authorized_are_required_not_just_declared_capability() {
        let mut declared_only = candidate(
            OfficeProviderId::MicrosoftOffice,
            OfficeApplication::MicrosoftWord,
            0,
            &["docx"],
            &[],
        );
        declared_only.executable = false;
        let pages = candidate(
            OfficeProviderId::AppleIwork,
            OfficeApplication::ApplePages,
            10,
            &["pages"],
            &["docx"],
        );

        let route = resolve_office_route(
            &OfficeRouteRequest {
                capability: "document.read",
                location: OfficeResourceLocation::Local,
                format: Some("docx"),
                preferred_application: None,
            },
            &[declared_only, pages],
        )
        .unwrap();

        assert_eq!(route.application, OfficeApplication::ApplePages);
        assert!(route.fidelity_warning.is_some());
    }

    #[test]
    fn local_pptx_routes_to_executable_powerpoint_not_declared_only_fallback() {
        let mut powerpoint = candidate(
            OfficeProviderId::MicrosoftOffice,
            OfficeApplication::MicrosoftPowerPoint,
            0,
            &["ppt", "pptx"],
            &[],
        );
        powerpoint.capabilities = PRESENTATION_COMMON;
        let mut declared_only_keynote = candidate(
            OfficeProviderId::AppleIwork,
            OfficeApplication::AppleKeynote,
            10,
            &["key"],
            &["pptx"],
        );
        declared_only_keynote.capabilities = PRESENTATION_COMMON;
        declared_only_keynote.executable = false;

        let route = resolve_office_route(
            &OfficeRouteRequest {
                capability: "presentation.edit",
                location: OfficeResourceLocation::Local,
                format: Some("pptx"),
                preferred_application: None,
            },
            &[declared_only_keynote, powerpoint],
        )
        .unwrap();

        assert_eq!(route.application, OfficeApplication::MicrosoftPowerPoint);
        assert!(route.fidelity_warning.is_none());
    }

    #[test]
    fn native_format_overrides_generic_priority() {
        let word = candidate(
            OfficeProviderId::MicrosoftOffice,
            OfficeApplication::MicrosoftWord,
            0,
            &["doc", "docx"],
            &[],
        );
        let pages = candidate(
            OfficeProviderId::AppleIwork,
            OfficeApplication::ApplePages,
            10,
            &["pages"],
            &["docx"],
        );
        let route = resolve_office_route(
            &OfficeRouteRequest {
                capability: "document.read",
                location: OfficeResourceLocation::Local,
                format: Some("pages"),
                preferred_application: None,
            },
            &[word, pages],
        )
        .unwrap();
        assert_eq!(route.application, OfficeApplication::ApplePages);
    }

    #[test]
    fn cloud_resource_ids_never_route_to_local_apps() {
        let word = candidate(
            OfficeProviderId::MicrosoftOffice,
            OfficeApplication::MicrosoftWord,
            0,
            &["docx"],
            &[],
        );
        let docs = candidate(
            OfficeProviderId::GoogleWorkspace,
            OfficeApplication::GoogleDocs,
            100,
            &[],
            &[],
        );
        let route = resolve_office_route(
            &OfficeRouteRequest {
                capability: "document.read",
                location: OfficeResourceLocation::GoogleCloud,
                format: None,
                preferred_application: None,
            },
            &[word, docs],
        )
        .unwrap();
        assert_eq!(route.application, OfficeApplication::GoogleDocs);
        assert!(!route.local);
    }

    #[test]
    fn local_paths_never_route_to_google() {
        let docs = candidate(
            OfficeProviderId::GoogleWorkspace,
            OfficeApplication::GoogleDocs,
            0,
            &[],
            &["docx"],
        );
        assert!(resolve_office_route(
            &OfficeRouteRequest {
                capability: "document.read",
                location: OfficeResourceLocation::Local,
                format: Some("docx"),
                preferred_application: None,
            },
            &[docs],
        )
        .is_none());
    }
}
