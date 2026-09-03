//! Cross-suite conversion and PDF export through Apple iWork.
//!
//! The owner settled what "convert" means, and it is two things:
//!
//!   1. Cross-suite format conversion, so an iWork application can be handed an
//!      Office file and an Office application an iWork file.
//!   2. PDF export, from any Office or iWork file.
//!
//! The DESTINATION format decides which one a request is, which is what makes a
//! single capability able to mean both without ambiguity. `.pdf` is an export;
//! anything else is a conversion.
//!
//! iWork is where the cross-suite half has to happen: Word cannot write a
//! `.pages`, Excel cannot write a `.numbers`, PowerPoint cannot write a `.key`,
//! but each iWork application both reads and writes its Microsoft counterpart.
//!
//! Everything here was probed against the real applications first
//! (`verify/probe_office_conversion_semantics.sh`) and three findings shape the
//! code:
//!
//!   * A `POSIX file` specifier handed to an iWork application that is not
//!     frontmost is SILENTLY DISCARDED for a foreign format: `open` returns
//!     success and no document ever appears, with no error and no window, for as
//!     long as you care to wait. An alias resolves in the sending process, so
//!     what arrives is a real file reference. Everything here sends an alias.
//!   * Importing a foreign format produces a NEW UNSAVED document, so `open`
//!     returns `missing value` and the document's own `file` is `missing value`
//!     too. It can only be found by which document id appeared -- which is also
//!     what makes it safe to close, because only a document this code caused is
//!     ever touched.
//!   * `export` OVERWRITES an existing destination without complaint. The
//!     no-overwrite rule is therefore enforced here, before any application is
//!     asked to do anything, and never left to the application.

use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IworkApplication {
    Pages,
    Numbers,
    Keynote,
}

impl IworkApplication {
    fn bundle_id(self) -> &'static str {
        match self {
            Self::Pages => "com.apple.Pages",
            Self::Numbers => "com.apple.Numbers",
            Self::Keynote => "com.apple.Keynote",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Pages => "Pages",
            Self::Numbers => "Numbers",
            Self::Keynote => "Keynote",
        }
    }

    /// The extension this application saves as its own document.
    fn native_extension(self) -> &'static str {
        match self {
            Self::Pages => "pages",
            Self::Numbers => "numbers",
            Self::Keynote => "key",
        }
    }

    /// The Microsoft counterpart it can both read and write.
    fn office_extension(self) -> &'static str {
        match self {
            Self::Pages => "docx",
            Self::Numbers => "xlsx",
            Self::Keynote => "pptx",
        }
    }

    /// Extensions it will open. The older binary formats are deliberately not
    /// claimed: they were not probed, and an unprobed format is a guess.
    fn readable(self) -> [&'static str; 2] {
        [self.native_extension(), self.office_extension()]
    }

    fn capability(self) -> &'static str {
        match self {
            Self::Pages => "document.convert",
            Self::Numbers => "spreadsheet.convert",
            Self::Keynote => "presentation.convert",
        }
    }

    fn script(self) -> &'static str {
        match self {
            Self::Pages => PAGES_SCRIPT,
            Self::Numbers => NUMBERS_SCRIPT,
            Self::Keynote => KEYNOTE_SCRIPT,
        }
    }
}

#[derive(Debug)]
pub(crate) struct IworkConvertError {
    pub(crate) invalid_request: bool,
    pub(crate) message: String,
}

impl IworkConvertError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            invalid_request: true,
            message: message.into(),
        }
    }

    fn execution(message: impl Into<String>) -> Self {
        Self {
            invalid_request: false,
            message: message.into(),
        }
    }
}

/// What the destination asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// Export to PDF.
    Pdf,
    /// Write the Microsoft counterpart.
    Office,
    /// Write the application's own format.
    Native,
}

impl Outcome {
    /// The operation code the script branches on.
    fn code(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Office => "office",
            Self::Native => "native",
        }
    }
}

fn extension_of(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

fn require_path<'a>(
    input: &'a Value,
    field: &str,
    capability: &str,
) -> Result<&'a str, IworkConvertError> {
    let path = input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| IworkConvertError::invalid(format!("{capability} requires {field}")))?;

    if !Path::new(path).is_absolute() {
        return Err(IworkConvertError::invalid(format!(
            "{capability} requires an absolute {field}"
        )));
    }

    Ok(path)
}

