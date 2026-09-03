//! Apple Pages adapter.
//!
//! Every AppleScript form here was proved on real Pages 15.3.1 by
//! `verify/probe_iwork_pages_numbers_semantics.sh` before it was written. The
//! two findings that shape this file:
//!
//! * an export target **must carry its extension**. Given a path without one,
//!   Pages treats it as a folder, fails with error 6, and leaves its document
//!   open.
//! * the bundle identifier is `com.apple.Pages`. `com.apple.iWork.Pages` does
//!   not resolve.

use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const MAX_PARAGRAPHS: usize = 500;
const MAX_PARAGRAPH_CHARS: usize = 4_096;
const MAX_DOCUMENT_BODY: usize = 65_536;

#[derive(Debug)]
pub(crate) struct PagesError {
    pub(crate) invalid_request: bool,
    pub(crate) message: String,
}

impl PagesError {
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

/// An absolute path with the expected extension, existing or refusing to
/// overwrite depending on which side of the operation it is on.
fn require_local_path<'a>(
    input: &'a Value,
    field: &str,
    operation: &str,
    extension: &str,
    must_exist: bool,
) -> Result<&'a str, PagesError> {
    let path = input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| PagesError::invalid(format!("{operation} requires {field}")))?;

    let target = Path::new(path);

    if !target.is_absolute() {
        return Err(PagesError::invalid(format!(
            "{operation} requires an absolute {field}"
        )));
    }

    let actual = target
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    if actual != extension {
        return Err(PagesError::invalid(format!(
            "{operation} requires a .{extension} {field}"
        )));
    }

    if must_exist {
        if !target.is_file() {
            return Err(PagesError::invalid(format!("{operation} {field} does not exist")));
        }

        return Ok(path);
    }

    if target.exists() {
        return Err(PagesError::invalid(format!(
            "{operation} refuses to overwrite an existing {field}"
        )));
    }

    let parent = target
        .parent()
        .ok_or_else(|| PagesError::invalid(format!("{operation} requires an absolute {field}")))?;

    if !parent.is_dir() {
        return Err(PagesError::invalid(format!(
            "{operation} {field} parent directory does not exist"
        )));
    }

    Ok(path)
}

