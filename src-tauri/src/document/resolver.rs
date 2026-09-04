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
    /// The format being written, for a conversion.
    ///
    /// Conversion is the one capability that cannot be routed on the source
    /// alone: `.docx` to `.pdf` is Word's job and `.docx` to `.pages` is
    /// Pages', because Word cannot write a `.pages` at all. Left as None for
    /// every other capability.
    pub destination_format: Option<&'a str>,
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
    /// Formats it can WRITE besides its own.
    ///
    /// Reading a format and writing it are different abilities and conversion
    /// needs both asked separately. macOS conversion reads and writes DOC and
    /// DOCX; Word reads both but, through this adapter, writes only PDF -- so
    /// DOC to DOCX must not route to Word even though Word reads DOC natively.
    pub export_formats: &'static [&'static str],
}

impl OfficeCandidate {
    fn supports(&self, capability: &str) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Whether this ADAPTER can produce this format.
    ///
    /// Only `export_formats` is consulted, and the distinction is not
    /// pedantic: Word writes a .docx perfectly well, but its adapter here only
    /// exports PDF, so routing DOC to DOCX to Word on the strength of Word
    /// natively writing DOCX sends the request to code that cannot do it. What
    /// the application could do and what this build can ask it to do are
    /// different lists, and this is the second one.
    ///
    /// Empty for a candidate that converts nothing, and irrelevant to every
    /// capability that is not a conversion, which passes None.
    fn can_write(&self, format: Option<&str>) -> bool {
        let Some(format) = format.map(normalize_format) else {
            return true;
        };

        self.export_formats
            .iter()
            .any(|candidate| *candidate == format)
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
                && candidate.can_write(request.destination_format)
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
fn wps_installed() -> bool {
    ["/Applications/wpsoffice.app", "/Applications/WPS Office.app"]
        .iter()
        .any(|path| std::path::Path::new(path).exists())
}

/// Whether the Google account is set up, from configuration rather than from
/// the Keychain.
///
/// A routing decision must not perform a privileged read. Asking the Keychain
/// answered the same question and made every rebuilt test binary raise a macOS
/// authorization panel, which is a bad trade for a fact that configuration
/// already records. An expired or refresh-needed account still counts as a
/// candidate: the request is the only thing that can find out, and its error
/// says exactly what to do.
fn google_workspace_connected() -> bool {
    crate::providers::provider_instance_connected(crate::google_workspace::office::INSTANCE)
}

pub(crate) fn office_candidates() -> Vec<OfficeCandidate> {
    const DOCUMENT_READ: &[&str] = &["document.read"];
    const WORD: &[&str] = &[
        "document.read",
        "document.create",
        "document.convert",
        "document.edit",
    ];
    const EXCEL: &[&str] = &[
        "spreadsheet.read",
        "spreadsheet.create",
        "spreadsheet.edit",
        "spreadsheet.convert",
    ];
    const POWERPOINT: &[&str] = &[
        "presentation.read",
        "presentation.create",
        "presentation.edit",
        "presentation.convert",
    ];
    const PAGES: &[&str] = &["document.read", "document.create", "document.convert"];
    const NUMBERS_CAPS: &[&str] = &[
        "spreadsheet.read",
        "spreadsheet.create",
        "spreadsheet.convert",
    ];
    const KEYNOTE_CAPS: &[&str] = &[
        "presentation.read",
        "presentation.create",
        "presentation.convert",
    ];
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
            // It reads and writes files, but it does not convert between
            // formats, so it never wins a conversion.
            export_formats: &[],
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
            // Word's adapter exports PDF. It does NOT convert DOC to DOCX, so
            // it must not claim to write them for a conversion even though it
            // reads both natively.
            export_formats: &["pdf"],
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
            export_formats: &["pdf"],
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
            export_formats: &["pdf"],
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
            // Reading a .docx is what makes .docx -> .pages possible, and
            // nothing else on the machine can write a .pages.
            import_formats: &["docx"],
            export_formats: &["pages", "docx", "pdf"],
        },
        OfficeCandidate {
            provider: OfficeProviderId::AppleIwork,
            application: OfficeApplication::AppleNumbers,
            local: true,
            available: installed("numbers"),
            authorized: true,
            executable: true,
            priority: 20,
            capabilities: NUMBERS_CAPS,
            native_formats: &["numbers"],
            import_formats: &["xlsx"],
            export_formats: &["numbers", "xlsx", "pdf"],
        },
        OfficeCandidate {
            provider: OfficeProviderId::AppleIwork,
            application: OfficeApplication::AppleKeynote,
            local: true,
            available: installed("keynote"),
            authorized: true,
            executable: true,
            priority: 20,
            capabilities: KEYNOTE_CAPS,
            native_formats: &["key"],
            import_formats: &["pptx"],
            export_formats: &["key", "pptx", "pdf"],
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
            // It converts among the formats it reads, which is what makes it
            // the only route for DOC to DOCX. It cannot write a PDF.
            export_formats: &["doc", "docx", "rtf", "txt"],
        },
        // Google Workspace. Docs, Sheets and Slides have real adapters proven by
        // their own E2Es; what they lacked was any route from a
        // provider-neutral capability, which is why they are here now. They are
        // the only candidates that are not local, so a cloud request can reach
        // nothing else and a local path can never reach them.
        //
        // `available` is whether the account is connected, answered from the
        // stored credential rather than by asking for a token, because a
        // routing decision should not perform a network refresh.
        OfficeCandidate {
            provider: OfficeProviderId::GoogleWorkspace,
            application: OfficeApplication::GoogleDocs,
            local: false,
            available: google_workspace_connected(),
            authorized: true,
            executable: true,
            priority: 100,
            capabilities: &["document.read", "document.create"],
            // A cloud resource is named by id and has no extension, so format
            // never decides a cloud route.
            native_formats: &[],
            import_formats: &[],
            export_formats: &[],
        },
        OfficeCandidate {
            provider: OfficeProviderId::GoogleWorkspace,
            application: OfficeApplication::GoogleSheets,
            local: false,
            available: google_workspace_connected(),
            authorized: true,
            executable: true,
            priority: 100,
            capabilities: &[
                "spreadsheet.read",
                "spreadsheet.create",
                "spreadsheet.edit",
            ],
            native_formats: &[],
            import_formats: &[],
            export_formats: &[],
        },
        OfficeCandidate {
            provider: OfficeProviderId::GoogleWorkspace,
            application: OfficeApplication::GoogleSlides,
            local: false,
            available: google_workspace_connected(),
            authorized: true,
            executable: true,
            priority: 100,
            capabilities: &["presentation.read", "presentation.create"],
            native_formats: &[],
            import_formats: &[],
            export_formats: &[],
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
            available: wps_installed(),
            authorized: true,
            executable: false,
            priority: 30,
            capabilities: DOCUMENT_READ,
            native_formats: &["doc", "docx"],
            import_formats: &[],
            export_formats: &[],
        },
        OfficeCandidate {
            provider: OfficeProviderId::WpsOffice,
            application: OfficeApplication::WpsSpreadsheet,
            local: true,
            available: wps_installed(),
            authorized: true,
            executable: false,
            priority: 30,
            capabilities: &["spreadsheet.read"],
            native_formats: &["xls", "xlsx"],
            import_formats: &[],
            export_formats: &[],
        },
        OfficeCandidate {
            provider: OfficeProviderId::WpsOffice,
            application: OfficeApplication::WpsPresentation,
            local: true,
            available: wps_installed(),
            authorized: true,
            executable: false,
            priority: 30,
            capabilities: &["presentation.read"],
            native_formats: &["ppt", "pptx"],
            import_formats: &[],
            export_formats: &[],
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
            export_formats: &[],
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
                destination_format: None,
                preferred_application: None,
            },
            candidates,
        )
        .map(|route| route.application)
    }

    fn convert_on(
        candidates: &[OfficeCandidate],
        capability: &str,
        from: &str,
        to: &str,
    ) -> Option<OfficeApplication> {
        resolve_office_route(
            &OfficeRouteRequest {
                capability,
                location: OfficeResourceLocation::Local,
                format: Some(from),
                destination_format: Some(to),
                preferred_application: None,
            },
            candidates,
        )
        .map(|route| route.application)
    }

    /// The conversion table, which the destination decides.
    ///
    /// The owner settled that conversion is two things -- cross-suite format
    /// conversion and PDF export -- and that both must work. Every row here is
    /// a pair a caller can actually ask for, and the reason this is a table
    /// rather than a rule is that the answer changes with the destination and
    /// not only with the source: the same .docx goes to Word to become a PDF
    /// and to Pages to become a .pages, because Word cannot write a .pages and
    /// macOS conversion cannot write a PDF.
    #[test]
    fn conversion_routes_on_the_destination_and_not_only_the_source() {
        let everything = machine_with(&[
            OfficeApplication::MicrosoftWord,
            OfficeApplication::MicrosoftExcel,
            OfficeApplication::MicrosoftPowerPoint,
            OfficeApplication::ApplePages,
            OfficeApplication::AppleNumbers,
            OfficeApplication::AppleKeynote,
            OfficeApplication::MacosNative,
        ]);

        for (capability, from, to, expected) in [
            // PDF export goes to whichever application owns the source format.
            ("document.convert", "docx", "pdf", OfficeApplication::MicrosoftWord),
            ("document.convert", "pages", "pdf", OfficeApplication::ApplePages),
            ("spreadsheet.convert", "xlsx", "pdf", OfficeApplication::MicrosoftExcel),
            (
                "spreadsheet.convert",
                "numbers",
                "pdf",
                OfficeApplication::AppleNumbers,
            ),
            (
                "presentation.convert",
                "pptx",
                "pdf",
                OfficeApplication::MicrosoftPowerPoint,
            ),
            ("presentation.convert", "key", "pdf", OfficeApplication::AppleKeynote),
            // Cross-suite conversion goes to iWork in BOTH directions, because
            // no Microsoft application reads or writes an iWork format.
            ("document.convert", "docx", "pages", OfficeApplication::ApplePages),
            ("document.convert", "pages", "docx", OfficeApplication::ApplePages),
            (
                "spreadsheet.convert",
                "xlsx",
                "numbers",
                OfficeApplication::AppleNumbers,
            ),
            (
                "spreadsheet.convert",
                "numbers",
                "xlsx",
                OfficeApplication::AppleNumbers,
            ),
            ("presentation.convert", "pptx", "key", OfficeApplication::AppleKeynote),
            ("presentation.convert", "key", "pptx", OfficeApplication::AppleKeynote),
            // And conversion between the Microsoft word-processing formats
            // needs no application. Word reads both natively but its adapter
            // only exports PDF, so it must not win this row -- which is the
            // whole reason a candidate declares what it can WRITE separately
            // from what it can read.
            ("document.convert", "doc", "docx", OfficeApplication::MacosNative),
            ("document.convert", "docx", "doc", OfficeApplication::MacosNative),
            ("document.convert", "rtf", "docx", OfficeApplication::MacosNative),
        ] {
            assert_eq!(
                convert_on(&everything, capability, from, to),
                Some(expected),
                "{capability} from .{from} to .{to} reached the wrong adapter"
            );
        }

        // A pair nothing on the machine can do says so, rather than routing to
        // an application that would refuse the file.
        for (capability, from, to) in [
            ("document.convert", "pages", "key"),
            ("spreadsheet.convert", "numbers", "docx"),
            ("presentation.convert", "key", "xlsx"),
            ("document.convert", "docx", "epub"),
        ] {
            assert_eq!(
                convert_on(&everything, capability, from, to),
                None,
                "{capability} from .{from} to .{to} should have no route"
            );
        }
    }

    /// Conversion on a machine with no Office application at all.
    ///
    /// The structured layer reads and writes files but converts nothing, so
    /// this is where the capability legitimately runs out -- except between the
    /// Microsoft word-processing formats, which macOS itself converts.
    #[test]
    fn conversion_without_any_office_application_says_what_it_cannot_do() {
        let bare = machine_with(&[OfficeApplication::MacosNative]);

        assert_eq!(
            convert_on(&bare, "document.convert", "doc", "docx"),
            Some(OfficeApplication::MacosNative)
        );

        for (capability, from, to) in [
            ("document.convert", "docx", "pdf"),
            ("document.convert", "docx", "pages"),
            ("spreadsheet.convert", "xlsx", "pdf"),
            ("spreadsheet.convert", "xlsx", "numbers"),
            ("presentation.convert", "pptx", "pdf"),
            ("presentation.convert", "pptx", "key"),
        ] {
            assert_eq!(
                convert_on(&bare, capability, from, to),
                None,
                "{capability} from .{from} to .{to} should have no route with no application"
            );
        }
    }

    /// The Provider Matrix, traced rather than declared.
    ///
    /// Every row here is a capability and a file the product actually accepts,
    /// and the answer is the adapter a call reaches -- not what a registry
    /// entry claims. Four adapters were found written, proven and reachable
    /// from nowhere before this existed, so a table built from declarations is
    /// exactly the thing that must not be trusted.
    ///
    /// This comment used to say `document.create` and `document.convert` were
    /// deliberately absent because the providers disagreed about what
    /// conversion meant. That was settled -- the destination decides -- and
    /// both entry points route here now. The comment outliving the code is the
    /// exact failure this table exists to prevent, so it is corrected rather
    /// than quietly deleted.
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
            // Creating follows reading: Word writes a real Word document, so it
            // answers for its own format whenever it is installed.
            ("document.create", "docx", OfficeApplication::MicrosoftWord),
            ("document.create", "pages", OfficeApplication::ApplePages),
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
            // Plain-text conversion is the floor for creating too.
            ("document.create", "docx", Some(OfficeApplication::MacosNative)),
            ("document.create", "rtf", Some(OfficeApplication::MacosNative)),
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
    /// The cloud half of the matrix.
    ///
    /// Google's Docs, Sheets and Slides adapters were real and proven and
    /// reachable from no provider-neutral capability -- the fifth adapter in
    /// this work found working and callable from nowhere. These rows are the
    /// statement that they are reachable now, and that the two boundaries hold
    /// in both directions: a cloud resource never reaches a local application,
    /// and a local path never reaches Google.
    #[test]
    fn a_cloud_resource_reaches_google_and_a_local_path_never_does() {
        // Everything installed AND the account connected, which is the case
        // where a mistake would be invisible.
        let everything: Vec<OfficeCandidate> = office_candidates()
            .into_iter()
            .map(|mut candidate| {
                candidate.available = true;
                candidate
            })
            .collect();

        for (capability, expected) in [
            ("document.read", OfficeApplication::GoogleDocs),
            ("document.create", OfficeApplication::GoogleDocs),
            ("spreadsheet.read", OfficeApplication::GoogleSheets),
            ("spreadsheet.create", OfficeApplication::GoogleSheets),
            ("spreadsheet.edit", OfficeApplication::GoogleSheets),
            ("presentation.read", OfficeApplication::GoogleSlides),
            ("presentation.create", OfficeApplication::GoogleSlides),
        ] {
            let route = resolve_office_route(
                &OfficeRouteRequest {
                    capability,
                    location: OfficeResourceLocation::GoogleCloud,
                    // A cloud resource is named by id and has no extension, so
                    // format never decides a cloud route.
                    format: None,
                    destination_format: None,
                    preferred_application: None,
                },
                &everything,
            )
            .unwrap_or_else(|| panic!("{capability} had no cloud route"));

            assert_eq!(route.application, expected, "{capability} routed wrongly");
            assert!(!route.local, "{capability} routed to a local application");
        }

        // What Google has no adapter for says so, rather than falling back to
        // a local application that cannot see the file at all.
        for capability in [
            "document.edit",
            "document.convert",
            "spreadsheet.convert",
            "presentation.edit",
            "presentation.convert",
        ] {
            assert!(
                resolve_office_route(
                    &OfficeRouteRequest {
                        capability,
                        location: OfficeResourceLocation::GoogleCloud,
                        format: None,
                        destination_format: None,
                        preferred_application: None,
                    },
                    &everything,
                )
                .is_none(),
                "{capability} must not claim a cloud route"
            );
        }

        // And with the account not connected there is no cloud route at all,
        // which is what `available` means for a provider that is reached over
        // the network rather than installed.
        let disconnected: Vec<OfficeCandidate> = office_candidates()
            .into_iter()
            .map(|mut candidate| {
                candidate.available = candidate.provider != OfficeProviderId::GoogleWorkspace;
                candidate
            })
            .collect();

        assert!(resolve_office_route(
            &OfficeRouteRequest {
                capability: "document.read",
                location: OfficeResourceLocation::GoogleCloud,
                format: None,
                destination_format: None,
                preferred_application: None,
            },
            &disconnected,
        )
        .is_none());
    }

    #[test]
    fn wps_being_installed_does_not_invent_an_adapter_for_it() {
        let with_wps = machine_with(&[
            OfficeApplication::WpsWriter,
            OfficeApplication::WpsSpreadsheet,
            OfficeApplication::WpsPresentation,
        ]);

        // All three kinds, because WPS reads all three Microsoft formats and
        // has an adapter for none of them. Every one falls to the structured
        // layer, which is the true statement: the FILES are supported, the
        // application is not driven.
        for (capability, format) in [
            ("document.read", "docx"),
            ("spreadsheet.read", "xlsx"),
            ("presentation.read", "pptx"),
        ] {
            assert_eq!(
                route_on(&with_wps, capability, format),
                Some(OfficeApplication::StructuredFile),
                "{capability} on a .{format} should fall to the structured layer"
            );
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
                destination_format: None,
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
                destination_format: None,
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
                destination_format: None,
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
                destination_format: None,
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
                destination_format: None,
                preferred_application: None,
            },
            &[docs],
        )
        .is_none());
    }
}