/// Which conversion this request is, or a refusal saying why it is not one.
fn resolve_outcome(
    application: IworkApplication,
    source: &str,
    destination: &str,
) -> Result<Outcome, IworkConvertError> {
    let capability = application.capability();
    let from = extension_of(source);
    let to = extension_of(destination);

    if !application.readable().contains(&from.as_str()) {
        return Err(IworkConvertError::invalid(format!(
            "{capability} through {} reads .{} and .{}, not .{from}",
            application.name(),
            application.native_extension(),
            application.office_extension()
        )));
    }

    if to == "pdf" {
        return Ok(Outcome::Pdf);
    }

    if to == application.office_extension() {
        return Ok(Outcome::Office);
    }

    if to == application.native_extension() {
        return Ok(Outcome::Native);
    }

    Err(IworkConvertError::invalid(format!(
        "{capability} through {} writes .pdf, .{} and .{}, not .{to}",
        application.name(),
        application.office_extension(),
        application.native_extension()
    )))
}

fn run_osascript(
    application: IworkApplication,
    script: &str,
    args: &[&str],
) -> Result<String, IworkConvertError> {
    let name = application.name();

    let mut child = Command::new("/usr/bin/osascript")
        .arg("-")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| {
            IworkConvertError::execution(format!("Unable to start {name} AppleScript automation"))
        })?;

    {
        let stdin = child.stdin.as_mut().ok_or_else(|| {
            IworkConvertError::execution("Unable to open AppleScript input".to_owned())
        })?;

        stdin.write_all(script.as_bytes()).map_err(|_| {
            IworkConvertError::execution(format!("Unable to write {name} AppleScript"))
        })?;
    }

    let output = child.wait_with_output().map_err(|_| {
        IworkConvertError::execution(format!("{name} AppleScript did not complete"))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();

        return Err(IworkConvertError::execution(if stderr.is_empty() {
            format!("{name} AppleScript automation failed")
        } else {
            format!("{name} AppleScript automation failed: {stderr}")
        }));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub(crate) fn convert_with_iwork(
    application: IworkApplication,
    input: &Value,
) -> Result<Value, IworkConvertError> {
    let capability = application.capability();
    let source = require_path(input, "source", capability)?;
    let destination = require_path(input, "destination", capability)?;

    if source == destination {
        return Err(IworkConvertError::invalid(format!(
            "{capability} source and destination must differ"
        )));
    }

    let outcome = resolve_outcome(application, source, destination)?;

    if !Path::new(source).is_file() {
        return Err(IworkConvertError::invalid(format!(
            "{capability} source does not exist"
        )));
    }

    // `export` overwrites without complaint, so the refusal happens here rather
    // than being left to the application.
    if Path::new(destination).exists() {
        return Err(IworkConvertError::invalid(format!(
            "{capability} refuses to overwrite an existing destination"
        )));
    }

    let parent = Path::new(destination).parent().ok_or_else(|| {
        IworkConvertError::invalid(format!("{capability} requires an absolute destination"))
    })?;

    if !parent.is_dir() {
        return Err(IworkConvertError::invalid(format!(
            "{capability} destination parent directory does not exist"
        )));
    }

    let output = run_osascript(
        application,
        application.script(),
        &[source, destination, outcome.code()],
    )?;

    if !output.contains("AIOS_IWORK_CONVERTED") {
        return Err(IworkConvertError::execution(format!(
            "{} did not confirm the conversion: {output}",
            application.name()
        )));
    }

    if !Path::new(destination).is_file() {
        return Err(IworkConvertError::execution(format!(
            "{} did not leave a file at the destination",
            application.name()
        )));
    }

    Ok(json!({
        "capability": capability,
        "selectedProvider": "apple-iwork",
        "resourceLocation": "local",
        "inputResource": source,
        "operationResult": {
            "status": "converted",
            "application": application.name(),
            "source": source,
            "destination": destination,
            "format": extension_of(destination),
        },
        "warnings": if outcome == Outcome::Pdf { Vec::<&str>::new() } else {
            vec!["Cross-suite conversion may change document fidelity."]
        },
        "confirmationConsumed": true,
        "validationResult": "converted",
    }))
}

/// The body every one of the three scripts shares.
///
/// It is textually repeated per application rather than parameterised because
/// the export format is an AppleScript keyword, not a string: `as Microsoft
/// Word` cannot be built from a variable. Only that one line differs.
macro_rules! iwork_script {
    ($bundle:literal, $office:literal, $wait_for_content:literal) => {
        concat!(
            r#"
on documentIds()
    set found to {}
    tell application id ""#,
            $bundle,
            r#""
        repeat with candidate in (every document)
            try
                set end of found to (id of candidate) as text
            end try
        end repeat
    end tell
    return found
end documentIds

on isKnown(knownIds, candidateId)
    repeat with knownId in knownIds
        if (contents of knownId as text) is candidateId then return true
    end repeat
    return false
end isKnown

on run argv
    set sourcePath to item 1 of argv
    set targetPath to item 2 of argv
    set operation to item 3 of argv

    -- An alias, not a POSIX file specifier. A POSIX file handed to an iWork
    -- application that is not frontmost is silently discarded for a foreign
    -- format: open returns success and no document ever appears.
    set sourceAlias to (POSIX file sourcePath) as alias
    set openedDocument to missing value
    set wasAlreadyOpen to false

    tell application id ""#,
            $bundle,
            r#""
        -- A document the person already has open stays theirs: it is reused and
        -- never closed.
        repeat with candidateDocument in documents
            try
                set candidateFile to file of candidateDocument
                if candidateFile is not missing value then
                    if (candidateFile as alias) is sourceAlias then
                        set openedDocument to candidateDocument
                        set wasAlreadyOpen to true
                        exit repeat
                    end if
                end if
            end try
        end repeat
    end tell

    if openedDocument is missing value then
        -- Importing a foreign format returns missing value and produces a new
        -- unsaved document whose own file is missing value, so it is found by
        -- which id appeared. That is also what makes it safe to close.
        set knownBefore to my documentIds()

        tell application id ""#,
            $bundle,
            r#""
            open sourceAlias
        end tell

        repeat 120 times
            set seenNow to my documentIds()
            repeat with candidateId in seenNow
                if not my isKnown(knownBefore, contents of candidateId as text) then
                    tell application id ""#,
            $bundle,
            r#""
                        repeat with candidate in (every document)
                            try
                                if ((id of candidate) as text) is (contents of candidateId as text) then
                                    set openedDocument to candidate
                                end if
                            end try
                        end repeat
                    end tell
                    if openedDocument is not missing value then exit repeat
                end if
            end repeat
            if openedDocument is not missing value then exit repeat
            delay 0.25
        end repeat
    end if

    if openedDocument is missing value then
        error "AIOS_IWORK_NO_DOCUMENT" number -1728
    end if
"#,
            $wait_for_content,
            r#"
    tell application id ""#,
            $bundle,
            r#""
        try
            -- The target must carry its extension. Without one iWork treats the
            -- path as a folder and fails with error 6.
            if operation is "pdf" then
                export openedDocument to POSIX file targetPath as PDF
            else if operation is "office" then
                export openedDocument to POSIX file targetPath as "#,
            $office,
            r#"
            else
                save openedDocument in POSIX file targetPath
            end if

            if not wasAlreadyOpen then
                close openedDocument saving no
            end if

            return "AIOS_IWORK_CONVERTED"
        on error errorMessage number errorNumber
            if not wasAlreadyOpen then
                try
                    close openedDocument saving no
                end try
            end if

            error errorMessage number errorNumber
        end try
    end tell
end run
"#
        )
    };
}

const PAGES_SCRIPT: &str = iwork_script!("com.apple.Pages", "Microsoft Word", "");

/// Numbers needs an extra wait, and only Numbers.
///
/// Its document appears before the import has finished, so `sheet 1` exists
/// while `table 1 of sheet 1` is still an invalid index (-1719). An earlier
/// probe read that error as a refusal to open; it is not. The fix is to wait for
/// the table rather than for the document. In the probe it took a single quarter
/// second, which is exactly why it has to be waited for rather than assumed.
const NUMBERS_SCRIPT: &str = iwork_script!(
    "com.apple.Numbers",
    "Microsoft Excel",
    r#"
    repeat 120 times
        tell application id "com.apple.Numbers"
            try
                if (count of tables of sheet 1 of openedDocument) > 0 then exit repeat
            end try
        end tell
        delay 0.25
    end repeat
"#
);

const KEYNOTE_SCRIPT: &str = iwork_script!("com.apple.Keynote", "Microsoft PowerPoint", "");

#[cfg(test)]
mod tests {
    use super::*;

    fn every_application() -> [IworkApplication; 3] {
        [
            IworkApplication::Pages,
            IworkApplication::Numbers,
            IworkApplication::Keynote,
        ]
    }

    #[test]
    fn the_destination_decides_what_conversion_means() {
        for application in every_application() {
            let native = application.native_extension();
            let office = application.office_extension();

            // The same source, three destinations, three different operations --
            // which is what lets one capability mean both cross-suite
            // conversion and PDF export without ambiguity.
            assert_eq!(
                resolve_outcome(
                    application,
                    &format!("/safe/in.{native}"),
                    "/safe/out.pdf"
                )
                .unwrap(),
                Outcome::Pdf
            );
            assert_eq!(
                resolve_outcome(
                    application,
                    &format!("/safe/in.{native}"),
                    &format!("/safe/out.{office}")
                )
                .unwrap(),
                Outcome::Office
            );
            assert_eq!(
                resolve_outcome(
                    application,
                    &format!("/safe/in.{office}"),
                    &format!("/safe/out.{native}")
                )
                .unwrap(),
                Outcome::Native
            );

            // Case is not what decides it.
            assert_eq!(
                resolve_outcome(
                    application,
                    &format!("/safe/in.{}", native.to_uppercase()),
                    "/safe/out.PDF"
                )
                .unwrap(),
                Outcome::Pdf
            );
        }
    }

    #[test]
    fn a_format_the_application_does_not_handle_is_refused_by_name() {
        // Each application is asked for another one's format, which is the
        // mistake a caller actually makes.
        for (application, wrong_source, wrong_destination) in [
            (IworkApplication::Pages, "/safe/in.xlsx", "/safe/out.key"),
            (IworkApplication::Numbers, "/safe/in.pptx", "/safe/out.docx"),
            (IworkApplication::Keynote, "/safe/in.docx", "/safe/out.numbers"),
        ] {
            let native = application.native_extension();

            let source_error = resolve_outcome(
                application,
                wrong_source,
                &format!("/safe/out.{native}"),
            )
            .unwrap_err();
            assert!(source_error.invalid_request);
            assert!(
                source_error.message.contains("reads"),
                "message was {}",
                source_error.message
            );

            let destination_error = resolve_outcome(
                application,
                &format!("/safe/in.{native}"),
                wrong_destination,
            )
            .unwrap_err();
            assert!(destination_error.invalid_request);
            assert!(
                destination_error.message.contains("writes"),
                "message was {}",
                destination_error.message
            );
        }
    }

    #[test]
    fn paths_fail_closed_before_any_application_is_asked() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("in.pages");
        std::fs::write(&source, b"placeholder").unwrap();
        let existing = root.path().join("taken.pdf");
        std::fs::write(&existing, b"placeholder").unwrap();

        let source_path = source.to_str().unwrap().to_owned();

        for (label, request) in [
            ("no source", json!({"destination": "/safe/out.pdf"})),
            ("no destination", json!({"source": source_path.clone()})),
            (
                "relative source",
                json!({"source": "in.pages", "destination": "/safe/out.pdf"}),
            ),
            (
                "relative destination",
                json!({"source": source_path.clone(), "destination": "out.pdf"}),
            ),
            (
                "same path twice",
                json!({"source": source_path.clone(), "destination": source_path.clone()}),
            ),
            (
                "missing source",
                json!({
                    "source": root.path().join("absent.pages").to_str().unwrap(),
                    "destination": root.path().join("out.pdf").to_str().unwrap()
                }),
            ),
            (
                // `export` overwrites without complaint, so this refusal is the
                // only thing standing between a caller and a lost file.
                "destination already exists",
                json!({
                    "source": source_path.clone(),
                    "destination": existing.to_str().unwrap()
                }),
            ),
            (
                "destination directory does not exist",
                json!({
                    "source": source_path.clone(),
                    "destination": root.path().join("absent").join("out.pdf").to_str().unwrap()
                }),
            ),
        ] {
            let error = convert_with_iwork(IworkApplication::Pages, &request).unwrap_err();
            assert!(error.invalid_request, "{label} should be a request problem");
        }

        // The file that was already there is untouched.
        assert_eq!(std::fs::read(&existing).unwrap(), b"placeholder");
    }

    /// A directory the iWork applications can read from.
    ///
    /// The same reasoning as the Pages and Numbers adapters: the probe wrote
    /// under Documents and the applications read it back, an arbitrary temp
    /// directory is not known to work, and a sandbox prompt in an unattended run
    /// blocks every later automation.
    #[cfg(target_os = "macos")]
    fn workspace(label: &str) -> std::path::PathBuf {
        let root = std::path::Path::new(&std::env::var("HOME").unwrap())
            .join("Documents")
            .join(format!("ai-os-convert-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// Pages: a .docx becomes a .pages, and a .pages becomes a .docx and a PDF.
    ///
    /// The .docx fixture comes from `textutil`, so no Word automation is needed
    /// to prove that Word's format is accepted.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires Apple Pages"]
    fn pages_converts_both_directions_and_exports_pdf_real_e2e() {
        let root = workspace("pages");
        let text = root.join("source.txt");
        let docx = root.join("from-textutil.docx");
        std::fs::write(&text, b"Conversion alpha.\nConversion bravo.\n").unwrap();

        assert!(
            std::process::Command::new("/usr/bin/textutil")
                .args(["-convert", "docx", "-output"])
                .arg(&docx)
                .arg(&text)
                .status()
                .unwrap()
                .success(),
            "textutil produced no .docx fixture"
        );

        // Office in, iWork out.
        let native = root.join("converted.pages");
        let to_native = convert_with_iwork(
            IworkApplication::Pages,
            &json!({"source": docx.to_str().unwrap(), "destination": native.to_str().unwrap()}),
        )
        .unwrap();

        assert_eq!(to_native["operationResult"]["format"], "pages");
        assert!(native.is_file(), "Pages left no .pages behind");
        // A cross-suite conversion says so; a PDF export has nothing to warn
        // about.
        assert!(!to_native["warnings"].as_array().unwrap().is_empty());

        // iWork in, Office out -- the direction Word itself cannot do, because
        // Word cannot read a .pages at all.
        let round_trip = root.join("round-trip.docx");
        convert_with_iwork(
            IworkApplication::Pages,
            &json!({
                "source": native.to_str().unwrap(),
                "destination": round_trip.to_str().unwrap()
            }),
        )
        .unwrap();
        assert!(round_trip.is_file(), "Pages left no .docx behind");

        // And the text survived the whole loop, which is what makes it a
        // conversion rather than a file of the right size.
        let read_back = crate::document::structured::read_structured_document(
            &json!({"path": round_trip.to_str().unwrap()}),
        )
        .unwrap();
        assert!(
            read_back["operationResult"]["text"]
                .as_str()
                .unwrap()
                .contains("Conversion alpha."),
            "round trip lost the text: {read_back:#?}"
        );

        // PDF export.
        let pdf = root.join("exported.pdf");
        let exported = convert_with_iwork(
            IworkApplication::Pages,
            &json!({
                "source": native.to_str().unwrap(),
                "destination": pdf.to_str().unwrap()
            }),
        )
        .unwrap();

        assert_eq!(exported["operationResult"]["format"], "pdf");
        assert!(exported["warnings"].as_array().unwrap().is_empty());
        assert!(std::fs::metadata(&pdf).unwrap().len() > 0);

        // Refusing to overwrite is proven against the real application, not
        // only against the path checks.
        let refused = convert_with_iwork(
            IworkApplication::Pages,
            &json!({
                "source": native.to_str().unwrap(),
                "destination": pdf.to_str().unwrap()
            }),
        )
        .unwrap_err();
        assert!(refused.invalid_request);

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Numbers: a .xlsx becomes a .numbers, and back, and a PDF.
    ///
    /// The .xlsx fixture is written by the structured layer, so this also proves
    /// that a workbook produced with no application at all is one Numbers
    /// accepts -- not just one Excel accepts.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires Apple Numbers"]
    fn numbers_converts_both_directions_and_exports_pdf_real_e2e() {
        let root = workspace("numbers");
        let xlsx = root.join("from-structured.xlsx");

        crate::document::structured::create_structured_spreadsheet(&json!({
            "path": xlsx.to_str().unwrap(),
            "content": "Region\tTotal\nNorth\t42",
        }))
        .unwrap();

        let native = root.join("converted.numbers");
        convert_with_iwork(
            IworkApplication::Numbers,
            &json!({"source": xlsx.to_str().unwrap(), "destination": native.to_str().unwrap()}),
        )
        .unwrap();
        assert!(native.is_file(), "Numbers left no .numbers behind");

        let round_trip = root.join("round-trip.xlsx");
        convert_with_iwork(
            IworkApplication::Numbers,
            &json!({
                "source": native.to_str().unwrap(),
                "destination": round_trip.to_str().unwrap()
            }),
        )
        .unwrap();

        // Read the round trip with no application, so what is checked is the
        // file and not Numbers' opinion of it.
        let read_back = crate::document::structured::read_structured_spreadsheet(
            &json!({"path": round_trip.to_str().unwrap()}),
        )
        .unwrap();
        let content = read_back["content"]
            .as_str()
            .or_else(|| read_back["sheets"][0]["content"].as_str())
            .unwrap_or_default()
            .to_owned();
        assert!(
            content.contains("Region") && content.contains("North"),
            "round trip lost the cells: {read_back:#?}"
        );

        let pdf = root.join("exported.pdf");
        convert_with_iwork(
            IworkApplication::Numbers,
            &json!({
                "source": native.to_str().unwrap(),
                "destination": pdf.to_str().unwrap()
            }),
        )
        .unwrap();
        assert!(std::fs::metadata(&pdf).unwrap().len() > 0);

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Keynote: a .key becomes a .pptx, that .pptx becomes a .key, and a PDF.
    ///
    /// PowerPoint is deliberately not driven for the fixture: doing that hung an
    /// earlier probe, and Keynote can produce the .pptx itself, which makes this
    /// test prove both directions without needing Microsoft Office installed.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires Apple Keynote"]
    fn keynote_converts_both_directions_and_exports_pdf_real_e2e() {
        let root = workspace("keynote");
        let native = root.join("source.key");

        crate::document::keynote::create_keynote_presentation(&json!({
            "path": native.to_str().unwrap(),
            "title": "Conversion Title",
            "body": "Conversion Body",
        }))
        .unwrap();
        assert!(native.is_file(), "Keynote left no .key behind");

        let pptx = root.join("converted.pptx");
        convert_with_iwork(
            IworkApplication::Keynote,
            &json!({
                "source": native.to_str().unwrap(),
                "destination": pptx.to_str().unwrap()
            }),
        )
        .unwrap();

        // Read the .pptx with no application: the slide has to carry the title
        // Keynote was given, or the conversion moved bytes and not content.
        let read_back = crate::document::structured::read_structured_presentation(
            &json!({"path": pptx.to_str().unwrap()}),
        )
        .unwrap();
        assert_eq!(read_back["operationResult"]["slideCount"], 1);
        assert!(
            read_back["operationResult"]["slides"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Conversion Title"),
            "the .pptx lost the title: {read_back:#?}"
        );

        // And back the other way, from the .pptx this just produced.
        let back_to_native = root.join("round-trip.key");
        convert_with_iwork(
            IworkApplication::Keynote,
            &json!({
                "source": pptx.to_str().unwrap(),
                "destination": back_to_native.to_str().unwrap()
            }),
        )
        .unwrap();
        assert!(back_to_native.is_file(), "Keynote left no round-trip .key");

        let pdf = root.join("exported.pdf");
        convert_with_iwork(
            IworkApplication::Keynote,
            &json!({
                "source": native.to_str().unwrap(),
                "destination": pdf.to_str().unwrap()
            }),
        )
        .unwrap();
        assert!(std::fs::metadata(&pdf).unwrap().len() > 0);

        std::fs::remove_dir_all(&root).unwrap();
    }
}
