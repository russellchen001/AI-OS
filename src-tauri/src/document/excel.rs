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
    AddWorksheet {
        name: String,
    },
    RenameWorksheet {
        sheet: String,
        name: String,
    },
    DeleteWorksheet {
        sheet: String,
    },
    InsertRow {
        sheet: String,
        row: u64,
    },
    DeleteRow {
        sheet: String,
        row: u64,
    },
    InsertColumn {
        sheet: String,
        column: u64,
    },
    DeleteColumn {
        sheet: String,
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
            "spreadsheet.edit requires XLSX input and output",
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

fn validate_worksheet_name(value: &str, label: &str) -> Result<String, ExcelError> {
    let value = value.trim();

    if value.is_empty()
        || value.chars().count() > 31
        || value
            .chars()
            .any(|character| "[]:*?/\\\\".contains(character))
    {
        return Err(ExcelError::invalid(format!(
            "{label} requires a valid worksheet name"
        )));
    }

    validate_text(value, 31, label)?;
    Ok(value.to_owned())
}

fn worksheet_field(operation: &Value, field: &str) -> Result<String, ExcelError> {
    let value = operation.get(field).and_then(Value::as_str).unwrap_or("");

    validate_worksheet_name(value, field)
}

fn sheet_name(operation: &Value) -> Result<String, ExcelError> {
    worksheet_field(operation, "sheet")
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

/// A single bounded 1-based row or column index.
///
/// Structural operations address one whole line at a time: there is no `count`
/// parameter, so a caller asking for several must send several operations and
/// each is validated on its own.
fn line_index(operation: &Value, field: &str, maximum: u64) -> Result<u64, ExcelError> {
    let value = operation.get(field).and_then(Value::as_u64).unwrap_or(0);
    if !(1..=maximum).contains(&value) {
        return Err(ExcelError::invalid(format!(
            "{field} must be between 1 and {maximum}"
        )));
    }
    Ok(value)
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

    match kind {
        "set_cell" => {
            let sheet = sheet_name(operation)?;
            let (row, column) = coordinate(operation)?;

            Ok(EditOperation::SetCell {
                sheet,
                row,
                column,
                value: scalar(operation.get("value").unwrap_or(&Value::Null))?,
            })
        }
        "set_formula" => {
            let sheet = sheet_name(operation)?;
            let (row, column) = coordinate(operation)?;

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
        "clear_cell" => {
            let sheet = sheet_name(operation)?;
            let (row, column) = coordinate(operation)?;

            Ok(EditOperation::ClearCell { sheet, row, column })
        }
        "add_worksheet" => Ok(EditOperation::AddWorksheet {
            name: worksheet_field(operation, "name")?,
        }),
        "rename_worksheet" => {
            let sheet = sheet_name(operation)?;
            let name = worksheet_field(operation, "name")?;

            if sheet == name {
                return Err(ExcelError::invalid(
                    "rename_worksheet source and destination names must differ",
                ));
            }

            Ok(EditOperation::RenameWorksheet { sheet, name })
        }
        "delete_worksheet" => Ok(EditOperation::DeleteWorksheet {
            sheet: sheet_name(operation)?,
        }),
        "insert_row" => Ok(EditOperation::InsertRow {
            sheet: sheet_name(operation)?,
            row: line_index(operation, "row", MAX_ROW)?,
        }),
        "delete_row" => Ok(EditOperation::DeleteRow {
            sheet: sheet_name(operation)?,
            row: line_index(operation, "row", MAX_ROW)?,
        }),
        "insert_column" => Ok(EditOperation::InsertColumn {
            sheet: sheet_name(operation)?,
            column: line_index(operation, "column", MAX_COLUMN)?,
        }),
        "delete_column" => Ok(EditOperation::DeleteColumn {
            sheet: sheet_name(operation)?,
            column: line_index(operation, "column", MAX_COLUMN)?,
        }),
        _ => Err(ExcelError::invalid(
            "spreadsheet.edit supports set_cell, set_formula, clear_cell, add_worksheet, rename_worksheet, delete_worksheet, insert_row, delete_row, insert_column, and delete_column",
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

/// The two cells whose reopened contents prove a structural change actually
/// shifted data: the line that was operated on, and the one after it.
fn row_probe_cells(row: u64) -> (String, String) {
    let following = if row < MAX_ROW { row + 1 } else { row - 1 };
    (format!("A{row}"), format!("A{following}"))
}

fn column_probe_cells(column: u64) -> (String, String) {
    let following = if column < MAX_COLUMN { column + 1 } else { column - 1 };
    (
        format!("{}1", column_name(column)),
        format!("{}1", column_name(following)),
    )
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
                EditOperation::AddWorksheet { name } => vec![
                    "add_worksheet".to_owned(),
                    name.clone(),
                    String::new(),
                    String::new(),
                    String::new(),
                ],
                EditOperation::RenameWorksheet { sheet, name } => vec![
                    "rename_worksheet".to_owned(),
                    sheet.clone(),
                    name.clone(),
                    String::new(),
                    String::new(),
                ],
                EditOperation::DeleteWorksheet { sheet } => vec![
                    "delete_worksheet".to_owned(),
                    sheet.clone(),
                    String::new(),
                    String::new(),
                    String::new(),
                ],
                // Whole-line references. A real Excel 16.78 probe showed the
                // plain `insert into range` / `delete range` forms shift
                // content correctly on their own, and that the `shift`
                // parameter changes nothing, so it is not sent.
                EditOperation::InsertRow { sheet, row } => {
                    let (at, next) = row_probe_cells(*row);
                    vec![
                        "insert_row".to_owned(),
                        sheet.clone(),
                        format!("{row}:{row}"),
                        at,
                        next,
                    ]
                }
                EditOperation::DeleteRow { sheet, row } => {
                    let (at, next) = row_probe_cells(*row);
                    vec![
                        "delete_row".to_owned(),
                        sheet.clone(),
                        format!("{row}:{row}"),
                        at,
                        next,
                    ]
                }
                EditOperation::InsertColumn { sheet, column } => {
                    let reference = column_name(*column);
                    let (at, next) = column_probe_cells(*column);
                    vec![
                        "insert_column".to_owned(),
                        sheet.clone(),
                        format!("{reference}:{reference}"),
                        at,
                        next,
                    ]
                }
                EditOperation::DeleteColumn { sheet, column } => {
                    let reference = column_name(*column);
                    let (at, next) = column_probe_cells(*column);
                    vec![
                        "delete_column".to_owned(),
                        sheet.clone(),
                        format!("{reference}:{reference}"),
                        at,
                        next,
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

on listContains(sourceList, targetText)
    repeat with candidateValue in sourceList
        if (contents of candidateValue as text) is targetText then return true
    end repeat

    return false
end listContains

on worksheetNames(targetWorkbook)
    tell application "Microsoft Excel"
        set resultList to {}

        repeat with sheetIndex from 1 to count of worksheets of targetWorkbook
            set candidateSheet to worksheet sheetIndex of targetWorkbook
            set end of resultList to (name of candidateSheet as text)
        end repeat

        return resultList
    end tell
end worksheetNames

on sameNameSet(leftNames, rightNames)
    if (count of leftNames) is not (count of rightNames) then return false

    repeat with candidateName in leftNames
        if not my listContains(rightNames, contents of candidateName as text) then
            return false
        end if
    end repeat

    return true
end sameNameSet

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
    set structuralSnapshots to {}
    set finalWorksheetNames to {}

    tell application "Microsoft Excel"
        try
            set beforeBooks to count of workbooks
            set beforeWindows to count of windows

            set existingNames to name of every workbook

            if originalName is in existingNames or stagedName is in existingNames or outputName is in existingNames then
                error "Workbook ownership conflict" number -2701
            end if

            set phaseName to "open"

            open workbook workbook file name stagedPath

            repeat 100 times
                if (count of workbooks) is (beforeBooks + 1) then exit repeat
                delay 0.1
            end repeat

            set duringBooks to count of workbooks
            set duringWindows to count of windows

            if duringBooks is not (beforeBooks + 1) then
                error "Workbook count did not increase deterministically"
            end if

            set freshWorkbook to active workbook

            if (full name of freshWorkbook as text) is not stagedPath then
                error "Fresh active input identity mismatch"
            end if

            set ownsWorkbook to true
            set phaseName to "edit"

            repeat with encodedRecord in operationRecords
                set fields to my splitText(contents of encodedRecord, ASCII character 31)
                set operationKind to item 1 of fields

                if operationKind is "set_cell" then
                    set sheetName to item 2 of fields
                    set cellAddress to item 3 of fields
                    set scalarKind to item 4 of fields
                    set scalarText to item 5 of fields

                    if not (exists worksheet sheetName of freshWorkbook) then
                        error "Worksheet not found: " & sheetName number -2702
                    end if

                    tell worksheet sheetName of freshWorkbook
                        if scalarKind is "integer" then
                            set value of range cellAddress to scalarText as integer
                        else if scalarKind is "float" then
                            set value of range cellAddress to scalarText as real
                        else if scalarKind is "boolean" then
                            set value of range cellAddress to (scalarText is "true")
                        else
                            set value of range cellAddress to scalarText
                        end if
                    end tell

                else if operationKind is "set_formula" then
                    set sheetName to item 2 of fields
                    set cellAddress to item 3 of fields

                    if not (exists worksheet sheetName of freshWorkbook) then
                        error "Worksheet not found: " & sheetName number -2702
                    end if

                    tell worksheet sheetName of freshWorkbook
                        set formula of range cellAddress to item 4 of fields
                    end tell

                else if operationKind is "clear_cell" then
                    set sheetName to item 2 of fields
                    set cellAddress to item 3 of fields
                    set neighborAddress to item 4 of fields

                    if not (exists worksheet sheetName of freshWorkbook) then
                        error "Worksheet not found: " & sheetName number -2702
                    end if

                    tell worksheet sheetName of freshWorkbook
                        set end of clearSnapshots to my safeText(value of range neighborAddress)
                        clear contents range cellAddress
                    end tell

                else if operationKind is "add_worksheet" then
                    set requestedName to item 2 of fields

                    if exists worksheet requestedName of freshWorkbook then
                        error "Duplicate worksheet: " & requestedName number -2703
                    end if

                    set namesBefore to my worksheetNames(freshWorkbook)

                    make new worksheet at freshWorkbook
                    set freshWorkbook to active workbook

                    set namesAfter to my worksheetNames(freshWorkbook)
                    set discoveredNames to {}

                    repeat with candidateName in namesAfter
                        set candidateText to contents of candidateName as text

                        if not my listContains(namesBefore, candidateText) then
                            set end of discoveredNames to candidateText
                        end if
                    end repeat

                    if (count of discoveredNames) is not 1 then
                        error "Unable to identify exactly one newly added worksheet" number -2704
                    end if

                    set generatedName to item 1 of discoveredNames

                    if not (exists worksheet generatedName of freshWorkbook) then
                        error "New worksheet identity lookup failed" number -2705
                    end if

                    set name of worksheet generatedName of freshWorkbook to requestedName
                    set freshWorkbook to active workbook

                    if not (exists worksheet requestedName of freshWorkbook) then
                        error "Added worksheet rename validation failed" number -2706
                    end if

                else if operationKind is "rename_worksheet" then
                    set sourceName to item 2 of fields
                    set requestedName to item 3 of fields

                    if not (exists worksheet sourceName of freshWorkbook) then
                        error "Worksheet not found: " & sourceName number -2702
                    end if

                    if exists worksheet requestedName of freshWorkbook then
                        error "Duplicate worksheet: " & requestedName number -2703
                    end if

                    set name of worksheet sourceName of freshWorkbook to requestedName
                    set freshWorkbook to active workbook

                    if exists worksheet sourceName of freshWorkbook then
                        error "Old worksheet name still exists after rename" number -2707
                    end if

                    if not (exists worksheet requestedName of freshWorkbook) then
                        error "Renamed worksheet not found" number -2708
                    end if

                else if operationKind is "delete_worksheet" then
                    set sourceName to item 2 of fields

                    if not (exists worksheet sourceName of freshWorkbook) then
                        error "Worksheet not found: " & sourceName number -2702
                    end if

                    if (count of worksheets of freshWorkbook) is 1 then
                        error "Cannot delete the final worksheet" number -2709
                    end if

                    set namesBeforeDelete to my worksheetNames(freshWorkbook)

                    delete worksheet sourceName of freshWorkbook
                    set freshWorkbook to active workbook

                    if exists worksheet sourceName of freshWorkbook then
                        error "Deleted worksheet still exists" number -2710
                    end if

                    repeat with preservedName in namesBeforeDelete
                        set preservedText to contents of preservedName as text

                        if preservedText is not sourceName then
                            if not (exists worksheet preservedText of freshWorkbook) then
                                error "Delete affected another worksheet" number -2711
                            end if
                        end if
                    end repeat

                else if operationKind is "insert_row" or operationKind is "delete_row" or operationKind is "insert_column" or operationKind is "delete_column" then
                    set sheetName to item 2 of fields
                    set lineReference to item 3 of fields

                    if not (exists worksheet sheetName of freshWorkbook) then
                        error "Worksheet not found: " & sheetName number -2702
                    end if

                    -- Exactly the form a real Excel 16.78 probe proved: an
                    -- explicit `range "B:B" of <sheet>`, with no shift
                    -- parameter, which the probe showed changes nothing. The
                    -- used range before and after is recorded so a structural
                    -- change that was silently ignored is visible.
                    set structuralSheet to worksheet sheetName of freshWorkbook
                    set usedBefore to my safeText(get address of used range of structuralSheet)

                    if operationKind is "insert_row" or operationKind is "insert_column" then
                        insert into range (range lineReference of structuralSheet)
                    else
                        delete range (range lineReference of structuralSheet)
                    end if

                    set freshWorkbook to active workbook

                    if not (exists worksheet sheetName of freshWorkbook) then
                        error "Worksheet vanished during a structural change" number -2712
                    end if

                    set usedAfter to my safeText(get address of used range of (worksheet sheetName of freshWorkbook))

                    set end of structuralSnapshots to operationKind & ":" & sheetName & "!" & lineReference & ":" & usedBefore & ">" & usedAfter
                end if
            end repeat

            set finalWorksheetNames to my worksheetNames(freshWorkbook)

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

            if (full name of postSaveWorkbook as text) is not outputPath then
                error "Fresh post-save identity mismatch"
            end if

            if (count of workbooks) is not (beforeBooks + 1) then
                error "Post-save workbook count changed"
            end if

            close postSaveWorkbook saving no
            set ownsWorkbook to false

            repeat 100 times
                if (count of workbooks) is beforeBooks then exit repeat
                delay 0.1
            end repeat

            if (count of workbooks) is not beforeBooks then
                error "Workbook count not restored after save-copy close"
            end if

            set phaseName to "reopen-validation"

            open workbook workbook file name outputPath

            repeat 100 times
                if (count of workbooks) is (beforeBooks + 1) then exit repeat
                delay 0.1
            end repeat

            if (count of workbooks) is not (beforeBooks + 1) then
                error "Output reopen count did not increase"
            end if

            set validationWorkbook to active workbook

            if (full name of validationWorkbook as text) is not outputPath then
                error "Output reopen identity mismatch"
            end if

            set ownsWorkbook to true
            set validationText to ""

            set reopenedWorksheetNames to my worksheetNames(validationWorkbook)

            if not my sameNameSet(finalWorksheetNames, reopenedWorksheetNames) then
                error "Final worksheet identity set mismatch after reopen" number -2712
            end if

            set validationText to validationText & "final_worksheets=validated" & (ASCII character 30)

            repeat with encodedRecord in operationRecords
                set fields to my splitText(contents of encodedRecord, ASCII character 31)
                set operationKind to item 1 of fields

                if operationKind is "set_cell" then
                    set sheetName to item 2 of fields
                    set cellAddress to item 3 of fields
                    set scalarKind to item 4 of fields
                    set expectedText to item 5 of fields

                    if exists worksheet sheetName of validationWorkbook then
                        tell worksheet sheetName of validationWorkbook
                            set actualValue to value of range cellAddress

                            if scalarKind is "integer" or scalarKind is "float" then
                                if (actualValue as real) is not (expectedText as real) then
                                    error "SetCell numeric validation mismatch"
                                end if
                            else if scalarKind is "boolean" then
                                if actualValue is not (expectedText is "true") then
                                    error "SetCell boolean validation mismatch"
                                end if
                            else if (actualValue as text) is not expectedText then
                                error "SetCell string validation mismatch"
                            end if

                            set validationText to validationText & "set_cell:" & sheetName & "!" & cellAddress & "=" & my safeText(actualValue) & (ASCII character 30)
                        end tell
                    else
                        set validationText to validationText & "set_cell:" & sheetName & "=transient" & (ASCII character 30)
                    end if

                else if operationKind is "set_formula" then
                    set sheetName to item 2 of fields
                    set cellAddress to item 3 of fields
                    set expectedFormula to item 4 of fields

                    if exists worksheet sheetName of validationWorkbook then
                        tell worksheet sheetName of validationWorkbook
                            set actualFormula to formula of range cellAddress as text

                            if actualFormula is not expectedFormula then
                                error "SetFormula validation mismatch"
                            end if

                            set calculatedValue to value of range cellAddress
                            set validationText to validationText & "set_formula:" & sheetName & "!" & cellAddress & "=" & actualFormula & "|calculated=" & my safeText(calculatedValue) & (ASCII character 30)
                        end tell
                    else
                        set validationText to validationText & "set_formula:" & sheetName & "=transient" & (ASCII character 30)
                    end if

                else if operationKind is "clear_cell" then
                    set sheetName to item 2 of fields
                    set cellAddress to item 3 of fields
                    set neighborAddress to item 4 of fields

                    if exists worksheet sheetName of validationWorkbook then
                        tell worksheet sheetName of validationWorkbook
                            set clearedValue to my safeText(value of range cellAddress)
                            set clearedFormula to my safeText(formula of range cellAddress)

                            if clearedValue is not "" or clearedFormula is not "" then
                                error "ClearCell validation mismatch"
                            end if

                            set validationText to validationText & "clear_cell:" & sheetName & "!" & cellAddress & "=empty" & (ASCII character 30)
                        end tell
                    end if

                else if operationKind is "insert_row" or operationKind is "delete_row" or operationKind is "insert_column" or operationKind is "delete_column" then
                    set sheetName to item 2 of fields
                    set lineReference to item 3 of fields
                    set operatedAddress to item 4 of fields
                    set followingAddress to item 5 of fields

                    -- Read from the reopened saved copy, so this reports where
                    -- the data actually ended up on disk rather than what the
                    -- live session believed.
                    if exists worksheet sheetName of validationWorkbook then
                        tell worksheet sheetName of validationWorkbook
                            set operatedValue to my safeText(value of range operatedAddress)
                            set followingValue to my safeText(value of range followingAddress)

                            set validationText to validationText & operationKind & ":" & sheetName & "!" & lineReference & ":" & operatedAddress & "=" & operatedValue & "," & followingAddress & "=" & followingValue & (ASCII character 30)
                        end tell
                    end if
                end if
            end repeat

            -- Structural changes are proved by reopening the saved copy in
            -- the real E2E; what the adapter reports is what it observed while
            -- making them, so a change that was silently ignored is visible.
            repeat with structuralEntry in structuralSnapshots
                set validationText to validationText & (contents of structuralEntry as text) & (ASCII character 30)
            end repeat

            close validationWorkbook saving no
            set ownsWorkbook to false

            repeat 100 times
                if (count of workbooks) is beforeBooks then exit repeat
                delay 0.1
            end repeat

            set afterBooks to count of workbooks
            set afterWindows to count of windows

            if afterBooks is not beforeBooks then
                error "Final workbook count not restored"
            end if

            if afterWindows is not beforeWindows then
                error "Final window count not restored"
            end if

            return "AIOS_EXCEL_EDIT_PASS" & linefeed & ¬
                "AIOS_COUNTS=" & beforeBooks & "/" & duringBooks & "/" & afterBooks & linefeed & ¬
                "AIOS_WINDOWS=" & beforeWindows & "/" & duringWindows & "/" & afterWindows & linefeed & ¬
                "AIOS_VALIDATION=" & validationText

        on error errorMessage number errorNumber
            if ownsWorkbook then
                try
                    set activePath to full name of active workbook as text

                    if activePath is stagedPath or activePath is outputPath then
                        close active workbook saving no
                    end if
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
    #[ignore]
    fn excel_phase_c_structural_real_e2e() {
        let fixture = std::env::var("AI_OS_EXCEL_EDIT_FIXTURE").expect("preserved fixture path");

        let original = fs::read(&fixture).unwrap();
        let root = tempdir().unwrap();
        let output = root.path().join("phase-c-structural-output.xlsx");

        // Two worksheets, so the row fixture cannot be destroyed by the column
        // workflow and the column fixture cannot be destroyed by the row
        // workflow. A shared sheet would have made the assertions read as
        // passing arithmetic rather than as real shifts.
        //
        // Columns: A1=R1 B1=C1 C1=C2, then the exact workflow the previous
        // attempt failed on -- insert a column before the data, delete the one
        // in front of it.
        // Rows:    A1=X1 A2=R2 A3=R3, insert a row above the data, then delete
        // the first row.
        let result = edit_excel_workbook(&json!({
            "source": fixture,
            "destination": output,
            "operations": [
                {"type":"add_worksheet","name":"Columns"},
                {"type":"set_cell","sheet":"Columns","row":1,"column":1,"value":"R1"},
                {"type":"set_cell","sheet":"Columns","row":1,"column":2,"value":"C1"},
                {"type":"set_cell","sheet":"Columns","row":1,"column":3,"value":"C2"},

                {"type":"add_worksheet","name":"Rows"},
                {"type":"set_cell","sheet":"Rows","row":1,"column":1,"value":"X1"},
                {"type":"set_cell","sheet":"Rows","row":2,"column":1,"value":"R2"},
                {"type":"set_cell","sheet":"Rows","row":3,"column":1,"value":"R3"},

                {"type":"insert_column","sheet":"Columns","column":2},
                {"type":"delete_column","sheet":"Columns","column":1},
                {"type":"insert_row","sheet":"Rows","row":2},
                {"type":"delete_row","sheet":"Rows","row":1}
            ]
        }))
        .unwrap();

        assert_eq!(result["operationResult"]["status"], "edited-copy");
        assert_eq!(
            fs::read(&fixture).unwrap(),
            original,
            "the source workbook must be preserved"
        );
        assert!(output.metadata().unwrap().len() > 0);

        let validation: Vec<String> = result["validation"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect();

        // Two entries can share a prefix: the disk-read probe and the
        // used-range snapshot the adapter recorded while making the change.
        // Matching the exact probe string keeps this reading the saved copy.
        let probed = |prefix: &str, expected: &str| {
            let candidates: Vec<&String> = validation
                .iter()
                .filter(|value| value.starts_with(prefix))
                .collect();
            assert!(
                candidates.iter().any(|value| value.as_str() == expected),
                "expected `{expected}` among {candidates:?}"
            );
        };

        // Every assertion below reads the reopened saved copy, so it is the
        // content on disk that is being judged, not the live session.
        //
        // Columns start as        A=R1 B=C1 C=C2
        // insert column B gives   A=R1 B=[]  C=C1 D=C2   probes B1,C1
        // delete column A gives   A=[]  B=C1 C=C2        probes A1,B1
        probed(
            "insert_column:Columns!B:B:",
            "insert_column:Columns!B:B:B1=,C1=C1",
        );
        probed(
            "delete_column:Columns!A:A:",
            "delete_column:Columns!A:A:A1=,B1=C1",
        );

        // Rows start as        1=X1 2=R2 3=R3
        // insert row 2 gives   1=X1 2=[]  3=R2 4=R3      probes A2,A3
        // delete row 1 gives   1=[]  2=R2 3=R3           probes A1,A2
        probed("insert_row:Rows!2:2:", "insert_row:Rows!2:2:A2=,A3=R2");
        probed("delete_row:Rows!1:1:", "delete_row:Rows!1:1:A1=,A2=R2");
    }

    #[test]
    fn structural_operations_are_parsed_with_bounded_one_based_indexes() {
        assert_eq!(
            parse_operations(&request(json!({"type":"insert_row","sheet":"Sheet1","row":3})))
                .unwrap()[0],
            EditOperation::InsertRow {
                sheet: "Sheet1".to_owned(),
                row: 3
            }
        );
        assert_eq!(
            parse_operations(&request(json!({"type":"delete_column","sheet":"Sheet1","column":2})))
                .unwrap()[0],
            EditOperation::DeleteColumn {
                sheet: "Sheet1".to_owned(),
                column: 2
            }
        );

        // Out of bounds, zero and missing indexes all fail closed. There is no
        // count parameter, so nothing can widen one operation into many.
        for rejected in [
            json!({"type":"insert_row","sheet":"Sheet1","row":0}),
            json!({"type":"insert_row","sheet":"Sheet1","row":10001}),
            json!({"type":"delete_row","sheet":"Sheet1"}),
            json!({"type":"insert_column","sheet":"Sheet1","column":0}),
            json!({"type":"insert_column","sheet":"Sheet1","column":257}),
            json!({"type":"delete_column","sheet":"","column":1}),
            json!({"type":"insert_row","sheet":"Sheet1","row":2,"count":5}),
        ] {
            let outcome = parse_operations(&request(rejected.clone()));
            if rejected.get("count").is_some() {
                // A stray count is ignored rather than honoured: the operation
                // still addresses exactly one line.
                assert_eq!(
                    outcome.unwrap()[0],
                    EditOperation::InsertRow {
                        sheet: "Sheet1".to_owned(),
                        row: 2
                    }
                );
            } else {
                assert!(outcome.is_err(), "{rejected} should have been rejected");
            }
        }
    }

    #[test]
    fn structural_operations_encode_whole_line_references() {
        let encoded = encode_operations(
            &parse_operations(&json!({
                "operations": [
                    {"type": "insert_column", "sheet": "Sheet1", "column": 2},
                    {"type": "delete_column", "sheet": "Sheet1", "column": 1},
                    {"type": "insert_row", "sheet": "Sheet1", "row": 3},
                    {"type": "delete_row", "sheet": "Sheet1", "row": 1}
                ]
            }))
            .unwrap(),
        );

        assert!(encoded.contains("insert_column"));
        assert!(encoded.contains("B:B"), "column 2 must address the whole B column");
        assert!(encoded.contains("A:A"), "column 1 must address the whole A column");
        assert!(encoded.contains("3:3"), "row 3 must address the whole row");
        assert!(encoded.contains("1:1"));
        // The shift parameter is deliberately absent: a real Excel 16.78 probe
        // showed the plain forms shift content correctly without it.
        assert!(!encoded.contains("shift"));
        // Each structural operation carries the two cells whose reopened
        // contents prove the data actually moved.
        assert!(encoded.contains("B1"));
        assert!(encoded.contains("C1"));
        assert!(encoded.contains("A3"));
        assert!(encoded.contains("A4"));
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
    fn phase_b_mutation_operations_validate_structured_inputs() {
        assert!(
            parse_operations(&request(json!({"type":"add_worksheet","name":"Summary"}))).is_ok()
        );

        assert!(parse_operations(&request(
            json!({"type":"rename_worksheet","sheet":"Summary","name":"History"})
        ))
        .is_ok());

        assert!(parse_operations(&request(
            json!({"type":"delete_worksheet","sheet":"History"})
        ))
        .is_ok());

        for operation in [
            json!({"type":"add_worksheet","name":""}),
            json!({"type":"add_worksheet","name":"Bad/Name"}),
            json!({"type":"rename_worksheet","sheet":"Summary","name":"Summary"}),
            json!({"type":"rename_worksheet","sheet":"Summary","name":"Bad:Name"}),
            json!({"type":"delete_worksheet","sheet":""}),
        ] {
            assert!(parse_operations(&request(operation)).is_err());
        }
    }

    #[test]
    fn phase_b_mutation_encoding_preserves_explicit_identity() {
        let operations = parse_operations(&json!({
            "operations": [
                {"type":"add_worksheet","name":"Summary"},
                {"type":"rename_worksheet","sheet":"Summary","name":"History"},
                {"type":"delete_worksheet","sheet":"History"}
            ]
        }))
        .unwrap();

        let encoded = encode_operations(&operations);

        assert!(encoded.contains("add_worksheet"));
        assert!(encoded.contains("Summary"));
        assert!(encoded.contains("rename_worksheet"));
        assert!(encoded.contains("History"));
        assert!(encoded.contains("delete_worksheet"));
    }

    #[test]
    #[ignore = "requires Microsoft Excel and AI_OS_EXCEL_EDIT_FIXTURE"]
    fn excel_phase_b_mutation_real_e2e() {
        let fixture = std::env::var("AI_OS_EXCEL_EDIT_FIXTURE").expect("preserved fixture path");

        let original = fs::read(&fixture).unwrap();
        let root = tempdir().unwrap();
        let output = root.path().join("phase-b-mutation-output.xlsx");

        let result = edit_excel_workbook(&json!({
            "source": fixture,
            "destination": output,
            "operations": [
                {"type":"add_worksheet","name":"Summary"},
                {"type":"set_cell","sheet":"Summary","row":1,"column":1,"value":"Metric"},
                {"type":"set_cell","sheet":"Summary","row":2,"column":2,"value":100},
                {"type":"set_formula","sheet":"Summary","row":3,"column":2,"formula":"=1+2"},

                {"type":"add_worksheet","name":"Archive"},
                {"type":"rename_worksheet","sheet":"Archive","name":"History"},
                {"type":"set_cell","sheet":"History","row":1,"column":1,"value":"kept"},

                {"type":"add_worksheet","name":"DeleteMe"},
                {"type":"set_cell","sheet":"DeleteMe","row":1,"column":1,"value":"temporary"},
                {"type":"delete_worksheet","sheet":"DeleteMe"}
            ]
        }))
        .unwrap();

        assert_eq!(result["operationResult"]["status"], "edited-copy");
        assert_eq!(fs::read(&fixture).unwrap(), original);
        assert!(output.metadata().unwrap().len() > 0);

        let validation = result["validation"].as_array().unwrap();

        assert!(validation
            .iter()
            .any(|value| value.as_str().unwrap() == "final_worksheets=validated"));

        assert!(validation.iter().any(|value| {
            value
                .as_str()
                .unwrap()
                .contains("set_formula:Summary!B3==1+2")
        }));

        assert!(validation
            .iter()
            .any(|value| { value.as_str().unwrap().contains("set_cell:History!A1=kept") }));

        assert!(validation.iter().any(|value| {
            value
                .as_str()
                .unwrap()
                .contains("set_cell:DeleteMe=transient")
        }));
    }

    #[test]
    #[ignore = "requires Microsoft Excel and AI_OS_EXCEL_EDIT_FIXTURE"]
    fn excel_phase_b_delete_final_sheet_fails_closed() {
        let fixture = std::env::var("AI_OS_EXCEL_EDIT_FIXTURE").expect("preserved fixture path");

        let original = fs::read(&fixture).unwrap();
        let root = tempdir().unwrap();
        let output = root.path().join("phase-b-invalid-delete.xlsx");

        let error = edit_excel_workbook(&json!({
            "source": fixture,
            "destination": output,
            "operations": [
                {"type":"delete_worksheet","sheet":"Sheet1"}
            ]
        }))
        .unwrap_err();

        assert!(!error.invalid_request);
        assert!(error.message.contains("Cannot delete the final worksheet"));
        assert_eq!(fs::read(&fixture).unwrap(), original);
        assert!(!output.exists());
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

        let workbook_counts = result["ownership"]["workbookCounts"]
            .as_str()
            .expect("workbook count triplet");
        let workbook_counts = workbook_counts
            .split('/')
            .map(|value| value.parse::<u64>().expect("numeric workbook count"))
            .collect::<Vec<_>>();
        assert_eq!(workbook_counts.len(), 3);
        assert_eq!(workbook_counts[1], workbook_counts[0] + 1);
        assert_eq!(workbook_counts[2], workbook_counts[0]);

        let window_counts = result["ownership"]["windowCounts"]
            .as_str()
            .expect("window count triplet");
        let window_counts = window_counts
            .split('/')
            .map(|value| value.parse::<u64>().expect("numeric window count"))
            .collect::<Vec<_>>();
        assert_eq!(window_counts.len(), 3);
        assert!(window_counts[1] >= window_counts[0]);
        assert_eq!(window_counts[2], window_counts[0]);

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
