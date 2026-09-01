use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_DOCUMENT_TEXT: usize = 64 * 1024;
const MAX_CREATE_TEXT: usize = 32 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WordError {
    pub invalid_request: bool,
    pub message: String,
}

impl WordError {
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

fn require_path<'a>(input: &'a Value, field: &str, must_exist: bool) -> Result<&'a str, WordError> {
    let path = input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| WordError::invalid(format!("Microsoft Word operation requires {field}")))?;
    let parsed = Path::new(path);
    if !parsed.is_absolute() {
        return Err(WordError::invalid(format!(
            "{field} must be an absolute path"
        )));
    }
    let extension = parsed
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "doc" | "docx" | "pdf") {
        return Err(WordError::invalid(format!(
            "unsupported Word file format: {extension}"
        )));
    }
    if must_exist && !parsed.is_file() {
        return Err(WordError::invalid(format!("{field} does not exist")));
    }
    if !must_exist && parsed.exists() {
        return Err(WordError::invalid(format!(
            "{field} already exists; overwrite is not permitted"
        )));
    }
    Ok(path)
}

fn run_osascript(script: &str, args: &[&str]) -> Result<String, WordError> {
    let mut child = Command::new("/usr/bin/osascript")
        .arg("-")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| WordError::execution("Unable to start Microsoft Word automation"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| WordError::execution("Unable to open AppleScript input"))?
        .write_all(script.as_bytes())
        .map_err(|_| WordError::execution("Unable to write Microsoft Word AppleScript"))?;
    let output = child
        .wait_with_output()
        .map_err(|_| WordError::execution("Microsoft Word AppleScript did not complete"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(WordError::execution(if stderr.is_empty() {
            "Microsoft Word automation failed".to_owned()
        } else {
            format!("Microsoft Word automation failed: {stderr}")
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn word_cache_output(
    extension: &str,
) -> Result<(std::path::PathBuf, std::path::PathBuf), WordError> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| WordError::execution("Word cache home is unavailable"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WordError::execution("System clock is unavailable"))?
        .as_nanos();
    let root = Path::new(&home)
        .join("Library/Containers/com.microsoft.Word/Data/Library/Caches/com.microsoft.Word")
        .join(format!("ai-os-word-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&root)
        .map_err(|_| WordError::execution("Unable to create the Word container workspace"))?;
    Ok((root.join(format!("output.{extension}")), root))
}

fn publish_cache_output(cache_output: &Path, destination: &Path) -> Result<(), WordError> {
    if destination.exists() {
        return Err(WordError::invalid(
            "destination already exists; overwrite is not permitted",
        ));
    }
    fs::rename(cache_output, destination).map_err(|_| {
        WordError::execution("Unable to move the Word output to the requested destination")
    })
}

const READ_SCRIPT: &str = r#"
on run argv
    set stagedPath to item 1 of argv
    set stagedAlias to POSIX file stagedPath as alias
    set beforeDocumentCount to 0
    set ownsActiveDocument to false
    tell application id "com.microsoft.Word"
        activate
        try
            set beforeDocumentCount to count of documents
            open stagedAlias confirm conversions false read only true add to recent files false
            repeat 100 times
                if (count of documents) > beforeDocumentCount then exit repeat
                delay 0.1
            end repeat
            if (count of documents) is not (beforeDocumentCount + 1) then error "Word document count did not increase deterministically"
            if (posix full name of active document as text) is not stagedPath then error "Word active document identity did not match the operation copy"
            set ownsActiveDocument to true
            set documentText to content of text object of active document
            set paragraphCount to count of paragraphs of active document
            set tableCount to count of tables of active document
            set imageCount to count of inline shapes of active document
            if (posix full name of active document as text) is not stagedPath then error "Word active document ownership changed before close"
            close active document saving no
            set ownsActiveDocument to false
            repeat 100 times
                if (count of documents) is beforeDocumentCount then exit repeat
                delay 0.1
            end repeat
            if (count of documents) is not beforeDocumentCount then error "Word document count was not restored after close"
            return "AIOS_PARAGRAPHS=" & paragraphCount & linefeed & ¬
                "AIOS_TABLES=" & tableCount & linefeed & ¬
                "AIOS_IMAGES=" & imageCount & linefeed & ¬
                "AIOS_TEXT=" & documentText
        on error errorMessage number errorNumber
            if ownsActiveDocument then
                try
                    if (posix full name of active document as text) is stagedPath then close active document saving no
                end try
            end if
            error errorMessage number errorNumber
        end try
    end tell
end run
"#;

const CREATE_SCRIPT: &str = r#"
on run argv
    set outputPath to item 1 of argv
    set documentText to item 2 of argv
    set createdDocument to missing value
    set beforeDocumentCount to 0
    set ownsActiveDocument to false
    tell application id "com.microsoft.Word"
        activate
        try
            set beforeDocumentCount to count of documents
            set createdDocument to make new document
            set content of text object of createdDocument to documentText
            save as createdDocument file name outputPath file format format document default add to recent files false
            if (count of documents) is not (beforeDocumentCount + 1) then error "Word create document count changed unexpectedly"
            if (posix full name of active document as text) is not outputPath then error "Word created document identity did not match the operation output"
            set ownsActiveDocument to true
            close active document saving no
            set ownsActiveDocument to false
            repeat 100 times
                if (count of documents) is beforeDocumentCount then exit repeat
                delay 0.1
            end repeat
            if (count of documents) is not beforeDocumentCount then error "Word create document count was not restored after close"
            return "AIOS_WORD_CREATED"
        on error errorMessage number errorNumber
            if ownsActiveDocument then
                try
                    if (posix full name of active document as text) is outputPath then close active document saving no
                end try
            else if createdDocument is not missing value then
                try
                    close createdDocument saving no
                end try
            end if
            error errorMessage number errorNumber
        end try
    end tell
end run
"#;

fn parse_read(path: &str, output: &str) -> Result<Value, WordError> {
    let marker = "AIOS_TEXT=";
    let text_start = output
        .find(marker)
        .ok_or_else(|| WordError::execution("Word read returned no text marker"))?;
    let metadata = &output[..text_start];
    let mut paragraphs = 0_u64;
    let mut tables = 0_u64;
    let mut images = 0_u64;
    for line in metadata.lines() {
        if let Some(value) = line.strip_prefix("AIOS_PARAGRAPHS=") {
            paragraphs = value.parse().unwrap_or(0);
        }
        if let Some(value) = line.strip_prefix("AIOS_TABLES=") {
            tables = value.parse().unwrap_or(0);
        }
        if let Some(value) = line.strip_prefix("AIOS_IMAGES=") {
            images = value.parse().unwrap_or(0);
        }
    }
    let raw_text = &output[text_start + marker.len()..];
    let text: String = raw_text.chars().take(MAX_DOCUMENT_TEXT).collect();
    Ok(json!({
        "capability": "document.read",
        "selectedProvider": "microsoft-word",
        "resourceLocation": "local",
        "inputResource": path,
        "operationResult": { "text": text, "paragraphCount": paragraphs, "tableCount": tables, "imageCount": images },
        "warnings": if raw_text.chars().count() > MAX_DOCUMENT_TEXT { vec!["Document text was truncated at the deterministic extraction limit."] } else { Vec::<&str>::new() },
        "confirmationConsumed": true,
        "validationResult": "read"
    }))
}

pub(crate) fn read_word_document(input: &Value) -> Result<Value, WordError> {
    let path = require_path(input, "path", true)?;
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("docx");
    let (staged_input, cache_root) = word_cache_output(extension)?;
    fs::copy(path, &staged_input)
        .map_err(|_| WordError::execution("Unable to stage the Word document for reading"))?;
    let staged_string = staged_input.to_string_lossy().to_string();
    let result =
        run_osascript(READ_SCRIPT, &[&staged_string]).and_then(|output| parse_read(path, &output));
    let _ = fs::remove_dir_all(cache_root);
    result
}

pub(crate) fn create_word_document(input: &Value) -> Result<Value, WordError> {
    let path = require_path(input, "path", false)?;
    let title = input
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let body = input
        .get("body")
        .or_else(|| input.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let text = if title.is_empty() {
        body.to_owned()
    } else {
        format!("{title}\n{body}")
    };
    if text.is_empty() || text.chars().count() > MAX_CREATE_TEXT {
        return Err(WordError::invalid(
            "document.create requires bounded title/body text",
        ));
    }
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("docx");
    let (cache_output, cache_root) = word_cache_output(extension)?;
    let cache_string = cache_output.to_string_lossy().to_string();
    let create_result = run_osascript(CREATE_SCRIPT, &[&cache_string, &text]);
    if !cache_output.is_file() {
        let _ = fs::remove_dir_all(&cache_root);
        return Err(create_result.err().unwrap_or_else(|| {
            WordError::execution("Word create did not produce the requested document")
        }));
    }
    publish_cache_output(&cache_output, Path::new(path))?;
    let _ = fs::remove_dir_all(&cache_root);
    let read = read_word_document(&json!({"path": path}))?;
    Ok(json!({
        "capability": "document.create",
        "selectedProvider": "microsoft-word",
        "resourceLocation": "local",
        "outputResource": path,
        "operationResult": "created",
        "warnings": [],
        "confirmationConsumed": true,
        "validationResult": read["operationResult"]
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_paths_and_overwrite_fail_closed() {
        assert!(
            read_word_document(&json!({"path":"relative.docx"}))
                .unwrap_err()
                .invalid_request
        );
        assert!(
            create_word_document(&json!({"path":"relative.docx","body":"x"}))
                .unwrap_err()
                .invalid_request
        );
    }

    #[test]
    #[ignore = "requires Microsoft Word and macOS Automation authorization"]
    fn word_readback_real_e2e() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ai-os-word-readback-e2e-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let docx = root.join("AI-OS-Word-Readback-E2E.docx");
        let created = create_word_document(&json!({
            "path": docx,
            "title": "AI-OS Readback Contract",
            "body": "First paragraph\nSecond paragraph"
        }))
        .unwrap();
        assert_eq!(created["operationResult"], "created");
        let read = read_word_document(&json!({"path": docx})).unwrap();
        assert!(read["operationResult"]["text"]
            .as_str()
            .unwrap()
            .contains("Second paragraph"));
        fs::remove_dir_all(root).unwrap();
    }
}
