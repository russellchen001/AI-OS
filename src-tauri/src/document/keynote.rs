use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const MAX_PRESENTATION_SLIDES: usize = 100;
const MAX_TEXT_PER_SLIDE: usize = 4_096;
const MAX_PRESENTATION_TITLE: usize = 512;
const MAX_PRESENTATION_BODY: usize = 16_384;

#[derive(Debug)]
pub(crate) struct KeynoteError {
    pub(crate) invalid_request: bool,
    pub(crate) message: String,
}

impl KeynoteError {
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

fn require_local_keynote_path<'a>(
    input: &'a Value,
    operation: &str,
    must_exist: bool,
) -> Result<&'a str, KeynoteError> {
    let path = input
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| KeynoteError::invalid(format!("{operation} requires path")))?;

    let target = Path::new(path);

    if !target.is_absolute() {
        return Err(KeynoteError::invalid(format!(
            "{operation} requires an absolute file path"
        )));
    }

    let extension = target
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    if extension != "key" {
        return Err(KeynoteError::invalid(format!(
            "{operation} currently supports KEY files"
        )));
    }

    if must_exist && !target.exists() {
        return Err(KeynoteError::invalid(format!(
            "{operation} target does not exist"
        )));
    }

    if !must_exist {
        if target.exists() {
            return Err(KeynoteError::invalid(
                "presentation.create refuses to overwrite an existing target",
            ));
        }

        let parent = target.parent().ok_or_else(|| {
            KeynoteError::invalid("presentation.create requires an absolute output path")
        })?;

        if !parent.is_dir() {
            return Err(KeynoteError::invalid(
                "presentation.create parent directory does not exist",
            ));
        }
    }

    Ok(path)
}

