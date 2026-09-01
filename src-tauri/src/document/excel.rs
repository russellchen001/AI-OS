use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_ROW: u64 = 10_000;
const MAX_COLUMN: u64 = 256;
const MAX_OPERATIONS: usize = 32;
const MAX_CELL_TEXT: usize = 4_096;
const MAX_FORMULA_TEXT: usize = 512;
const RECORD_SEPARATOR: char = '\u{1e}';
const FIELD_SEPARATOR: char = '\u{1f}';

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExcelError {
    pub invalid_request: bool,
    pub message: String,
}

impl ExcelError {
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

#[derive(Debug, Clone, PartialEq)]
enum CellScalar {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
}

#[derive(Debug, Clone, PartialEq)]
enum EditOperation {
    SetCell {
        sheet: String,
        row: u64,
        column: u64,
        value: CellScalar,
    },
    SetFormula {
        sheet: String,
        row: u64,
        column: u64,
        formula: String,
    },
    ClearCell {
        sheet: String,
        row: u64,
        column: u64,
    },
}

fn workbook_path<'a>(
    input: &'a Value,
    field: &str,
    must_exist: bool,
) -> Result<&'a str, ExcelError> {
    let path = input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ExcelError::invalid(format!("spreadsheet.edit requires {field}")))?;
    let parsed = Path::new(path);
    if !parsed.is_absolute() {
        return Err(ExcelError::invalid(format!(
            "{field} must be an absolute path"
        )));
    }
    if parsed
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        != "xlsx"
    {
        return Err(ExcelError::invalid(
            "spreadsheet.edit Phase A requires XLSX input and output",
        ));
    }
    if must_exist && !parsed.is_file() {
        return Err(ExcelError::invalid(format!("{field} does not exist")));
    }
    if !must_exist && parsed.exists() {
        return Err(ExcelError::invalid(format!(
            "{field} already exists; overwrite is not permitted"
        )));
    }
    Ok(path)
}

fn validate_text(value: &str, maximum: usize, label: &str) -> Result<(), ExcelError> {
    if value.chars().count() > maximum
        || value.contains(RECORD_SEPARATOR)
        || value.contains(FIELD_SEPARATOR)
    {
        return Err(ExcelError::invalid(format!(
            "{label} is oversized or contains a reserved separator"
        )));
    }
    Ok(())
}

fn sheet_name(operation: &Value) -> Result<String, ExcelError> {
    let sheet = operation
        .get("sheet")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if sheet.is_empty()
        || sheet.chars().count() > 31
        || sheet
            .chars()
            .any(|character| "[]:*?/\\".contains(character))
    {
        return Err(ExcelError::invalid(
            "operation requires a valid worksheet name",
        ));
    }
    validate_text(sheet, 31, "worksheet name")?;
    Ok(sheet.to_owned())
}

fn coordinate(operation: &Value) -> Result<(u64, u64), ExcelError> {
    let row = operation.get("row").and_then(Value::as_u64).unwrap_or(0);
    let column = operation.get("column").and_then(Value::as_u64).unwrap_or(0);
    if !(1..=MAX_ROW).contains(&row) {
        return Err(ExcelError::invalid(format!(
            "row must be between 1 and {MAX_ROW}"
        )));
    }
    if !(1..=MAX_COLUMN).contains(&column) {
        return Err(ExcelError::invalid(format!(
            "column must be between 1 and {MAX_COLUMN}"
        )));
    }
    Ok((row, column))
}

fn scalar(value: &Value) -> Result<CellScalar, ExcelError> {
    if let Some(value) = value.as_str() {
        validate_text(value, MAX_CELL_TEXT, "cell value")?;
        return Ok(CellScalar::String(value.to_owned()));
    }
    if let Some(value) = value.as_i64() {
        return Ok(CellScalar::Integer(value));
    }
    if let Some(value) = value.as_f64() {
        if value.is_finite() {
            return Ok(CellScalar::Float(value));
        }
    }
    if let Some(value) = value.as_bool() {
        return Ok(CellScalar::Boolean(value));
    }
    Err(ExcelError::invalid(
        "set_cell value must be a string, integer, finite float, or boolean",
    ))
}

