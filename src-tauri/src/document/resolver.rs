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
    /// Not an application: the file itself, read as the ZIP of XML it is.
    StructuredFile,
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

/// The candidates a real machine offers right now.
///
/// Two fields here mean different things and the distinction is the whole
/// point. `available` is whether the application is installed. `executable` is
/// whether THIS BUILD has an adapter for that application and capability -- and
/// four adapters were found written, proven and reachable from no production
/// path at all before this list existed. Nothing appears here that a call path
/// cannot actually run.
///
/// The format lists are what decides fidelity, not the priority order. macOS's
/// `textutil` reads a .docx by converting it to plain text, which is an import,
/// not a native read; declaring it as one is what let the lowest-priority
/// generic path outrank both Word and the structured layer on their own format.
pub(crate) fn office_candidates() -> Vec<OfficeCandidate> {
    const DOCUMENT_READ: &[&str] = &["document.read"];
    const WORD: &[&str] = &[
        "document.read",
        "document.create",
        "document.convert",
        "document.edit",
    ];
    const EXCEL: &[&str] = &["spreadsheet.read", "spreadsheet.create", "spreadsheet.edit"];
    const POWERPOINT: &[&str] = &[
        "presentation.read",
        "presentation.create",
        "presentation.edit",
        "presentation.convert",
    ];
    const PAGES: &[&str] = &["document.read", "document.create", "document.convert"];
    const NUMBERS: &[&str] = &["spreadsheet.read", "spreadsheet.create"];
    const KEYNOTE: &[&str] = &["presentation.read", "presentation.create"];
    const NATIVE: &[&str] = &["document.read", "document.create", "document.convert"];
    const STRUCTURED: &[&str] = &[
        "document.read",
        "presentation.read",
        "spreadsheet.read",
        "spreadsheet.create",
    ];

    let installed = |application_id: &str| crate::connections::local_application_installed(application_id);
    let microsoft = |name: &str| std::path::Path::new(name).exists();

    vec![
        // The file itself. No application, so it is available everywhere, and
        // it is ranked below every application that reads its own format.
        OfficeCandidate {
            provider: OfficeProviderId::LocalStructured,
            application: OfficeApplication::StructuredFile,
            local: true,
            available: true,
            authorized: true,
            executable: true,
            priority: 900,
            capabilities: STRUCTURED,
            native_formats: &["docx", "xlsx", "pptx"],
            import_formats: &[],
        },
        OfficeCandidate {
            provider: OfficeProviderId::MicrosoftOffice,
            application: OfficeApplication::MicrosoftWord,
            local: true,
            available: microsoft("/Applications/Microsoft Word.app"),
            authorized: true,
            executable: true,
            priority: 10,
            capabilities: WORD,
            native_formats: &["doc", "docx"],
            import_formats: &[],
        },
        OfficeCandidate {
            provider: OfficeProviderId::MicrosoftOffice,
            application: OfficeApplication::MicrosoftExcel,
            local: true,
            available: microsoft("/Applications/Microsoft Excel.app"),
            authorized: true,
            executable: true,
            priority: 10,
            capabilities: EXCEL,
            native_formats: &["xls", "xlsx"],
            import_formats: &[],
        },
        OfficeCandidate {
            provider: OfficeProviderId::MicrosoftOffice,
            application: OfficeApplication::MicrosoftPowerPoint,
            local: true,
            available: microsoft("/Applications/Microsoft PowerPoint.app"),
            authorized: true,
            executable: true,
            priority: 10,
            capabilities: POWERPOINT,
            native_formats: &["ppt", "pptx"],
            import_formats: &[],
        },
        // The iWork adapters each accept only their own format, so neither
        // declares an import route it would refuse.
        OfficeCandidate {
            provider: OfficeProviderId::AppleIwork,
            application: OfficeApplication::ApplePages,
            local: true,
            available: installed("pages"),
            authorized: true,
            executable: true,
            priority: 20,
            capabilities: PAGES,
            native_formats: &["pages"],
            import_formats: &[],
        },
        OfficeCandidate {
            provider: OfficeProviderId::AppleIwork,
            application: OfficeApplication::AppleNumbers,
            local: true,
            available: installed("numbers"),
            authorized: true,
            executable: true,
            priority: 20,
            capabilities: NUMBERS,
            native_formats: &["numbers"],
            import_formats: &[],
        },
        OfficeCandidate {
            provider: OfficeProviderId::AppleIwork,
            application: OfficeApplication::AppleKeynote,
            local: true,
            available: installed("keynote"),
            authorized: true,
            executable: true,
            priority: 20,
            capabilities: KEYNOTE,
            native_formats: &["key"],
            import_formats: &[],
        },
        // The floor for the Microsoft word-processing formats: it needs no
        // application, and it is the only path that reads the old binary .doc
        // when Word is absent. Everything it reads, it reads by converting to
        // plain text, so all of its formats are imports.
        OfficeCandidate {
            provider: OfficeProviderId::MacosNative,
            application: OfficeApplication::MacosNative,
            local: true,
            available: cfg!(target_os = "macos"),
            authorized: true,
            executable: true,
            priority: 0,
            capabilities: NATIVE,
            native_formats: &[],
            import_formats: &["doc", "docx", "rtf", "txt"],
        },
        // WPS is installed on some machines and reads all three Microsoft
        // formats, but macOS publishes no deterministic automation contract for
        // it, so there is no adapter to route to. Its files are supported
        // through the structured layer instead; that is a different statement
        // from the application being supported, and this says the true one.
        OfficeCandidate {
            provider: OfficeProviderId::WpsOffice,
            application: OfficeApplication::WpsWriter,
            local: true,
            available: microsoft("/Applications/wpsoffice.app")
                || microsoft("/Applications/WPS Office.app"),
            authorized: true,
            executable: false,
            priority: 30,
            capabilities: DOCUMENT_READ,
            native_formats: &["doc", "docx"],
            import_formats: &[],
        },
    ]
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

    /// The production candidate list with a chosen set of applications
    /// installed, so a routing claim can be made about a machine other than the
    /// one running the test.
    fn machine_with(installed: &[OfficeApplication]) -> Vec<OfficeCandidate> {
        office_candidates()
            .into_iter()
            .map(|mut candidate| {
                candidate.available = candidate.application
                    == OfficeApplication::StructuredFile
                    || installed.contains(&candidate.application);
                candidate
            })
            .collect()
    }

    fn route_on(
        candidates: &[OfficeCandidate],
        capability: &str,
        format: &str,
    ) -> Option<OfficeApplication> {
        resolve_office_route(
            &OfficeRouteRequest {
                capability,
                location: OfficeResourceLocation::Local,
                format: Some(format),
                preferred_application: None,
            },
            candidates,
        )
        .map(|route| route.application)
    }

    /// The Provider Matrix, traced rather than declared.
    ///
    /// Every row here is a capability and a file the product actually accepts,
    /// and the answer is the adapter a call reaches -- not what a registry
    /// entry claims. Four adapters were found written, proven and reachable
    /// from nowhere before this existed, so a table built from declarations is
    /// exactly the thing that must not be trusted.
    ///
    /// `document.create` and `document.convert` are deliberately absent, and
    /// their absence is the honest statement: those two entry points still
    /// route by extension rather than through this resolver, because the
    /// providers do not agree on what the capability means. Pages converts to
    /// PDF, macOS conversion converts between DOC and DOCX, and Word exports
    /// PDF; deciding which one `document.convert` is belongs to the product,
    /// not to routing. A row must not be added here until the call path
    /// actually reaches it.
    #[test]
    fn every_capability_and_format_reaches_the_adapter_it_should() {
        let everything = machine_with(&[
            OfficeApplication::MicrosoftWord,
            OfficeApplication::MicrosoftExcel,
            OfficeApplication::MicrosoftPowerPoint,
            OfficeApplication::ApplePages,
            OfficeApplication::AppleNumbers,
            OfficeApplication::AppleKeynote,
            OfficeApplication::MacosNative,
        ]);

        for (capability, format, expected) in [
            // An application always answers for its own format.
            ("document.read", "docx", OfficeApplication::MicrosoftWord),
            ("document.read", "doc", OfficeApplication::MicrosoftWord),
            ("document.read", "pages", OfficeApplication::ApplePages),
            ("spreadsheet.read", "xlsx", OfficeApplication::MicrosoftExcel),
            ("spreadsheet.read", "xls", OfficeApplication::MicrosoftExcel),
            (
                "spreadsheet.read",
                "numbers",
                OfficeApplication::AppleNumbers,
            ),
            (
                "spreadsheet.create",
                "xlsx",
                OfficeApplication::MicrosoftExcel,
            ),
            ("spreadsheet.edit", "xlsx", OfficeApplication::MicrosoftExcel),
            (
                "presentation.read",
                "pptx",
                OfficeApplication::MicrosoftPowerPoint,
            ),
            ("presentation.read", "key", OfficeApplication::AppleKeynote),
            (
                "presentation.create",
                "pptx",
                OfficeApplication::MicrosoftPowerPoint,
            ),
            (
                "presentation.edit",
                "pptx",
                OfficeApplication::MicrosoftPowerPoint,
            ),
            (
                "presentation.convert",
                "pptx",
                OfficeApplication::MicrosoftPowerPoint,
            ),
            ("document.edit", "docx", OfficeApplication::MicrosoftWord),
            // Plain-text conversion is the floor, so it answers only for the
            // formats no application here claims.
            ("document.read", "rtf", OfficeApplication::MacosNative),
        ] {
            assert_eq!(
                route_on(&everything, capability, format),
                Some(expected),
                "{capability} on a .{format} reached the wrong adapter"
            );
        }
    }

    /// The same table on a machine with no Office application at all.
    ///
    /// This is what "a capability, not a set of per-application integrations"
    /// has to mean: the same request still reaches an adapter, and for the
    /// Microsoft formats it reaches one that returns the same shape the
    /// application would have.
    #[test]
    fn the_capability_survives_a_machine_with_no_office_application() {
        let bare = machine_with(&[OfficeApplication::MacosNative]);

        for (capability, format, expected) in [
            ("document.read", "docx", Some(OfficeApplication::StructuredFile)),
            ("spreadsheet.read", "xlsx", Some(OfficeApplication::StructuredFile)),
            (
                "spreadsheet.create",
                "xlsx",
                Some(OfficeApplication::StructuredFile),
            ),
            (
                "presentation.read",
                "pptx",
                Some(OfficeApplication::StructuredFile),
            ),
            // The old binary format is not a ZIP of XML, so conversion is the
            // only thing left that reads it.
            ("document.read", "doc", Some(OfficeApplication::MacosNative)),
            // And what genuinely cannot be done says so, rather than routing to
            // an adapter that would refuse the file.
            ("spreadsheet.edit", "xlsx", None),
            // Editing and PDF export need the application; the structured layer
            // reads and writes files, it does not drive a word processor.
            ("document.edit", "docx", None),
            ("presentation.edit", "pptx", None),
            ("presentation.convert", "pptx", None),
            ("presentation.read", "key", None),
            ("document.read", "pages", None),
            ("spreadsheet.read", "numbers", None),
        ] {
            assert_eq!(
                route_on(&bare, capability, format),
                expected,
                "{capability} on a .{format} routed wrongly with no application installed"
            );
        }
    }

    /// WPS installed changes nothing, and that is the honest outcome.
    ///
    /// WPS reads all three Microsoft formats, but macOS publishes no
    /// deterministic automation contract for it, so there is no adapter to
    /// route to. Its files are still supported -- through the structured layer,
    /// because a WPS .docx is a .docx. A provider having no adapter and a
    /// format being unsupported are different statements.
    #[test]
    fn wps_being_installed_does_not_invent_an_adapter_for_it() {
        let with_wps = machine_with(&[OfficeApplication::WpsWriter]);

        assert_eq!(
            route_on(&with_wps, "document.read", "docx"),
            Some(OfficeApplication::StructuredFile)
        );
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