fn run_osascript(script: &str, args: &[&str]) -> Result<String, PagesError> {
    let mut child = Command::new("/usr/bin/osascript")
        .arg("-")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| PagesError::execution("Unable to start Pages AppleScript automation"))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| PagesError::execution("Unable to open AppleScript input"))?;

        stdin
            .write_all(script.as_bytes())
            .map_err(|_| PagesError::execution("Unable to write Pages AppleScript"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|_| PagesError::execution("Pages AppleScript did not complete"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(PagesError::execution(if stderr.is_empty() {
            "Pages AppleScript automation failed".to_owned()
        } else {
            format!("Pages AppleScript automation failed: {stderr}")
        }));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

const READ_SCRIPT: &str = r#"
on cleanText(sourceText, maximumLength)
    set outputText to sourceText as text

    set oldDelimiters to AppleScript's text item delimiters
    repeat with separatorValue in {return, linefeed, tab}
        set AppleScript's text item delimiters to separatorValue
        set textParts to text items of outputText
        set AppleScript's text item delimiters to " "
        set outputText to textParts as text
    end repeat
    set AppleScript's text item delimiters to oldDelimiters

    if (count characters of outputText) > maximumLength then
        set outputText to text 1 thru maximumLength of outputText
    end if

    return outputText
end cleanText

on run argv
    set targetPath to item 1 of argv
    set paragraphLimit to (item 2 of argv) as integer
    set paragraphChars to (item 3 of argv) as integer
    set targetAlias to POSIX file targetPath as alias
    set openedDocument to missing value
    set wasAlreadyOpen to false

    tell application id "com.apple.Pages"
        try
            -- A document the user already has open is theirs: read it where it
            -- is and leave it open.
            repeat with candidateDocument in documents
                try
                    set candidateFile to file of candidateDocument
                    if candidateFile is not missing value then
                        if (candidateFile as alias) is targetAlias then
                            set openedDocument to candidateDocument
                            set wasAlreadyOpen to true
                            exit repeat
                        end if
                    end if
                end try
            end repeat

            if openedDocument is missing value then
                set openedDocument to open POSIX file targetPath
            end if

            set documentBody to ""
            try
                set documentBody to (body text of openedDocument) as text
            end try

            set totalCharacters to count characters of documentBody
            set totalWords to 0
            set totalParagraphs to 0

            if totalCharacters > 0 then
                set totalWords to count words of documentBody
                set totalParagraphs to count paragraphs of documentBody
            end if

            set emittedParagraphs to totalParagraphs
            if emittedParagraphs > paragraphLimit then
                set emittedParagraphs to paragraphLimit
            end if

            set outputText to "AIOS_CHARACTERS=" & totalCharacters & linefeed
            set outputText to outputText & "AIOS_WORDS=" & totalWords & linefeed
            set outputText to outputText & "AIOS_PARAGRAPHS=" & totalParagraphs & linefeed
            set outputText to outputText & "AIOS_EMITTED_PARAGRAPHS=" & emittedParagraphs & linefeed

            if totalParagraphs > emittedParagraphs then
                set outputText to outputText & "AIOS_TRUNCATED=true" & linefeed
            else
                set outputText to outputText & "AIOS_TRUNCATED=false" & linefeed
            end if

            repeat with paragraphIndex from 1 to emittedParagraphs
                set paragraphText to my cleanText(paragraph paragraphIndex of documentBody, paragraphChars)
                set outputText to outputText & "AIOS_PARAGRAPH=" & paragraphText & linefeed
            end repeat

            if not wasAlreadyOpen then
                close openedDocument saving no
            end if

            return outputText
        on error errorMessage number errorNumber
            if openedDocument is not missing value and not wasAlreadyOpen then
                try
                    close openedDocument saving no
                end try
            end if

            error errorMessage number errorNumber
        end try
    end tell
end run
"#;

const CREATE_SCRIPT: &str = r#"
on run argv
    set targetPath to item 1 of argv
    set documentBody to item 2 of argv
    set createdDocument to missing value

    tell application id "com.apple.Pages"
        try
            set createdDocument to make new document

            if documentBody is not "" then
                set body text of createdDocument to documentBody
            end if

            save createdDocument in POSIX file targetPath
            close createdDocument saving no
            set createdDocument to missing value

            return "AIOS_PAGES_CREATED"
        on error errorMessage number errorNumber
            if createdDocument is not missing value then
                try
                    close createdDocument saving no
                end try
            end if

            error errorMessage number errorNumber
        end try
    end tell
end run
"#;

const CONVERT_SCRIPT: &str = r#"
on run argv
    set sourcePath to item 1 of argv
    set targetPath to item 2 of argv
    set sourceAlias to POSIX file sourcePath as alias
    set openedDocument to missing value
    set wasAlreadyOpen to false

    tell application id "com.apple.Pages"
        try
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

            if openedDocument is missing value then
                set openedDocument to open POSIX file sourcePath
            end if

            -- The target path must carry its extension. Without one Pages
            -- treats it as a folder and fails with error 6.
            export openedDocument to POSIX file targetPath as PDF

            if not wasAlreadyOpen then
                close openedDocument saving no
            end if

            return "AIOS_PAGES_EXPORTED"
        on error errorMessage number errorNumber
            if openedDocument is not missing value and not wasAlreadyOpen then
                try
                    close openedDocument saving no
                end try
            end if

            error errorMessage number errorNumber
        end try
    end tell
end run
"#;

fn parse_read_output(path: &str, output: &str) -> Result<Value, PagesError> {
    let mut characters = None;
    let mut words = None;
    let mut paragraph_count = None;
    let mut emitted = None;
    let mut truncated = false;
    let mut paragraphs: Vec<Value> = Vec::new();

    for line in output.lines() {
        if let Some(value) = line.strip_prefix("AIOS_CHARACTERS=") {
            characters = value.trim().parse::<u64>().ok();
        } else if let Some(value) = line.strip_prefix("AIOS_WORDS=") {
            words = value.trim().parse::<u64>().ok();
        } else if let Some(value) = line.strip_prefix("AIOS_PARAGRAPHS=") {
            paragraph_count = value.trim().parse::<u64>().ok();
        } else if let Some(value) = line.strip_prefix("AIOS_EMITTED_PARAGRAPHS=") {
            emitted = value.trim().parse::<u64>().ok();
        } else if let Some(value) = line.strip_prefix("AIOS_TRUNCATED=") {
            truncated = value.trim() == "true";
        } else if let Some(value) = line.strip_prefix("AIOS_PARAGRAPH=") {
            paragraphs.push(Value::String(value.to_owned()));
        }
    }

    let characters =
        characters.ok_or_else(|| PagesError::execution("Pages read returned no character count"))?;
    let words = words.ok_or_else(|| PagesError::execution("Pages read returned no word count"))?;
    let paragraph_count = paragraph_count
        .ok_or_else(|| PagesError::execution("Pages read returned no paragraph count"))?;
    let emitted =
        emitted.ok_or_else(|| PagesError::execution("Pages read returned no emitted count"))?;

    // A short paragraph list means the payload was cut on the way out, which the
    // caller has to know about rather than receive as a complete document.
    if (paragraphs.len() as u64) != emitted {
        return Err(PagesError::execution(
            "Pages read returned fewer paragraphs than it declared",
        ));
    }

    Ok(json!({
        "path": path,
        "status": "document",
        "characters": characters,
        "words": words,
        "paragraphCount": paragraph_count,
        "emittedParagraphs": emitted,
        "truncated": truncated,
        "paragraphs": paragraphs,
    }))
}

pub(crate) fn read_pages_document(input: &Value) -> Result<Value, PagesError> {
    let path = require_local_path(input, "path", "document.read", "pages", true)?;
    let output = run_osascript(
        READ_SCRIPT,
        &[
            path,
            &MAX_PARAGRAPHS.to_string(),
            &MAX_PARAGRAPH_CHARS.to_string(),
        ],
    )?;

    parse_read_output(path, &output)
}

pub(crate) fn create_pages_document(input: &Value) -> Result<Value, PagesError> {
    let path = require_local_path(input, "path", "document.create", "pages", false)?;

    let body = input.get("body").and_then(Value::as_str).unwrap_or("");

    if body.chars().count() > MAX_DOCUMENT_BODY {
        return Err(PagesError::invalid(
            "document.create body exceeds the supported length",
        ));
    }

    let output = run_osascript(CREATE_SCRIPT, &[path, body])?;

    if !output.contains("AIOS_PAGES_CREATED") {
        return Err(PagesError::execution("Pages did not confirm the new document"));
    }

    if !Path::new(path).is_file() {
        return Err(PagesError::execution("Pages did not leave a document at the path"));
    }

    Ok(json!({
        "path": path,
        "status": "created",
        "provider": "apple-iwork",
    }))
}

pub(crate) fn convert_pages_document(input: &Value) -> Result<Value, PagesError> {
    let source = require_local_path(input, "source", "document.convert", "pages", true)?;
    let destination = require_local_path(input, "destination", "document.convert", "pdf", false)?;

    let output = run_osascript(CONVERT_SCRIPT, &[source, destination])?;

    if !output.contains("AIOS_PAGES_EXPORTED") {
        return Err(PagesError::execution("Pages did not confirm the export"));
    }

    if !Path::new(destination).is_file() {
        return Err(PagesError::execution("Pages did not leave a PDF at the destination"));
    }

    Ok(json!({
        "source": source,
        "destination": destination,
        "status": "converted",
        "format": "pdf",
        "provider": "apple-iwork",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn pages_paths_fail_closed_on_shape_existence_and_overwrite() {
        let root = tempfile::tempdir().unwrap();
        let existing = root.path().join("existing.pages");
        fs::write(&existing, b"placeholder").unwrap();

        // The borrow has to outlive the call, so these return whether it was
        // accepted rather than the borrowed path itself.
        let read = |value: &Value| {
            require_local_path(value, "path", "document.read", "pages", true).is_ok()
        };
        let create = |value: &Value| {
            require_local_path(value, "path", "document.create", "pages", false).is_ok()
        };

        assert!(read(&json!({"path": existing.to_str().unwrap()})));
        assert!(create(&json!({"path": root.path().join("fresh.pages").to_str().unwrap()})));

        for rejected in [
            // No path at all, or an empty one.
            json!({}),
            json!({"path": "   "}),
            // Relative paths cannot be handed to AppleScript's POSIX file.
            json!({"path": "relative.pages"}),
            // Another application's format.
            json!({"path": root.path().join("wrong.key").to_str().unwrap()}),
            json!({"path": root.path().join("none").to_str().unwrap()}),
        ] {
            assert!(!read(&rejected), "{rejected} should be rejected");
            assert!(!create(&rejected), "{rejected} should be rejected");
        }

        // A read needs the file to be there; a create refuses to replace it.
        assert!(!read(&json!({"path": root.path().join("missing.pages").to_str().unwrap()})));
        assert!(!create(&json!({"path": existing.to_str().unwrap()})));

        // A create into a directory that does not exist fails here rather than
        // half-way through Pages.
        assert!(!create(
            &json!({"path": root.path().join("nope").join("child.pages").to_str().unwrap()})
        ));
    }

    #[test]
    fn pages_read_parser_refuses_a_short_payload() {
        let complete = "AIOS_CHARACTERS=33\nAIOS_WORDS=4\nAIOS_PARAGRAPHS=2\n\
AIOS_EMITTED_PARAGRAPHS=2\nAIOS_TRUNCATED=false\n\
AIOS_PARAGRAPH=Alpha paragraph.\nAIOS_PARAGRAPH=Bravo paragraph.\n";

        let parsed = parse_read_output("/safe/a.pages", complete).unwrap();
        assert_eq!(parsed["status"], "document");
        assert_eq!(parsed["characters"], 33);
        assert_eq!(parsed["words"], 4);
        assert_eq!(parsed["paragraphCount"], 2);
        assert_eq!(parsed["truncated"], false);
        assert_eq!(parsed["paragraphs"][1], "Bravo paragraph.");

        // Declared two, sent one: the payload was cut, and reporting it as a
        // complete document would be a lie.
        let cut = complete.replace("AIOS_PARAGRAPH=Bravo paragraph.\n", "");
        assert!(parse_read_output("/safe/a.pages", &cut).is_err());

        // A document longer than the emit limit says so.
        let truncated = complete
            .replace("AIOS_PARAGRAPHS=2", "AIOS_PARAGRAPHS=900")
            .replace("AIOS_TRUNCATED=false", "AIOS_TRUNCATED=true");
        let parsed = parse_read_output("/safe/a.pages", &truncated).unwrap();
        assert_eq!(parsed["paragraphCount"], 900);
        assert_eq!(parsed["emittedParagraphs"], 2);
        assert_eq!(parsed["truncated"], true);

        // Nothing at all is an execution failure, not an empty document.
        assert!(parse_read_output("/safe/a.pages", "").is_err());
    }

    /// Pages is sandboxed. The probe wrote under the user's Documents folder and
    /// Pages could read it back; an arbitrary temp directory is not known to
    /// work, and a sandbox prompt in an unattended run blocks every later
    /// automation. So this uses a uniquely named directory there and removes it.
    #[cfg(target_os = "macos")]
    fn pages_workspace() -> std::path::PathBuf {
        let root = std::path::Path::new(&std::env::var("HOME").unwrap())
            .join("Documents")
            .join(format!("ai-os-pages-e2e-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires Apple Pages"]
    fn pages_document_real_e2e() {
        let root = pages_workspace();
        let document = root.join("phase-i-pages.pages");
        let exported = root.join("phase-i-pages.pdf");

        let body = "Alpha paragraph.\nBravo paragraph.\nCharlie paragraph.";

        let created = create_pages_document(&json!({
            "path": document.to_str().unwrap(),
            "body": body,
        }))
        .unwrap();

        assert_eq!(created["status"], "created");
        assert!(document.is_file(), "Pages left no document behind");

        // Read it back in its own osascript invocation, the way a caller would.
        let read = read_pages_document(&json!({"path": document.to_str().unwrap()})).unwrap();

        assert_eq!(read["status"], "document");
        assert_eq!(read["paragraphCount"], 3);
        assert_eq!(read["emittedParagraphs"], 3);
        assert_eq!(read["truncated"], false);
        assert_eq!(read["paragraphs"][0], "Alpha paragraph.");
        assert_eq!(read["paragraphs"][2], "Charlie paragraph.");

        // The counts are Pages' own, so this checks they are plausible rather
        // than guessing its word-breaking rules.
        assert_eq!(read["words"], 6);
        assert!(read["characters"].as_u64().unwrap() >= body.chars().count() as u64 - 2);

        // Convert. The extension on the destination is what makes this work.
        let converted = convert_pages_document(&json!({
            "source": document.to_str().unwrap(),
            "destination": exported.to_str().unwrap(),
        }))
        .unwrap();

        assert_eq!(converted["format"], "pdf");
        assert!(exported.is_file(), "Pages left no PDF behind");
        assert!(
            fs::read(&exported).unwrap().starts_with(b"%PDF"),
            "the exported file is not a PDF"
        );

        // Creating over an existing document is refused, not silently replaced.
        assert!(create_pages_document(&json!({
            "path": document.to_str().unwrap(),
            "body": "replacement",
        }))
        .is_err());

        let _ = fs::remove_dir_all(&root);
    }
}