fn parse_operation(operation: &Value) -> Result<EditOperation, ExcelError> {
    let kind = operation.get("type").and_then(Value::as_str).unwrap_or("");
    let sheet = sheet_name(operation)?;
    let (row, column) = coordinate(operation)?;
    match kind {
        "set_cell" => Ok(EditOperation::SetCell {
            sheet,
            row,
            column,
            value: scalar(operation.get("value").unwrap_or(&Value::Null))?,
        }),
        "set_formula" => {
            let formula = operation
                .get("formula")
                .and_then(Value::as_str)
                .unwrap_or("");
            validate_text(formula, MAX_FORMULA_TEXT, "formula")?;
            if formula.len() < 2 || !formula.starts_with('=') {
                return Err(ExcelError::invalid(
                    "formula must begin with = and contain an expression",
                ));
            }
            Ok(EditOperation::SetFormula {
                sheet,
                row,
                column,
                formula: formula.to_owned(),
            })
        }
        "clear_cell" => Ok(EditOperation::ClearCell { sheet, row, column }),
        _ => Err(ExcelError::invalid(
            "spreadsheet.edit Phase A supports set_cell, set_formula, and clear_cell",
        )),
    }
}

fn parse_operations(input: &Value) -> Result<Vec<EditOperation>, ExcelError> {
    let operations = input
        .get("operations")
        .and_then(Value::as_array)
        .ok_or_else(|| ExcelError::invalid("spreadsheet.edit requires an operations array"))?;
    if operations.is_empty() || operations.len() > MAX_OPERATIONS {
        return Err(ExcelError::invalid(format!(
            "operation count must be between 1 and {MAX_OPERATIONS}"
        )));
    }
    operations.iter().map(parse_operation).collect()
}

fn column_name(mut column: u64) -> String {
    let mut name = String::new();
    while column > 0 {
        let remainder = ((column - 1) % 26) as u8;
        name.insert(0, (b'A' + remainder) as char);
        column = (column - 1) / 26;
    }
    name
}

fn encode_operations(operations: &[EditOperation]) -> String {
    operations
        .iter()
        .map(|operation| {
            let fields = match operation {
                EditOperation::SetCell {
                    sheet,
                    row,
                    column,
                    value,
                } => {
                    let (kind, rendered) = match value {
                        CellScalar::String(value) => ("string", value.clone()),
                        CellScalar::Integer(value) => ("integer", value.to_string()),
                        CellScalar::Float(value) => ("float", value.to_string()),
                        CellScalar::Boolean(value) => ("boolean", value.to_string()),
                    };
                    vec![
                        "set_cell".to_owned(),
                        sheet.clone(),
                        format!("{}{}", column_name(*column), row),
                        kind.to_owned(),
                        rendered,
                    ]
                }
                EditOperation::SetFormula {
                    sheet,
                    row,
                    column,
                    formula,
                } => vec![
                    "set_formula".to_owned(),
                    sheet.clone(),
                    format!("{}{}", column_name(*column), row),
                    formula.clone(),
                    String::new(),
                ],
                EditOperation::ClearCell { sheet, row, column } => {
                    let neighbor_column = if *column < MAX_COLUMN {
                        column + 1
                    } else {
                        column - 1
                    };
                    vec![
                        "clear_cell".to_owned(),
                        sheet.clone(),
                        format!("{}{}", column_name(*column), row),
                        format!("{}{}", column_name(neighbor_column), row),
                        String::new(),
                    ]
                }
            };
            fields.join(&FIELD_SEPARATOR.to_string())
        })
        .collect::<Vec<_>>()
        .join(&RECORD_SEPARATOR.to_string())
}

