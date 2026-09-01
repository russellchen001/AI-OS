use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_DOCUMENT_TEXT: usize = 64 * 1024;
const MAX_CREATE_TEXT: usize = 32 * 1024;
const MAX_TABLE_ROWS: usize = 20;
const MAX_TABLE_COLUMNS: usize = 20;
const MAX_TABLE_CELL_TEXT: usize = 512;

#[derive(Debug, Clone, PartialEq)]
struct WordTableInput {
    rows: usize,
    columns: usize,
    cells: Vec<String>,
}

fn parse_table_input(input: &Value) -> Result<WordTableInput, WordError> {
    let table = input
        .get("table")
        .and_then(Value::as_object)
        .ok_or_else(|| WordError::invalid("document.edit requires a table operation"))?;
    let rows = table.get("rows").and_then(Value::as_u64).unwrap_or(0) as usize;
    let columns = table.get("columns").and_then(Value::as_u64).unwrap_or(0) as usize;
    if rows == 0 || rows > MAX_TABLE_ROWS {
        return Err(WordError::invalid(format!(
            "table rows must be between 1 and {MAX_TABLE_ROWS}"
        )));
    }
    if columns == 0 || columns > MAX_TABLE_COLUMNS {
        return Err(WordError::invalid(format!(
            "table columns must be between 1 and {MAX_TABLE_COLUMNS}"
        )));
    }
    let cells = table
        .get("cells")
        .and_then(Value::as_array)
        .ok_or_else(|| WordError::invalid("table cells must be an array"))?
        .iter()
        .map(|cell| {
            cell.as_str()
                .filter(|text| {
                    !text.is_empty()
                        && text.chars().count() <= MAX_TABLE_CELL_TEXT
                        && !text.contains('\u{1e}')
                })
                .map(str::to_owned)
                .ok_or_else(|| WordError::invalid("table cell text is invalid or too large"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if cells.len() != rows * columns {
        return Err(WordError::invalid(
            "table cell count must equal rows multiplied by columns",
        ));
    }
    Ok(WordTableInput {
        rows,
        columns,
        cells,
    })
}

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
            set headingBold to bold of text object of paragraph 1 of active document
            set headingItalic to italic of text object of paragraph 1 of active document
            set headingFontSize to font size of font object of text object of paragraph 1 of active document
            set tableCells to ""
            if tableCount > 0 then
                set firstTable to table 1 of active document
                repeat with rowIndex from 1 to number of rows of firstTable
                    repeat with columnIndex from 1 to number of columns of firstTable
                        set tableCells to tableCells & (content of text object of (get cell from table firstTable row rowIndex column columnIndex)) & (ASCII character 30)
                    end repeat
                end repeat
            end if
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
                "AIOS_HEADING_BOLD=" & headingBold & linefeed & ¬
                "AIOS_HEADING_ITALIC=" & headingItalic & linefeed & ¬
                "AIOS_HEADING_FONT_SIZE=" & headingFontSize & linefeed & ¬
                "AIOS_TABLE_CELLS=" & tableCells & linefeed & ¬
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

const EDIT_SCRIPT: &str = r#"
on splitCells(encodedCells)
    set previousDelimiters to AppleScript's text item delimiters
    set AppleScript's text item delimiters to ASCII character 30
    set cellItems to text items of encodedCells
    set AppleScript's text item delimiters to previousDelimiters
    return cellItems
end splitCells

on run argv
    set stagedPath to item 1 of argv
    set outputPath to item 2 of argv
    set paragraphIndex to (item 3 of argv) as integer
    set paragraphText to item 4 of argv
    set headingIndex to (item 5 of argv) as integer
    set headingBold to (item 6 of argv) is "true"
    set headingItalic to (item 7 of argv) is "true"
    set headingFontSize to (item 8 of argv) as real
    set tableRows to (item 9 of argv) as integer
    set tableColumns to (item 10 of argv) as integer
    set cellItems to my splitCells(item 11 of argv)
    set stagedAlias to POSIX file stagedPath as alias
    set beforeDocumentCount to 0
    set ownsActiveDocument to false
    tell application id "com.microsoft.Word"
        activate
        try
            set beforeDocumentCount to count of documents
            open stagedAlias confirm conversions false read only false add to recent files false
            repeat 100 times
                if (count of documents) > beforeDocumentCount then exit repeat
                delay 0.1
            end repeat
            if (count of documents) is not (beforeDocumentCount + 1) then error "Word edit document count did not increase deterministically"
            if (posix full name of active document as text) is not stagedPath then error "Word edit active document identity did not match the operation copy"
            set ownsActiveDocument to true
            if paragraphIndex > (count of paragraphs of active document) then error "Word edit paragraph index is out of range"
            set content of text object of paragraph paragraphIndex of active document to paragraphText & return
            if headingIndex > (count of paragraphs of active document) then error "Word heading paragraph index is out of range"
            set headingRange to text object of paragraph headingIndex of active document
            set bold of headingRange to headingBold
            set italic of headingRange to headingItalic
            set font size of font object of headingRange to headingFontSize
            set insertionPoint to count of characters of (content of text object of active document)
            set selection start of selection to insertionPoint
            set selection end of selection to insertionPoint
            set insertedTable to make new table at text object of selection with properties {number of rows:tableRows, number of columns:tableColumns}
            set cellIndex to 1
            repeat with rowIndex from 1 to tableRows
                repeat with columnIndex from 1 to tableColumns
                    set content of text object of (get cell from table insertedTable row rowIndex column columnIndex) to item cellIndex of cellItems
                    set cellIndex to cellIndex + 1
                end repeat
            end repeat
            save as active document file name outputPath file format format document default add to recent files false
            if (posix full name of active document as text) is not outputPath then error "Word edit output identity did not match save-copy path"
            close active document saving no
            set ownsActiveDocument to false
            repeat 100 times
                if (count of documents) is beforeDocumentCount then exit repeat
                delay 0.1
            end repeat
            if (count of documents) is not beforeDocumentCount then error "Word edit document count was not restored after close"
            return "AIOS_WORD_EDITED"
        on error errorMessage number errorNumber
            if ownsActiveDocument then
                try
                    set activePath to posix full name of active document as text
                    if activePath is stagedPath or activePath is outputPath then close active document saving no
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
    let mut heading_bold = false;
    let mut heading_italic = false;
    let mut heading_font_size = 0.0_f64;
    let mut table_cells = Vec::new();
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
        if let Some(value) = line.strip_prefix("AIOS_HEADING_BOLD=") {
            heading_bold = value == "true";
        }
        if let Some(value) = line.strip_prefix("AIOS_HEADING_ITALIC=") {
            heading_italic = value == "true";
        }
        if let Some(value) = line.strip_prefix("AIOS_HEADING_FONT_SIZE=") {
            heading_font_size = value.parse().unwrap_or(0.0);
        }
        if let Some(value) = line.strip_prefix("AIOS_TABLE_CELLS=") {
            table_cells = value
                .split('\u{1e}')
                .map(|cell| cell.trim_end_matches(['\r', '\u{7}']).to_owned())
                .filter(|cell| !cell.is_empty())
                .collect();
        }
    }
    let raw_text = &output[text_start + marker.len()..];
    let text: String = raw_text.chars().take(MAX_DOCUMENT_TEXT).collect();
    Ok(json!({
        "capability": "document.read",
        "selectedProvider": "microsoft-word",
        "resourceLocation": "local",
        "inputResource": path,
        "operationResult": {
            "text": text,
            "paragraphCount": paragraphs,
            "tableCount": tables,
            "imageCount": images,
            "headingBold": heading_bold,
            "headingItalic": heading_italic,
            "headingFontSize": heading_font_size,
            "tableCells": table_cells
        },
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

pub(crate) fn edit_word_document(input: &Value) -> Result<Value, WordError> {
    let source = require_path(input, "source", true)?;
    let destination = require_path(input, "destination", false)?;
    let source_extension = Path::new(source)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let destination_extension = Path::new(destination)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(source_extension.as_str(), "doc" | "docx") || destination_extension != "docx" {
        return Err(WordError::invalid(
            "document.edit requires a DOC/DOCX source and DOCX destination",
        ));
    }
    let paragraph_index = input
        .get("paragraphIndex")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let paragraph_text = input
        .get("paragraphText")
        .and_then(Value::as_str)
        .unwrap_or("");
    let heading_index = input
        .get("headingIndex")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let heading_bold = input
        .get("headingBold")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let heading_italic = input
        .get("headingItalic")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let heading_font_size = input
        .get("headingFontSize")
        .and_then(Value::as_f64)
        .unwrap_or(18.0);
    if paragraph_index == 0
        || heading_index == 0
        || paragraph_text.is_empty()
        || paragraph_text.chars().count() > MAX_TABLE_CELL_TEXT
        || !(8.0..=72.0).contains(&heading_font_size)
    {
        return Err(WordError::invalid(
            "document.edit paragraph and heading operation is invalid or unbounded",
        ));
    }
    let table = parse_table_input(input)?;
    let (staged_input, input_root) = word_cache_output(&source_extension)?;
    let (cache_output, output_root) = word_cache_output("docx")?;
    fs::copy(source, &staged_input)
        .map_err(|_| WordError::execution("Unable to stage the Word document for editing"))?;
    let staged_string = staged_input.to_string_lossy().to_string();
    let cache_string = cache_output.to_string_lossy().to_string();
    let cell_payload = table.cells.join("\u{1e}");
    let edit_result = run_osascript(
        EDIT_SCRIPT,
        &[
            &staged_string,
            &cache_string,
            &paragraph_index.to_string(),
            paragraph_text,
            &heading_index.to_string(),
            if heading_bold { "true" } else { "false" },
            if heading_italic { "true" } else { "false" },
            &heading_font_size.to_string(),
            &table.rows.to_string(),
            &table.columns.to_string(),
            &cell_payload,
        ],
    );
    if !cache_output.is_file() {
        let _ = fs::remove_dir_all(&input_root);
        let _ = fs::remove_dir_all(&output_root);
        return Err(edit_result.err().unwrap_or_else(|| {
            WordError::execution("Word edit did not produce the requested save-copy output")
        }));
    }
    publish_cache_output(&cache_output, Path::new(destination))?;
    let _ = fs::remove_dir_all(&input_root);
    let _ = fs::remove_dir_all(&output_root);
    let read = read_word_document(&json!({"path": destination}))?;
    let operation = &read["operationResult"];
    let validated_cells = operation["tableCells"]
        .as_array()
        .map(|cells| {
            cells
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !operation["text"]
        .as_str()
        .unwrap_or("")
        .contains(paragraph_text)
        || operation["tableCount"].as_u64().unwrap_or(0) == 0
        || validated_cells != table.cells
        || operation["headingBold"].as_bool() != Some(heading_bold)
        || operation["headingItalic"].as_bool() != Some(heading_italic)
        || (operation["headingFontSize"].as_f64().unwrap_or(0.0) - heading_font_size).abs() > 0.1
    {
        return Err(WordError::execution(
            "Word edit read-back validation did not match the structured operations",
        ));
    }
    Ok(json!({
        "capability": "document.edit",
        "selectedProvider": "microsoft-word",
        "resourceLocation": "local",
        "inputResource": source,
        "outputResource": destination,
        "operationResult": "edited-copy",
        "warnings": [],
        "confirmationConsumed": true,
        "validationResult": operation
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_table() -> Value {
        json!({"table":{"rows":2,"columns":2,"cells":["A1","B1","A2","B2"]}})
    }

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
    fn table_validation_is_bounded_and_exact() {
        assert_eq!(parse_table_input(&valid_table()).unwrap().cells.len(), 4);
        for invalid in [
            json!({"table":{"rows":0,"columns":2,"cells":[]}}),
            json!({"table":{"rows":2,"columns":0,"cells":[]}}),
            json!({"table":{"rows":21,"columns":2,"cells":[]}}),
            json!({"table":{"rows":2,"columns":21,"cells":[]}}),
            json!({"table":{"rows":2,"columns":2,"cells":["A1"]}}),
        ] {
            assert!(parse_table_input(&invalid).unwrap_err().invalid_request);
        }
    }

    #[test]
    fn edit_validation_refuses_existing_output() {
        let root =
            std::env::temp_dir().join(format!("ai-os-word-edit-validation-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.docx");
        let destination = root.join("destination.docx");
        fs::write(&source, b"fixture").unwrap();
        fs::write(&destination, b"existing").unwrap();
        let error = edit_word_document(&json!({
            "source": source,
            "destination": destination,
            "paragraphIndex": 3,
            "paragraphText": "Changed",
            "headingIndex": 1,
            "table":{"rows":2,"columns":2,"cells":["A1","B1","A2","B2"]}
        }))
        .unwrap_err();
        assert!(error.invalid_request);
        fs::remove_dir_all(root).unwrap();
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

    #[test]
    #[ignore = "requires Microsoft Word and macOS Automation authorization"]
    fn word_edit_table_real_e2e() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ai-os-word-edit-table-e2e-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("Word-Edit-Input.docx");
        let destination = root.join("Word-Edit-Output.docx");
        let before_state = run_osascript(
            "tell application id \"com.microsoft.Word\" to return (count of documents) as text",
            &[],
        )
        .unwrap();
        create_word_document(&json!({
            "path": source,
            "title": "AI-OS Word Edit",
            "body": "First paragraph\nSecond paragraph\nThird paragraph"
        }))
        .unwrap();
        let edited = edit_word_document(&json!({
            "source": source,
            "destination": destination,
            "paragraphIndex": 4,
            "paragraphText": "Third paragraph changed",
            "headingIndex": 1,
            "headingBold": true,
            "headingItalic": true,
            "headingFontSize": 18.0,
            "table":{"rows":2,"columns":2,"cells":["A1","B1","A2","B2"]}
        }))
        .unwrap();
        assert_eq!(edited["operationResult"], "edited-copy");
        let source_read = read_word_document(&json!({"path": source})).unwrap();
        assert!(!source_read["operationResult"]["text"]
            .as_str()
            .unwrap()
            .contains("Third paragraph changed"));
        let output_read = read_word_document(&json!({"path": destination})).unwrap();
        assert_eq!(output_read["operationResult"]["tableCount"], 1);
        assert_eq!(
            output_read["operationResult"]["tableCells"],
            json!(["A1", "B1", "A2", "B2"])
        );
        assert_eq!(output_read["operationResult"]["headingBold"], true);
        assert_eq!(output_read["operationResult"]["headingItalic"], true);
        assert_eq!(
            run_osascript(
                "tell application id \"com.microsoft.Word\" to return (count of documents) as text",
                &[],
            )
            .unwrap(),
            before_state
        );
        fs::remove_dir_all(root).unwrap();
    }
}