fn run_osascript(script: &str, args: &[&str]) -> Result<String, KeynoteError> {
    let mut child = Command::new("/usr/bin/osascript")
        .arg("-")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| KeynoteError::execution("Unable to start Keynote AppleScript automation"))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| KeynoteError::execution("Unable to open AppleScript input"))?;

        stdin
            .write_all(script.as_bytes())
            .map_err(|_| KeynoteError::execution("Unable to write Keynote AppleScript"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|_| KeynoteError::execution("Keynote AppleScript did not complete"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(KeynoteError::execution(if stderr.is_empty() {
            "Keynote AppleScript automation failed".to_owned()
        } else {
            format!("Keynote AppleScript automation failed: {stderr}")
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

on appendUnique(targetList, candidateText)
    if candidateText is missing value then return targetList

    set normalizedText to my cleanText(candidateText as text, 4096)

    if normalizedText is "" then return targetList

    if targetList does not contain normalizedText then
        set end of targetList to normalizedText
    end if

    return targetList
end appendUnique

on run argv
    set targetPath to item 1 of argv
    set targetAlias to POSIX file targetPath as alias
    set openedDocument to missing value
    set wasAlreadyOpen to false

    tell application id "com.apple.Keynote"
        try
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

            set totalSlides to count of slides of openedDocument
            set emittedSlides to totalSlides

            if emittedSlides > 100 then
                set emittedSlides to 100
            end if

            set outputText to "AIOS_SLIDES=" & totalSlides & linefeed
            set outputText to outputText & "AIOS_EMITTED_SLIDES=" & emittedSlides & linefeed

            if totalSlides > emittedSlides then
                set outputText to outputText & "AIOS_TRUNCATED=true" & linefeed
            else
                set outputText to outputText & "AIOS_TRUNCATED=false" & linefeed
            end if

            repeat with slideIndex from 1 to emittedSlides
                set currentSlide to slide slideIndex of openedDocument
                set collectedText to {}

                try
                    set collectedText to my appendUnique(collectedText, object text of default title item of currentSlide)
                end try

                try
                    set collectedText to my appendUnique(collectedText, object text of default body item of currentSlide)
                end try

                try
                    repeat with currentTextItem in every text item of currentSlide
                        try
                            set collectedText to my appendUnique(collectedText, object text of currentTextItem)
                        end try
                    end repeat
                end try

                set outputText to outputText & "AIOS_SLIDE=" & slideIndex & linefeed

                repeat with collectedItem in collectedText
                    set outputText to outputText & "AIOS_TEXT=" & (collectedItem as text) & linefeed
                end repeat
            end repeat

            if not wasAlreadyOpen then
                close openedDocument saving no
                set openedDocument to missing value
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
    set presentationTitle to item 2 of argv
    set presentationBody to item 3 of argv
    set createdDocument to missing value

    tell application id "com.apple.Keynote"
        try
            set createdDocument to make new document with properties {document theme:item 1 of themes}

            tell first slide of createdDocument
                try
                    set object text of default title item to presentationTitle
                end try

                if presentationBody is not "" then
                    try
                        set object text of default body item to presentationBody
                    end try
                end if
            end tell

            save createdDocument in POSIX file targetPath
            close createdDocument saving no
            set createdDocument to missing value

            return "AIOS_KEYNOTE_CREATED"
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

fn parse_read_output(path: &str, output: &str) -> Result<Value, KeynoteError> {
    let mut slide_count = None;
    let mut emitted_slides = None;
    let mut truncated = false;
    let mut slides: Vec<Value> = Vec::new();
    let mut current_index: Option<u64> = None;
    let mut current_text: Vec<String> = Vec::new();

    let flush_slide = |slides: &mut Vec<Value>, index: &mut Option<u64>, text: &mut Vec<String>| {
        if let Some(slide_index) = index.take() {
            slides.push(json!({
                "index": slide_index,
                "text": std::mem::take(text),
            }));
        }
    };

    for line in output.lines() {
        if let Some(value) = line.strip_prefix("AIOS_SLIDES=") {
            slide_count = value.trim().parse::<u64>().ok();
            continue;
        }

        if let Some(value) = line.strip_prefix("AIOS_EMITTED_SLIDES=") {
            emitted_slides = value.trim().parse::<u64>().ok();
            continue;
        }

        if let Some(value) = line.strip_prefix("AIOS_TRUNCATED=") {
            truncated = value.trim() == "true";
            continue;
        }

        if let Some(value) = line.strip_prefix("AIOS_SLIDE=") {
            flush_slide(&mut slides, &mut current_index, &mut current_text);
            current_index = value.trim().parse::<u64>().ok();
            continue;
        }

        if let Some(value) = line.strip_prefix("AIOS_TEXT=") {
            if current_text.len() < 128 {
                current_text.push(value.to_owned());
            }
        }
    }

    flush_slide(&mut slides, &mut current_index, &mut current_text);

    let slide_count = slide_count
        .ok_or_else(|| KeynoteError::execution("Keynote read returned no slide count"))?;

    let emitted_slides = emitted_slides.unwrap_or(slides.len() as u64);

    Ok(json!({
        "provider": "apple-iwork",
        "application": "keynote",
        "path": path,
        "status": "presentation",
        "slideCount": slide_count,
        "emittedSlides": emitted_slides,
        "truncated": truncated,
        "slides": slides,
    }))
}

pub(crate) fn read_keynote_presentation(input: &Value) -> Result<Value, KeynoteError> {
    let path = require_local_keynote_path(input, "presentation.read", true)?;
    let output = run_osascript(READ_SCRIPT, &[path])?;
    parse_read_output(path, &output)
}

pub(crate) fn create_keynote_presentation(input: &Value) -> Result<Value, KeynoteError> {
    let path = require_local_keynote_path(input, "presentation.create", false)?;

    let title = input
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| KeynoteError::invalid("presentation.create requires title"))?;

    if title.chars().count() > MAX_PRESENTATION_TITLE {
        return Err(KeynoteError::invalid(
            "presentation.create title exceeds the supported length",
        ));
    }

    let body = input
        .get("body")
        .or_else(|| input.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("");

    if body.chars().count() > MAX_PRESENTATION_BODY {
        return Err(KeynoteError::invalid(
            "presentation.create body exceeds the supported length",
        ));
    }

    let output = run_osascript(CREATE_SCRIPT, &[path, title, body])?;

    if !output
        .lines()
        .any(|line| line.trim() == "AIOS_KEYNOTE_CREATED")
    {
        return Err(KeynoteError::execution(
            "Keynote create returned an invalid completion marker",
        ));
    }

    if !Path::new(path).exists() {
        return Err(KeynoteError::execution(
            "Keynote create completed without creating the target",
        ));
    }

    let read_back = read_keynote_presentation(&json!({ "path": path }))?;

    let slide_count = read_back
        .get("slideCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    if slide_count == 0 {
        return Err(KeynoteError::execution(
            "Keynote create read-back validation found no slides",
        ));
    }

    let mut visible_text = Vec::new();

    if let Some(slides) = read_back.get("slides").and_then(Value::as_array) {
        for slide in slides {
            if let Some(items) = slide.get("text").and_then(Value::as_array) {
                for item in items.iter().filter_map(Value::as_str) {
                    visible_text.push(item);
                }
            }
        }
    }

    if !visible_text.iter().any(|value| *value == title) {
        return Err(KeynoteError::execution(
            "Keynote create read-back validation did not recover the title",
        ));
    }

    if !body.is_empty() && !visible_text.iter().any(|value| *value == body) {
        return Err(KeynoteError::execution(
            "Keynote create read-back validation did not recover the body",
        ));
    }

    Ok(json!({
        "provider": "apple-iwork",
        "application": "keynote",
        "path": path,
        "status": "created",
        "validated": true,
        "slideCount": slide_count,
        "slides": read_back.get("slides").cloned().unwrap_or_else(|| json!([])),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parser_preserves_slide_order_and_text() {
        let parsed = parse_read_output(
            "/tmp/example.key",
            "AIOS_SLIDES=2\nAIOS_EMITTED_SLIDES=2\nAIOS_TRUNCATED=false\nAIOS_SLIDE=1\nAIOS_TEXT=One\nAIOS_SLIDE=2\nAIOS_TEXT=Two\n",
        )
        .unwrap();

        assert_eq!(parsed["slideCount"], 2);
        assert_eq!(parsed["slides"][0]["index"], 1);
        assert_eq!(parsed["slides"][0]["text"][0], "One");
        assert_eq!(parsed["slides"][1]["index"], 2);
        assert_eq!(parsed["slides"][1]["text"][0], "Two");
    }

    #[test]
    fn local_path_validation_requires_absolute_key_file() {
        let error = read_keynote_presentation(&json!({"path":"relative.key"})).unwrap_err();
        assert!(error.invalid_request);
    }

    #[test]
    #[ignore = "requires Keynote and macOS Automation authorization"]
    fn keynote_real_e2e() {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let root = std::env::temp_dir().join(format!(
            "ai-os-keynote-real-e2e-{}-{timestamp}",
            std::process::id()
        ));

        fs::create_dir_all(&root).unwrap();

        let target = root.join("AI-OS-Presentation-E2E.key");
        let target_string = target.to_string_lossy().to_string();

        let title = "AI-OS Presentation Runtime E2E";
        let body = "Provider-neutral Keynote read and create";

        let created = create_keynote_presentation(&json!({
            "path": target_string,
            "title": title,
            "body": body,
        }))
        .expect("create Keynote presentation");

        assert_eq!(created["status"], "created");
        assert_eq!(created["validated"], true);
        assert!(target.exists());

        let read = read_keynote_presentation(&json!({
            "path": target.to_string_lossy(),
        }))
        .expect("read Keynote presentation");

        assert_eq!(read["status"], "presentation");
        assert_eq!(read["slideCount"], 1);

        let serialized = serde_json::to_string(&read).unwrap();
        assert!(serialized.contains(title));
        assert!(serialized.contains(body));

        run_osascript(
            r#"on run argv
                tell application id "com.apple.Keynote"
                    open POSIX file (item 1 of argv)
                    delay 1
                end tell
            end run"#,
            &[&target_string],
        )
        .expect("open presentation before ownership check");

        read_keynote_presentation(&json!({ "path": target_string }))
            .expect("read already-open Keynote presentation");

        let still_open = run_osascript(
            r#"on run argv
                set targetPath to item 1 of argv
                set targetAlias to POSIX file targetPath as alias
                tell application id "com.apple.Keynote"
                    repeat with candidateDocument in documents
                        try
                            set candidateFile to file of candidateDocument
                            if candidateFile is not missing value then
                                if (candidateFile as alias) is targetAlias then
                                    close candidateDocument saving no
                                    return "AIOS_ALREADY_OPEN_PRESERVED"
                                end if
                            end if
                        end try
                    end repeat
                end tell
                return "AIOS_ALREADY_OPEN_CLOSED"
            end run"#,
            &[&target_string],
        )
        .expect("inspect already-open presentation ownership");

        assert!(
            still_open.contains("AIOS_ALREADY_OPEN_PRESERVED"),
            "ownership check returned: {still_open}"
        );

        let overwrite = create_keynote_presentation(&json!({
            "path": target.to_string_lossy(),
            "title": "SHOULD NOT OVERWRITE",
            "body": "",
        }))
        .unwrap_err();

        assert!(overwrite.invalid_request);

        let read_after_rejected_overwrite = read_keynote_presentation(&json!({
            "path": target.to_string_lossy(),
        }))
        .unwrap();

        let serialized_after = serde_json::to_string(&read_after_rejected_overwrite).unwrap();
        assert!(serialized_after.contains(title));
        assert!(!serialized_after.contains("SHOULD NOT OVERWRITE"));

        fs::remove_dir_all(&root).unwrap();
    }
}