fn run_osascript(script: &str, args: &[&str]) -> Result<String, ExcelError> {
    let mut child = Command::new("/usr/bin/osascript")
        .arg("-")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| ExcelError::execution("Unable to start Microsoft Excel automation"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| ExcelError::execution("Unable to open Excel AppleScript input"))?
        .write_all(script.as_bytes())
        .map_err(|_| ExcelError::execution("Unable to write Excel AppleScript"))?;
    let output = child
        .wait_with_output()
        .map_err(|_| ExcelError::execution("Microsoft Excel automation did not complete"))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(ExcelError::execution(if error.is_empty() {
            "Microsoft Excel automation failed".to_owned()
        } else {
            format!("Microsoft Excel automation failed: {error}")
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn cache_workspace() -> Result<PathBuf, ExcelError> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| ExcelError::execution("Excel cache home is unavailable"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ExcelError::execution("System clock is unavailable"))?
        .as_nanos();
    let root = Path::new(&home)
        .join("Library/Containers/com.microsoft.Excel/Data/Library/Caches/com.microsoft.Excel")
        .join(format!("ai-os-excel-edit-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&root)
        .map_err(|_| ExcelError::execution("Unable to create Excel operation workspace"))?;
    Ok(root)
}

fn publish_without_overwrite(source: &Path, destination: &Path) -> Result<(), ExcelError> {
    let mut source_file = File::open(source)
        .map_err(|_| ExcelError::execution("Unable to open Excel save-copy output"))?;
    let mut destination_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| {
            if error.kind() == io::ErrorKind::AlreadyExists {
                ExcelError::invalid("destination already exists; overwrite is not permitted")
            } else {
                ExcelError::execution("Unable to create Excel destination")
            }
        })?;
    if io::copy(&mut source_file, &mut destination_file).is_err()
        || destination_file.sync_all().is_err()
    {
        let _ = fs::remove_file(destination);
        return Err(ExcelError::execution(
            "Unable to publish complete Excel output",
        ));
    }
    Ok(())
}

const EDIT_SCRIPT: &str = r#"
on splitText(sourceText, delimiterText)
    set oldDelimiters to AppleScript's text item delimiters
    set AppleScript's text item delimiters to delimiterText
    set resultItems to text items of sourceText
    set AppleScript's text item delimiters to oldDelimiters
    return resultItems
end splitText

on safeText(valueToRender)
    if valueToRender is missing value then return ""
    return valueToRender as text
end safeText

on run argv
    set originalPath to item 1 of argv
    set stagedPath to item 2 of argv
    set outputPath to item 3 of argv
    set originalName to item 4 of argv
    set stagedName to item 5 of argv
    set outputName to item 6 of argv
    set operationRecords to my splitText(item 7 of argv, ASCII character 30)
    set beforeBooks to 0
    set beforeWindows to 0
    set duringBooks to 0
    set duringWindows to 0
    set ownsWorkbook to false
    set phaseName to "preflight"
    set clearSnapshots to {}
    tell application "Microsoft Excel"
        try
            set beforeBooks to count of workbooks
            set beforeWindows to count of windows
            set existingNames to name of every workbook
            if originalName is in existingNames or stagedName is in existingNames or outputName is in existingNames then error "Workbook ownership conflict" number -2701
            set phaseName to "open"
            open workbook workbook file name stagedPath
            repeat 100 times
                if (count of workbooks) is (beforeBooks + 1) then exit repeat
                delay 0.1
            end repeat
            set duringBooks to count of workbooks
            set duringWindows to count of windows
            if duringBooks is not (beforeBooks + 1) then error "Workbook count did not increase deterministically"
            set freshWorkbook to active workbook
            if (full name of freshWorkbook as text) is not stagedPath then error "Fresh active input identity mismatch"
            set ownsWorkbook to true
            set phaseName to "edit"
            repeat with encodedRecord in operationRecords
                set fields to my splitText(contents of encodedRecord, ASCII character 31)
                set operationKind to item 1 of fields
                set sheetName to item 2 of fields
                set cellAddress to item 3 of fields
                if not (exists worksheet sheetName of freshWorkbook) then error "Worksheet not found: " & sheetName number -2702
                tell worksheet sheetName of freshWorkbook
                    if operationKind is "set_cell" then
                        set scalarKind to item 4 of fields
                        set scalarText to item 5 of fields
                        if scalarKind is "integer" then
                            set value of range cellAddress to scalarText as integer
                        else if scalarKind is "float" then
                            set value of range cellAddress to scalarText as real
                        else if scalarKind is "boolean" then
                            set value of range cellAddress to (scalarText is "true")
                        else
                            set value of range cellAddress to scalarText
                        end if
                    else if operationKind is "set_formula" then
                        set formula of range cellAddress to item 4 of fields
                    else if operationKind is "clear_cell" then
                        set neighborAddress to item 4 of fields
                        set end of clearSnapshots to my safeText(value of range neighborAddress)
                        clear contents range cellAddress
                    end if
                end tell
            end repeat
            set phaseName to "save-copy"
            save workbook as freshWorkbook filename outputPath file format Excel XML file format
            repeat 100 times
                try
                    if (full name of active workbook as text) is outputPath then exit repeat
                end try
                delay 0.1
            end repeat
            set phaseName to "fresh-post-save-ownership"
            set postSaveWorkbook to active workbook
            if (full name of postSaveWorkbook as text) is not outputPath then error "Fresh post-save identity mismatch"
            if (count of workbooks) is not (beforeBooks + 1) then error "Post-save workbook count changed"
            close postSaveWorkbook saving no
            set ownsWorkbook to false
            repeat 100 times
                if (count of workbooks) is beforeBooks then exit repeat
                delay 0.1
            end repeat
            if (count of workbooks) is not beforeBooks then error "Workbook count not restored after save-copy close"
            set phaseName to "reopen-validation"
            open workbook workbook file name outputPath
            repeat 100 times
                if (count of workbooks) is (beforeBooks + 1) then exit repeat
                delay 0.1
            end repeat
            if (count of workbooks) is not (beforeBooks + 1) then error "Output reopen count did not increase"
            set validationWorkbook to active workbook
            if (full name of validationWorkbook as text) is not outputPath then error "Output reopen identity mismatch"
            set ownsWorkbook to true
            set clearIndex to 1
            set validationText to ""
            repeat with encodedRecord in operationRecords
                set fields to my splitText(contents of encodedRecord, ASCII character 31)
                set operationKind to item 1 of fields
                set sheetName to item 2 of fields
                set cellAddress to item 3 of fields
                if not (exists worksheet sheetName of validationWorkbook) then error "Validation worksheet missing" number -2702
                tell worksheet sheetName of validationWorkbook
                    if operationKind is "set_cell" then
                        set scalarKind to item 4 of fields
                        set expectedText to item 5 of fields
                        set actualValue to value of range cellAddress
                        if scalarKind is "integer" or scalarKind is "float" then
                            if (actualValue as real) is not (expectedText as real) then error "SetCell numeric validation mismatch"
                        else if scalarKind is "boolean" then
                            if actualValue is not (expectedText is "true") then error "SetCell boolean validation mismatch"
                        else if (actualValue as text) is not expectedText then
                            error "SetCell string validation mismatch"
                        end if
                        set validationText to validationText & "set_cell:" & sheetName & "!" & cellAddress & "=" & my safeText(actualValue) & (ASCII character 30)
                    else if operationKind is "set_formula" then
                        set expectedFormula to item 4 of fields
                        set actualFormula to formula of range cellAddress as text
                        if actualFormula is not expectedFormula then error "SetFormula validation mismatch"
                        set calculatedValue to value of range cellAddress
                        set validationText to validationText & "set_formula:" & sheetName & "!" & cellAddress & "=" & actualFormula & "|calculated=" & my safeText(calculatedValue) & (ASCII character 30)
                    else if operationKind is "clear_cell" then
                        set neighborAddress to item 4 of fields
                        set clearedValue to my safeText(value of range cellAddress)
                        set clearedFormula to my safeText(formula of range cellAddress)
                        if clearedValue is not "" or clearedFormula is not "" then error "ClearCell validation mismatch"
                        if my safeText(value of range neighborAddress) is not item clearIndex of clearSnapshots then error "ClearCell changed adjacent cell"
                        set clearIndex to clearIndex + 1
                        set validationText to validationText & "clear_cell:" & sheetName & "!" & cellAddress & "=empty" & (ASCII character 30)
                    end if
                end tell
            end repeat
            close validationWorkbook saving no
            set ownsWorkbook to false
            repeat 100 times
                if (count of workbooks) is beforeBooks then exit repeat
                delay 0.1
            end repeat
            set afterBooks to count of workbooks
            set afterWindows to count of windows
            if afterBooks is not beforeBooks then error "Final workbook count not restored"
            if afterWindows is not beforeWindows then error "Final window count not restored"
            return "AIOS_EXCEL_EDIT_PASS" & linefeed & "AIOS_COUNTS=" & beforeBooks & "/" & duringBooks & "/" & afterBooks & linefeed & "AIOS_WINDOWS=" & beforeWindows & "/" & duringWindows & "/" & afterWindows & linefeed & "AIOS_VALIDATION=" & validationText
        on error errorMessage number errorNumber
            if ownsWorkbook then
                try
                    set activePath to full name of active workbook as text
                    if activePath is stagedPath or activePath is outputPath then close active workbook saving no
                end try
            end if
            error phaseName & ":" & errorNumber & ":" & errorMessage number errorNumber
        end try
    end tell
end run
"#;

pub(crate) fn edit_excel_workbook(input: &Value) -> Result<Value, ExcelError> {
    let source = workbook_path(input, "source", true)?;
    let destination = workbook_path(input, "destination", false)?;
    if source == destination {
        return Err(ExcelError::invalid(
            "source and destination must be different paths",
        ));
    }
    let operations = parse_operations(input)?;
    let encoded = encode_operations(&operations);
    let original = fs::read(source)
        .map_err(|_| ExcelError::execution("Unable to fingerprint source workbook"))?;
    let workspace = cache_workspace()?;
    let staged = workspace.join(format!("operation-{}.xlsx", uuid::Uuid::new_v4()));
    let output = workspace.join(format!("output-{}.xlsx", uuid::Uuid::new_v4()));
    fs::copy(source, &staged)
        .map_err(|_| ExcelError::execution("Unable to stage Excel input workbook"))?;
    let original_name = Path::new(source)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let staged_name = staged
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let output_name = output
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let staged_path = staged.to_string_lossy().to_string();
    let output_path = output.to_string_lossy().to_string();
    let automation = run_osascript(
        EDIT_SCRIPT,
        &[
            source,
            &staged_path,
            &output_path,
            original_name,
            staged_name,
            output_name,
            &encoded,
        ],
    );
    let automation_output = match automation {
        Ok(output_text) if output_text.starts_with("AIOS_EXCEL_EDIT_PASS") => output_text,
        Ok(_) => {
            let _ = fs::remove_dir_all(&workspace);
            return Err(ExcelError::execution(
                "Excel edit returned no success marker",
            ));
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&workspace);
            return Err(error);
        }
    };
    if !output
        .metadata()
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false)
    {
        let _ = fs::remove_dir_all(&workspace);
        return Err(ExcelError::execution(
            "Excel save-copy produced no non-empty output",
        ));
    }
    if fs::read(source).ok().as_deref() != Some(original.as_slice()) {
        let _ = fs::remove_dir_all(&workspace);
        return Err(ExcelError::execution(
            "Excel edit changed the original workbook",
        ));
    }
    if let Err(error) = publish_without_overwrite(&output, Path::new(destination)) {
        let _ = fs::remove_dir_all(&workspace);
        return Err(error);
    }
    let validation = automation_output
        .lines()
        .find_map(|line| line.strip_prefix("AIOS_VALIDATION="))
        .unwrap_or("")
        .split(RECORD_SEPARATOR)
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let counts = automation_output
        .lines()
        .find_map(|line| line.strip_prefix("AIOS_COUNTS="))
        .unwrap_or("");
    let windows = automation_output
        .lines()
        .find_map(|line| line.strip_prefix("AIOS_WINDOWS="))
        .unwrap_or("");
    let _ = fs::remove_dir_all(&workspace);
    Ok(json!({
        "capability": "spreadsheet.edit",
        "selectedProvider": "MicrosoftOffice",
        "resourceLocation": "Local",
        "inputResource": source,
        "outputResource": destination,
        "operationResult": {"status": "edited-copy", "operations": operations.len()},
        "validation": validation,
        "ownership": {"workbookCounts": counts, "windowCounts": windows, "postSaveReference": "fresh-active-workbook"},
        "warnings": [],
        "confirmationConsumed": true
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn request(operation: Value) -> Value {
        json!({"operations": [operation]})
    }

    #[test]
    fn phase_a_operations_validate_structured_inputs() {
        let set_cell = parse_operations(&request(
            json!({"type":"set_cell","sheet":"Sheet1","row":1,"column":1,"value":true}),
        ))
        .unwrap();
        assert!(matches!(
            set_cell[0],
            EditOperation::SetCell {
                value: CellScalar::Boolean(true),
                ..
            }
        ));
        assert!(parse_operations(&request(
            json!({"type":"clear_cell","sheet":"Sheet1","row":2,"column":3})
        ))
        .is_ok());
        for operation in [
            json!({"type":"set_cell","sheet":"Sheet1","row":0,"column":1,"value":1}),
            json!({"type":"set_cell","sheet":"Sheet1","row":1,"column":0,"value":1}),
            json!({"type":"set_cell","sheet":"Sheet1","row":10001,"column":1,"value":1}),
            json!({"type":"set_cell","sheet":"Sheet1","row":1,"column":257,"value":1}),
            json!({"type":"set_cell","sheet":"","row":1,"column":1,"value":1}),
        ] {
            assert!(parse_operations(&request(operation)).is_err());
        }
    }

    #[test]
    fn formulas_and_operation_bounds_fail_closed() {
        assert!(parse_operations(&request(
            json!({"type":"set_formula","sheet":"Sheet1","row":1,"column":1,"formula":"1+2"})
        ))
        .is_err());
        assert!(parse_operations(&request(json!({"type":"set_formula","sheet":"Sheet1","row":1,"column":1,"formula":format!("={}", "1".repeat(MAX_FORMULA_TEXT))}))).is_err());
        let operations = (0..=MAX_OPERATIONS)
            .map(|index| json!({"type":"clear_cell","sheet":"Sheet1","row":index + 1,"column":1}))
            .collect::<Vec<_>>();
        assert!(parse_operations(&json!({"operations": operations})).is_err());
    }

    #[test]
    fn paths_refuse_missing_unsupported_same_and_overwrite() {
        let root = tempdir().unwrap();
        let source = root.path().join("source.xlsx");
        let destination = root.path().join("destination.xlsx");
        fs::write(&source, b"fixture").unwrap();
        fs::write(&destination, b"existing").unwrap();
        let operation = json!({"type":"clear_cell","sheet":"Sheet1","row":1,"column":1});
        assert!(edit_excel_workbook(&json!({"source":root.path().join("missing.xlsx"),"destination":root.path().join("new.xlsx"),"operations":[operation.clone()]})).unwrap_err().invalid_request);
        assert!(
            edit_excel_workbook(
                &json!({"source":source,"destination":destination,"operations":[operation.clone()]})
            )
            .unwrap_err()
            .invalid_request
        );
        assert!(
            edit_excel_workbook(
                &json!({"source":source,"destination":source,"operations":[operation.clone()]})
            )
            .unwrap_err()
            .invalid_request
        );
        assert!(edit_excel_workbook(&json!({"source":root.path().join("source.csv"),"destination":root.path().join("new.xlsx"),"operations":[operation]})).unwrap_err().invalid_request);
    }

    #[test]
    #[ignore = "requires Microsoft Excel and AI_OS_EXCEL_EDIT_FIXTURE"]
    fn excel_phase_a_real_e2e() {
        let fixture = std::env::var("AI_OS_EXCEL_EDIT_FIXTURE").expect("preserved fixture path");
        let original = fs::read(&fixture).unwrap();
        let root = tempdir().unwrap();
        let output = root.path().join("phase-a-output.xlsx");
        let result = edit_excel_workbook(&json!({
            "source": fixture,
            "destination": output,
            "operations": [
                {"type":"set_cell","sheet":"Sheet1","row":2,"column":4,"value":12345},
                {"type":"set_formula","sheet":"Sheet1","row":3,"column":4,"formula":"=1+2"},
                {"type":"clear_cell","sheet":"Sheet1","row":2,"column":1}
            ]
        }))
        .unwrap();
        assert_eq!(result["operationResult"]["status"], "edited-copy");
        assert_eq!(result["ownership"]["workbookCounts"], "1/2/1");
        assert_eq!(result["ownership"]["windowCounts"], "1/2/1");
        assert_eq!(fs::read(&fixture).unwrap(), original);
        assert!(output.metadata().unwrap().len() > 0);
        let overwrite = edit_excel_workbook(&json!({
            "source": fixture,
            "destination": output,
            "operations": [{"type":"clear_cell","sheet":"Sheet1","row":1,"column":1}]
        }))
        .unwrap_err();
        assert!(overwrite.invalid_request);
        assert!(result["validation"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value
                .as_str()
                .unwrap()
                .contains("set_formula:Sheet1!D3==1+2|calculated=3")));
    }
}
