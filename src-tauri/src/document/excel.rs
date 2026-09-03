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
const MAX_NUMBER_FORMAT_TEXT: usize = 64;
const MAX_FONT_NAME_TEXT: usize = 64;
/// Excel's own limits: font size 1-409pt, palette index 1-56, column
/// width 0-255 characters, row height 0-409pt. Zero is excluded on both
/// dimensions because it hides the line rather than resizing it, which is
/// a different intent than the caller expressed.
const MAX_FONT_SIZE: u64 = 409;
const MAX_COLOR_INDEX: u64 = 56;
const MAX_COLUMN_WIDTH: u64 = 255;
const MAX_ROW_HEIGHT: u64 = 409;
const MAX_FORMAT_AREA: u64 = 65_536;
const MAX_CRITERIA_TEXT: usize = 256;
const MAX_CHART_NAME_TEXT: usize = 64;
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

/// The four chart types proven on real Excel 16.78. Each carries both the
/// constant to set and the constant Excel stores, because they are not always
/// the same: asking for `line chart` produces a chart that reads back as
/// `line markers`. Validation compares against what Excel stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChartKind {
    Column,
    Bar,
    Line,
    Pie,
}

impl ChartKind {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "column" => Some(Self::Column),
            "bar" => Some(Self::Bar),
            "line" => Some(Self::Line),
            "pie" => Some(Self::Pie),
            _ => None,
        }
    }

    fn selector(self) -> &'static str {
        match self {
            Self::Column => "column",
            Self::Bar => "bar",
            Self::Line => "line",
            Self::Pie => "pie",
        }
    }

    fn stored(self) -> &'static str {
        match self {
            Self::Column => "column clustered",
            Self::Bar => "bar clustered",
            Self::Line => "line markers",
            Self::Pie => "pie chart",
        }
    }
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
    /// One rectangular range, several optional attributes. Every attribute was
    /// proved on real Excel 16.78 to both apply and survive save-as-xlsx,
    /// close and reopen -- the adapter validates by reading the reopened saved
    /// copy, so an attribute that is correct live but lost on save could not be
    /// validated that way.
    FormatCells {
        sheet: String,
        start_row: u64,
        start_column: u64,
        end_row: u64,
        end_column: u64,
        bold: Option<bool>,
        italic: Option<bool>,
        font_size: Option<u64>,
        font_name: Option<String>,
        fill_color_index: Option<u64>,
        number_format: Option<String>,
    },
    SetColumnWidth {
        sheet: String,
        column: u64,
        width: u64,
    },
    SetRowHeight {
        sheet: String,
        row: u64,
        height: u64,
    },
    /// `hasHeader` is required rather than defaulted: guessing it wrong sorts
    /// the caller's header row into their data, which no later operation can
    /// undo.
    SortRange {
        sheet: String,
        start_row: u64,
        start_column: u64,
        end_row: u64,
        end_column: u64,
        key_column: u64,
        descending: bool,
        has_header: bool,
    },
    ApplyFilter {
        sheet: String,
        start_row: u64,
        start_column: u64,
        end_row: u64,
        end_column: u64,
        field: u64,
        criteria: String,
    },
    ClearFilter {
        sheet: String,
        start_row: u64,
        start_column: u64,
        end_row: u64,
        end_column: u64,
        has_header: bool,
    },
    /// An explicit name is required rather than accepting Excel's own
    /// `Chart 1`: it is the handle the reopened copy is searched by, and two
    /// charts sharing a name would make that search ambiguous.
    AddChart {
        sheet: String,
        start_row: u64,
        start_column: u64,
        end_row: u64,
        end_column: u64,
        chart_type: ChartKind,
        name: String,
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

/// A bounded rectangular range, returned as its A1 reference and the address of
/// the top-left cell, which is where the reopened copy is read to prove the
/// formatting landed.
fn cell_range(operation: &Value) -> Result<(u64, u64, u64, u64), ExcelError> {
    let start_row = line_index(operation, "startRow", MAX_ROW)?;
    let start_column = line_index(operation, "startColumn", MAX_COLUMN)?;
    let end_row = line_index(operation, "endRow", MAX_ROW)?;
    let end_column = line_index(operation, "endColumn", MAX_COLUMN)?;

    if end_row < start_row || end_column < start_column {
        return Err(ExcelError::invalid(
            "range end must not precede range start",
        ));
    }

    let area = (end_row - start_row + 1) * (end_column - start_column + 1);
    if area > MAX_FORMAT_AREA {
        return Err(ExcelError::invalid(format!(
            "range must cover at most {MAX_FORMAT_AREA} cells"
        )));
    }

    Ok((start_row, start_column, end_row, end_column))
}

/// The first row a sort or filter is allowed to move, and the last row of the
/// range. Both are read back from the reopened copy: the first proves a sort
/// reordered real data, and the pair proves a cleared filter left nothing
/// hidden.
fn data_rows(start_row: u64, end_row: u64, has_header: bool) -> (u64, u64) {
    let first = if has_header && end_row > start_row {
        start_row + 1
    } else {
        start_row
    };
    (first, end_row)
}

fn range_reference(start_row: u64, start_column: u64, end_row: u64, end_column: u64) -> String {
    format!(
        "{}{}:{}{}",
        column_name(start_column),
        start_row,
        column_name(end_column),
        end_row
    )
}

fn optional_bool(operation: &Value, field: &str) -> Result<Option<bool>, ExcelError> {
    match operation.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(ExcelError::invalid(format!("{field} must be a boolean"))),
    }
}

fn optional_bounded_number(
    operation: &Value,
    field: &str,
    maximum: u64,
) -> Result<Option<u64>, ExcelError> {
    match operation.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let value = value
                .as_u64()
                .ok_or_else(|| ExcelError::invalid(format!("{field} must be a whole number")))?;
            if !(1..=maximum).contains(&value) {
                return Err(ExcelError::invalid(format!(
                    "{field} must be between 1 and {maximum}"
                )));
            }
            Ok(Some(value))
        }
    }
}

fn optional_bounded_text(
    operation: &Value,
    field: &str,
    maximum: usize,
) -> Result<Option<String>, ExcelError> {
    match operation.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let value = value
                .as_str()
                .ok_or_else(|| ExcelError::invalid(format!("{field} must be a string")))?;
            if value.trim().is_empty() {
                return Err(ExcelError::invalid(format!("{field} must not be empty")));
            }
            validate_text(value, maximum, field)?;
            Ok(Some(value.to_owned()))
        }
    }
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
        "format_cells" => {
            let sheet = sheet_name(operation)?;
            let (start_row, start_column, end_row, end_column) = cell_range(operation)?;

            let bold = optional_bool(operation, "bold")?;
            let italic = optional_bool(operation, "italic")?;
            let font_size = optional_bounded_number(operation, "fontSize", MAX_FONT_SIZE)?;
            let font_name = optional_bounded_text(operation, "fontName", MAX_FONT_NAME_TEXT)?;
            let fill_color_index =
                optional_bounded_number(operation, "fillColorIndex", MAX_COLOR_INDEX)?;
            let number_format =
                optional_bounded_text(operation, "numberFormat", MAX_NUMBER_FORMAT_TEXT)?;

            // An operation that carries no attribute would change nothing and
            // then validate successfully, which is worse than refusing it.
            if bold.is_none()
                && italic.is_none()
                && font_size.is_none()
                && font_name.is_none()
                && fill_color_index.is_none()
                && number_format.is_none()
            {
                return Err(ExcelError::invalid(
                    "format_cells requires at least one of bold, italic, fontSize, fontName, fillColorIndex, or numberFormat",
                ));
            }

            Ok(EditOperation::FormatCells {
                sheet,
                start_row,
                start_column,
                end_row,
                end_column,
                bold,
                italic,
                font_size,
                font_name,
                fill_color_index,
                number_format,
            })
        }
        "set_column_width" => Ok(EditOperation::SetColumnWidth {
            sheet: sheet_name(operation)?,
            column: line_index(operation, "column", MAX_COLUMN)?,
            width: line_index(operation, "width", MAX_COLUMN_WIDTH)?,
        }),
        "set_row_height" => Ok(EditOperation::SetRowHeight {
            sheet: sheet_name(operation)?,
            row: line_index(operation, "row", MAX_ROW)?,
            height: line_index(operation, "height", MAX_ROW_HEIGHT)?,
        }),
        "sort_range" => {
            let sheet = sheet_name(operation)?;
            let (start_row, start_column, end_row, end_column) = cell_range(operation)?;
            let key_column = line_index(operation, "keyColumn", MAX_COLUMN)?;

            if !(start_column..=end_column).contains(&key_column) {
                return Err(ExcelError::invalid(
                    "keyColumn must fall inside the sorted range",
                ));
            }

            let descending = match operation.get("order").and_then(Value::as_str) {
                Some("ascending") => false,
                Some("descending") => true,
                _ => {
                    return Err(ExcelError::invalid(
                        "sort_range requires order to be ascending or descending",
                    ))
                }
            };

            let has_header = operation
                .get("hasHeader")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    ExcelError::invalid("sort_range requires an explicit hasHeader boolean")
                })?;

            if has_header && end_row == start_row {
                return Err(ExcelError::invalid(
                    "a sorted range with a header must contain at least one data row",
                ));
            }

            Ok(EditOperation::SortRange {
                sheet,
                start_row,
                start_column,
                end_row,
                end_column,
                key_column,
                descending,
                has_header,
            })
        }
        "apply_filter" => {
            let sheet = sheet_name(operation)?;
            let (start_row, start_column, end_row, end_column) = cell_range(operation)?;
            let width = end_column - start_column + 1;
            let field = line_index(operation, "field", width)?;

            let criteria = operation.get("criteria").and_then(Value::as_str).unwrap_or("");
            if criteria.trim().is_empty() {
                return Err(ExcelError::invalid("apply_filter requires criteria"));
            }
            validate_text(criteria, MAX_CRITERIA_TEXT, "criteria")?;

            Ok(EditOperation::ApplyFilter {
                sheet,
                start_row,
                start_column,
                end_row,
                end_column,
                field,
                criteria: criteria.to_owned(),
            })
        }
        "clear_filter" => {
            let sheet = sheet_name(operation)?;
            let (start_row, start_column, end_row, end_column) = cell_range(operation)?;

            let has_header = operation
                .get("hasHeader")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    ExcelError::invalid("clear_filter requires an explicit hasHeader boolean")
                })?;

            Ok(EditOperation::ClearFilter {
                sheet,
                start_row,
                start_column,
                end_row,
                end_column,
                has_header,
            })
        }
        "add_chart" => {
            let sheet = sheet_name(operation)?;
            let (start_row, start_column, end_row, end_column) = cell_range(operation)?;

            let chart_type = operation
                .get("chartType")
                .and_then(Value::as_str)
                .and_then(ChartKind::parse)
                .ok_or_else(|| {
                    ExcelError::invalid(
                        "add_chart requires chartType to be column, bar, line, or pie",
                    )
                })?;

            let name = operation.get("name").and_then(Value::as_str).unwrap_or("");
            if name.trim().is_empty() {
                return Err(ExcelError::invalid("add_chart requires a chart name"));
            }
            validate_text(name, MAX_CHART_NAME_TEXT, "chart name")?;

            Ok(EditOperation::AddChart {
                sheet,
                start_row,
                start_column,
                end_row,
                end_column,
                chart_type,
                name: name.to_owned(),
            })
        }
        _ => Err(ExcelError::invalid(
            "spreadsheet.edit supports set_cell, set_formula, clear_cell, add_worksheet, rename_worksheet, delete_worksheet, insert_row, delete_row, insert_column, delete_column, format_cells, set_column_width, set_row_height, sort_range, apply_filter, clear_filter, and add_chart",
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
                // Fixed arity, with an empty field for each absent attribute and
                // a terminator so no optional value is ever the trailing field.
                EditOperation::FormatCells {
                    sheet,
                    start_row,
                    start_column,
                    end_row,
                    end_column,
                    bold,
                    italic,
                    font_size,
                    font_name,
                    fill_color_index,
                    number_format,
                } => {
                    let flag = |value: &Option<bool>| {
                        value.map(|value| value.to_string()).unwrap_or_default()
                    };
                    let number = |value: &Option<u64>| {
                        value.map(|value| value.to_string()).unwrap_or_default()
                    };

                    vec![
                        "format_cells".to_owned(),
                        sheet.clone(),
                        range_reference(*start_row, *start_column, *end_row, *end_column),
                        format!("{}{}", column_name(*start_column), start_row),
                        flag(bold),
                        flag(italic),
                        number(font_size),
                        font_name.clone().unwrap_or_default(),
                        number(fill_color_index),
                        number_format.clone().unwrap_or_default(),
                        "end".to_owned(),
                    ]
                }
                EditOperation::SetColumnWidth {
                    sheet,
                    column,
                    width,
                } => vec![
                    "set_column_width".to_owned(),
                    sheet.clone(),
                    column.to_string(),
                    width.to_string(),
                ],
                EditOperation::SetRowHeight { sheet, row, height } => vec![
                    "set_row_height".to_owned(),
                    sheet.clone(),
                    row.to_string(),
                    height.to_string(),
                ],
                // The sort order and the header flag are AppleScript enumeration
                // constants, not strings, so they cannot be interpolated; these
                // fields select between four literal probed forms.
                EditOperation::SortRange {
                    sheet,
                    start_row,
                    start_column,
                    end_row,
                    end_column,
                    key_column,
                    descending,
                    has_header,
                } => {
                    let (first_row, _) = data_rows(*start_row, *end_row, *has_header);
                    let following = if first_row < *end_row {
                        first_row + 1
                    } else {
                        first_row
                    };
                    let key = column_name(*key_column);

                    vec![
                        "sort_range".to_owned(),
                        sheet.clone(),
                        range_reference(*start_row, *start_column, *end_row, *end_column),
                        format!("{key}{start_row}"),
                        if *descending {
                            "descending".to_owned()
                        } else {
                            "ascending".to_owned()
                        },
                        if *has_header {
                            "yes".to_owned()
                        } else {
                            "no".to_owned()
                        },
                        format!("{key}{first_row}"),
                        format!("{key}{following}"),
                    ]
                }
                EditOperation::ApplyFilter {
                    sheet,
                    start_row,
                    start_column,
                    end_row,
                    end_column,
                    field,
                    criteria,
                } => {
                    let (first_row, last_row) = data_rows(*start_row, *end_row, true);

                    vec![
                        "apply_filter".to_owned(),
                        sheet.clone(),
                        range_reference(*start_row, *start_column, *end_row, *end_column),
                        field.to_string(),
                        criteria.clone(),
                        first_row.to_string(),
                        last_row.to_string(),
                    ]
                }
                EditOperation::ClearFilter {
                    sheet,
                    start_row,
                    start_column,
                    end_row,
                    end_column,
                    has_header,
                } => {
                    let (first_row, last_row) = data_rows(*start_row, *end_row, *has_header);

                    vec![
                        "clear_filter".to_owned(),
                        sheet.clone(),
                        range_reference(*start_row, *start_column, *end_row, *end_column),
                        String::new(),
                        String::new(),
                        first_row.to_string(),
                        last_row.to_string(),
                    ]
                }
                EditOperation::AddChart {
                    sheet,
                    start_row,
                    start_column,
                    end_row,
                    end_column,
                    chart_type,
                    name,
                } => vec![
                    "add_chart".to_owned(),
                    sheet.clone(),
                    range_reference(*start_row, *start_column, *end_row, *end_column),
                    name.clone(),
                    chart_type.selector().to_owned(),
                    chart_type.stored().to_owned(),
                ],
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

-- Does any operation AFTER this one move content on the same worksheet?
--
-- Displacement has to be answered per operation, not per worksheet. Asking only
-- "was this sheet ever sorted" marks a format applied AFTER the sort as
-- displaced, which silently skips a check that would have held -- exactly the
-- quiet widening into "skip validation whenever anything happened" that this
-- rule must not become.
on movesAfter(recordList, startIndex, sheetName, movingKinds)
    repeat with laterIndex from (startIndex + 1) to (count of recordList)
        set laterFields to my splitText(contents of item laterIndex of recordList, ASCII character 31)

        if my listContains(movingKinds, item 1 of laterFields) then
            if (item 2 of laterFields) is sheetName then return true
        end if
    end repeat

    return false
end movesAfter

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

                    -- Excel raises "will permanently delete this sheet" unless
                    -- alerts are off, and an unattended run cannot answer a
                    -- modal: it sits there until killed, and the modal then
                    -- blocks every later Excel automation. This worked for a
                    -- long time only because this machine's `display alerts`
                    -- happened to be false, which made the failure look random
                    -- when it flipped back to true.
                    --
                    -- The previous value is restored on both paths, because
                    -- leaving a user's Excel with alerts off is not acceptable.
                    set alertsBeforeDelete to display alerts
                    set display alerts to false

                    try
                        delete worksheet sourceName of freshWorkbook
                        set display alerts to alertsBeforeDelete
                    on error deleteErrorText number deleteErrorNumber
                        set display alerts to alertsBeforeDelete
                        error deleteErrorText number deleteErrorNumber
                    end try

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

                else if operationKind is "format_cells" then
                    set sheetName to item 2 of fields
                    set rangeReference to item 3 of fields
                    set boldText to item 5 of fields
                    set italicText to item 6 of fields
                    set sizeText to item 7 of fields
                    set fontNameText to item 8 of fields
                    set fillText to item 9 of fields
                    set numberText to item 10 of fields

                    if not (exists worksheet sheetName of freshWorkbook) then
                        error "Worksheet not found: " & sheetName number -2702
                    end if

                    -- Each of these was proved on real Excel 16.78 to apply and
                    -- to survive save-as-xlsx, close and reopen. An empty field
                    -- means the caller did not ask for that attribute, so it is
                    -- left exactly as it was.
                    tell worksheet sheetName of freshWorkbook
                        if boldText is not "" then
                            set bold of font object of range rangeReference to (boldText is "true")
                        end if
                        if italicText is not "" then
                            set italic of font object of range rangeReference to (italicText is "true")
                        end if
                        if sizeText is not "" then
                            set font size of font object of range rangeReference to (sizeText as real)
                        end if
                        if fontNameText is not "" then
                            set name of font object of range rangeReference to fontNameText
                        end if
                        if fillText is not "" then
                            set color index of interior object of range rangeReference to (fillText as integer)
                        end if
                        if numberText is not "" then
                            set number format of range rangeReference to numberText
                        end if
                    end tell

                else if operationKind is "set_column_width" then
                    set sheetName to item 2 of fields
                    set columnIndexText to item 3 of fields
                    set widthText to item 4 of fields

                    if not (exists worksheet sheetName of freshWorkbook) then
                        error "Worksheet not found: " & sheetName number -2702
                    end if

                    tell worksheet sheetName of freshWorkbook
                        set column width of column (columnIndexText as integer) to (widthText as real)
                    end tell

                else if operationKind is "set_row_height" then
                    set sheetName to item 2 of fields
                    set rowIndexText to item 3 of fields
                    set heightText to item 4 of fields

                    if not (exists worksheet sheetName of freshWorkbook) then
                        error "Worksheet not found: " & sheetName number -2702
                    end if

                    tell worksheet sheetName of freshWorkbook
                        set row height of row (rowIndexText as integer) to (heightText as real)
                    end tell

                else if operationKind is "sort_range" then
                    set sheetName to item 2 of fields
                    set rangeReference to item 3 of fields
                    set keyReference to item 4 of fields
                    set orderText to item 5 of fields
                    set headerText to item 6 of fields

                    if not (exists worksheet sheetName of freshWorkbook) then
                        error "Worksheet not found: " & sheetName number -2702
                    end if

                    -- `sort ascending` and `header yes` are enumeration
                    -- constants, not strings, so they cannot be built from a
                    -- field. These are the four probed forms written out.
                    set sortSheet to worksheet sheetName of freshWorkbook
                    if orderText is "ascending" then
                        if headerText is "yes" then
                            sort (range rangeReference of sortSheet) key1 (range keyReference of sortSheet) order1 sort ascending header header yes
                        else
                            sort (range rangeReference of sortSheet) key1 (range keyReference of sortSheet) order1 sort ascending header header no
                        end if
                    else
                        if headerText is "yes" then
                            sort (range rangeReference of sortSheet) key1 (range keyReference of sortSheet) order1 sort descending header header yes
                        else
                            sort (range rangeReference of sortSheet) key1 (range keyReference of sortSheet) order1 sort descending header header no
                        end if
                    end if

                else if operationKind is "apply_filter" then
                    set sheetName to item 2 of fields
                    set rangeReference to item 3 of fields
                    set fieldText to item 4 of fields
                    set criteriaText to item 5 of fields

                    if not (exists worksheet sheetName of freshWorkbook) then
                        error "Worksheet not found: " & sheetName number -2702
                    end if

                    set filterSheet to worksheet sheetName of freshWorkbook
                    autofilter range (range rangeReference of filterSheet) field (fieldText as integer) criteria1 criteriaText

                else if operationKind is "clear_filter" then
                    set sheetName to item 2 of fields

                    if not (exists worksheet sheetName of freshWorkbook) then
                        error "Worksheet not found: " & sheetName number -2702
                    end if

                    -- Fails closed when there is no filter to clear: the caller
                    -- believed one was there, and silently succeeding would hide
                    -- that they were wrong.
                    show all data (worksheet sheetName of freshWorkbook)

                else if operationKind is "add_chart" then
                    set sheetName to item 2 of fields
                    set rangeReference to item 3 of fields
                    set chartName to item 4 of fields
                    set chartSelector to item 5 of fields

                    if not (exists worksheet sheetName of freshWorkbook) then
                        error "Worksheet not found: " & sheetName number -2702
                    end if

                    set chartSheet to worksheet sheetName of freshWorkbook

                    -- The reopened copy is searched by name, so two charts
                    -- sharing one would make that search ambiguous.
                    --
                    -- Addressed directly rather than enumerated. A real probe
                    -- showed that `repeat with x in chart objects of <sheet>`
                    -- hangs Excel indefinitely once a chart exists on that
                    -- sheet, and `with timeout` does not rescue it -- the
                    -- process has to be killed. `exists chart object <name>`,
                    -- `count of chart objects` and `name of chart object <i>`
                    -- all answer in well under a second.
                    if exists chart object chartName of chartSheet then
                        error "A chart named " & chartName & " already exists" number -2712
                    end if

                    -- Exactly the probed order. The source must be set by the
                    -- command form: `set (source data of chart of X) to <range>`
                    -- fails with -10006.
                    set madeChart to make new chart object at chartSheet
                    set source data chart of madeChart source (range rangeReference of chartSheet) plot by columns

                    if chartSelector is "column" then
                        set chart type of chart of madeChart to column clustered
                    else if chartSelector is "bar" then
                        set chart type of chart of madeChart to bar clustered
                    else if chartSelector is "line" then
                        set chart type of chart of madeChart to line markers
                    else
                        set chart type of chart of madeChart to pie chart
                    end if

                    set name of madeChart to chartName
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

            repeat with recordIndex from 1 to count of operationRecords
                set fields to my splitText(contents of item recordIndex of operationRecords, ASCII character 31)
                set operationKind to item 1 of fields
                set recordSheet to item 2 of fields

                -- Two answers, because the two kinds of movement invalidate
                -- different things. A structural change shifts everything after
                -- it, so row and column INDEXES move too and every address- or
                -- index-based probe is stale. A sort only reorders rows within
                -- its own range: values and the cell formatting that travels
                -- with them move, but indexes do not, so line measures and
                -- filter row probes still hold.
                set structuralAfter to my movesAfter(operationRecords, recordIndex, recordSheet, {"insert_row", "delete_row", "insert_column", "delete_column"})
                set reorderAfter to my movesAfter(operationRecords, recordIndex, recordSheet, {"sort_range"})

                if operationKind is "set_cell" then
                    set sheetName to item 2 of fields
                    set cellAddress to item 3 of fields
                    set scalarKind to item 4 of fields
                    set expectedText to item 5 of fields

                    if structuralAfter or reorderAfter then
                        set validationText to validationText & "set_cell:" & sheetName & "!" & cellAddress & "=displaced-by-moved-content" & (ASCII character 30)
                    else if exists worksheet sheetName of validationWorkbook then
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

                    if structuralAfter or reorderAfter then
                        set validationText to validationText & "set_formula:" & sheetName & "!" & cellAddress & "=displaced-by-moved-content" & (ASCII character 30)
                    else if exists worksheet sheetName of validationWorkbook then
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

                    if structuralAfter or reorderAfter then
                        set validationText to validationText & "clear_cell:" & sheetName & "!" & cellAddress & "=displaced-by-moved-content" & (ASCII character 30)
                    else if exists worksheet sheetName of validationWorkbook then
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

                else if operationKind is "format_cells" then
                    set sheetName to item 2 of fields
                    set rangeReference to item 3 of fields
                    set probeAddress to item 4 of fields
                    set boldText to item 5 of fields
                    set italicText to item 6 of fields
                    set sizeText to item 7 of fields
                    set fontNameText to item 8 of fields
                    set fillText to item 9 of fields
                    set numberText to item 10 of fields

                    if structuralAfter or reorderAfter then
                        set validationText to validationText & "format_cells:" & sheetName & "!" & rangeReference & "=displaced-by-moved-content" & (ASCII character 30)
                    else if exists worksheet sheetName of validationWorkbook then
                        tell worksheet sheetName of validationWorkbook
                            set observedText to ""

                            if boldText is not "" then
                                set actualBold to (bold of font object of range probeAddress) as text
                                if actualBold is not boldText then
                                    error "FormatCells bold validation mismatch"
                                end if
                                set observedText to observedText & "bold=" & actualBold & ";"
                            end if

                            if italicText is not "" then
                                set actualItalic to (italic of font object of range probeAddress) as text
                                if actualItalic is not italicText then
                                    error "FormatCells italic validation mismatch"
                                end if
                                set observedText to observedText & "italic=" & actualItalic & ";"
                            end if

                            if sizeText is not "" then
                                set actualSize to font size of font object of range probeAddress
                                if (actualSize as real) is not (sizeText as real) then
                                    error "FormatCells font size validation mismatch"
                                end if
                                set observedText to observedText & "size=" & (actualSize as text) & ";"
                            end if

                            if fontNameText is not "" then
                                set actualFontName to (name of font object of range probeAddress) as text
                                if actualFontName is not fontNameText then
                                    error "FormatCells font name validation mismatch"
                                end if
                                set observedText to observedText & "font=" & actualFontName & ";"
                            end if

                            if fillText is not "" then
                                set actualFill to color index of interior object of range probeAddress
                                if (actualFill as integer) is not (fillText as integer) then
                                    error "FormatCells fill validation mismatch"
                                end if
                                set observedText to observedText & "fill=" & (actualFill as text) & ";"
                            end if

                            if numberText is not "" then
                                set actualNumber to (number format of range probeAddress) as text
                                if actualNumber is not numberText then
                                    error "FormatCells number format validation mismatch"
                                end if
                                set observedText to observedText & "number=" & actualNumber & ";"
                            end if

                            set validationText to validationText & "format_cells:" & sheetName & "!" & rangeReference & ":" & observedText & (ASCII character 30)
                        end tell
                    end if

                else if operationKind is "set_column_width" or operationKind is "set_row_height" then
                    set sheetName to item 2 of fields
                    set lineIndexText to item 3 of fields
                    set measureText to item 4 of fields

                    if structuralAfter then
                        set validationText to validationText & operationKind & ":" & sheetName & "!" & lineIndexText & "=displaced-by-moved-content" & (ASCII character 30)
                    else if exists worksheet sheetName of validationWorkbook then
                        tell worksheet sheetName of validationWorkbook
                            if operationKind is "set_column_width" then
                                set actualMeasure to column width of column (lineIndexText as integer)
                            else
                                set actualMeasure to row height of row (lineIndexText as integer)
                            end if

                            -- Excel stores these as reals and may settle a
                            -- fraction away from the requested whole number, so
                            -- the comparison is a tolerance and the observed
                            -- value is reported rather than the requested one.
                            set measureDrift to (actualMeasure as real) - (measureText as real)
                            if measureDrift < 0 then
                                set measureDrift to -measureDrift
                            end if
                            if measureDrift > 0.5 then
                                error "Line measure validation mismatch"
                            end if

                            set validationText to validationText & operationKind & ":" & sheetName & "!" & lineIndexText & "=" & (actualMeasure as text) & (ASCII character 30)
                        end tell
                    end if

                else if operationKind is "sort_range" then
                    set sheetName to item 2 of fields
                    set rangeReference to item 3 of fields
                    set firstProbe to item 7 of fields
                    set secondProbe to item 8 of fields

                    if structuralAfter then
                        set validationText to validationText & "sort_range:" & sheetName & "!" & rangeReference & "=displaced-by-moved-content" & (ASCII character 30)
                    else if exists worksheet sheetName of validationWorkbook then
                        tell worksheet sheetName of validationWorkbook
                            set firstValue to my safeText(value of range firstProbe)
                            set secondValue to my safeText(value of range secondProbe)

                            set validationText to validationText & "sort_range:" & sheetName & "!" & rangeReference & ":" & firstProbe & "=" & firstValue & "," & secondProbe & "=" & secondValue & (ASCII character 30)
                        end tell
                    end if

                else if operationKind is "apply_filter" or operationKind is "clear_filter" then
                    set sheetName to item 2 of fields
                    set rangeReference to item 3 of fields
                    set firstRowText to item 6 of fields
                    set lastRowText to item 7 of fields

                    if structuralAfter then
                        set validationText to validationText & operationKind & ":" & sheetName & "!" & rangeReference & "=displaced-by-moved-content" & (ASCII character 30)
                    else if exists worksheet sheetName of validationWorkbook then
                        set filterSheet to worksheet sheetName of validationWorkbook

                        -- Both of these were proved to survive save-as-xlsx,
                        -- close and reopen, which is the only reason a filter
                        -- can be judged from the saved copy at all.
                        set filterMode to my safeText(autofilter mode of filterSheet)
                        set firstHidden to my safeText(hidden of row (firstRowText as integer) of filterSheet)
                        set lastHidden to my safeText(hidden of row (lastRowText as integer) of filterSheet)

                        if operationKind is "apply_filter" then
                            if filterMode is not "true" then
                                error "ApplyFilter left no autofilter on the worksheet"
                            end if
                        else
                            if firstHidden is "true" or lastHidden is "true" then
                                error "ClearFilter left a row hidden"
                            end if
                        end if

                        set validationText to validationText & operationKind & ":" & sheetName & "!" & rangeReference & ":mode=" & filterMode & ",row" & firstRowText & "hidden=" & firstHidden & ",row" & lastRowText & "hidden=" & lastHidden & (ASCII character 30)
                    end if

                else if operationKind is "add_chart" then
                    set sheetName to item 2 of fields
                    set rangeReference to item 3 of fields
                    set chartName to item 4 of fields
                    set expectedType to item 6 of fields

                    -- No displacement guard: a chart is found by name, not by
                    -- address, so moving the cells around it does not make this
                    -- probe stale.
                    if exists worksheet sheetName of validationWorkbook then
                        set validationSheet to worksheet sheetName of validationWorkbook

                        -- Addressed by name, never enumerated: see the note in
                        -- the mutation branch above.
                        if not (exists chart object chartName of validationSheet) then
                            error "AddChart left no chart named " & chartName
                        end if

                        set matchedChart to chart object chartName of validationSheet

                        -- Excel does not always store the constant that was
                        -- set: asking for `line chart` stores `line markers`.
                        -- This compares against what it stores.
                        set actualChartType to (chart type of chart of matchedChart) as text
                        if actualChartType is not expectedType then
                            error "AddChart chart type validation mismatch"
                        end if

                        -- A chart that exists but plots nothing would satisfy
                        -- every other check here.
                        set seriesCount to count of series of chart of matchedChart
                        if seriesCount < 1 then
                            error "AddChart produced a chart with no series"
                        end if

                        set seriesFormula to my safeText(formula of series 1 of chart of matchedChart)

                        set validationText to validationText & "add_chart:" & sheetName & "!" & rangeReference & ":name=" & chartName & ",type=" & actualChartType & ",series=" & (seriesCount as text) & ",f1=" & seriesFormula & (ASCII character 30)
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

/// Excel writes a PDF itself, so `.xlsx -> .pdf` needs no other application.
///
/// Proven form, from `verify/probe_office_conversion_semantics.sh`:
/// `save workbook as <book> filename <path> file format PDF file format`.
const EXPORT_PDF_SCRIPT: &str = r#"
on run argv
    set stagedPath to item 1 of argv
    set pdfPath to item 2 of argv
    set beforeCount to 0
    set ownsWorkbook to false
    tell application "Microsoft Excel"
        try
            set beforeCount to count of workbooks
            open POSIX file stagedPath
            repeat 100 times
                if (count of workbooks) > beforeCount then exit repeat
                delay 0.1
            end repeat
            if (count of workbooks) is not (beforeCount + 1) then error "Excel workbook count did not increase deterministically"
            set theBook to active workbook
            if (full name of theBook as text) is not stagedPath then error "Excel active workbook identity did not match the operation copy"
            set ownsWorkbook to true
            save workbook as theBook filename pdfPath file format PDF file format
            close active workbook saving no
            set ownsWorkbook to false
            repeat 100 times
                if (count of workbooks) is beforeCount then exit repeat
                delay 0.1
            end repeat
            if (count of workbooks) is not beforeCount then error "Excel workbook count was not restored after close"
            return "AIOS_EXCEL_PDF_EXPORTED"
        on error errorMessage number errorNumber
            if ownsWorkbook then
                try
                    close active workbook saving no
                end try
            end if
            error errorMessage number errorNumber
        end try
    end tell
end run
"#;

/// A path with the extension this operation requires.
///
/// `workbook_path` cannot be reused here: it insists on .xlsx for both fields
/// and reports every problem as `spreadsheet.edit`.
fn convert_path<'a>(
    input: &'a Value,
    field: &str,
    extension: &str,
    must_exist: bool,
) -> Result<&'a str, ExcelError> {
    let path = input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ExcelError::invalid(format!("spreadsheet.convert requires {field}")))?;
    let parsed = Path::new(path);
    if !parsed.is_absolute() {
        return Err(ExcelError::invalid(format!(
            "spreadsheet.convert requires an absolute {field}"
        )));
    }
    if parsed
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        != extension
    {
        return Err(ExcelError::invalid(format!(
            "spreadsheet.convert requires a .{extension} {field}"
        )));
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

pub(crate) fn export_excel_pdf(input: &Value) -> Result<Value, ExcelError> {
    let source = convert_path(input, "source", "xlsx", true)?;
    let destination = convert_path(input, "destination", "pdf", false)?;

    let original = fs::read(source)
        .map_err(|_| ExcelError::execution("Unable to fingerprint source workbook"))?;

    // The same reason the edit path stages: a read runs its own osascript
    // invocation and does not inherit the implicit access macOS grants an app
    // for a file that app just wrote, so a workbook opened from an arbitrary
    // directory can raise a grant prompt no automated run can answer.
    let workspace = cache_workspace()?;
    let staged = workspace.join(format!("convert-{}.xlsx", uuid::Uuid::new_v4()));
    let produced = workspace.join(format!("convert-{}.pdf", uuid::Uuid::new_v4()));

    fs::copy(source, &staged)
        .map_err(|_| ExcelError::execution("Unable to stage Excel input workbook"))?;

    let staged_path = staged.to_string_lossy().to_string();
    let produced_path = produced.to_string_lossy().to_string();

    let automation = run_osascript(EXPORT_PDF_SCRIPT, &[&staged_path, &produced_path]);
    let _ = fs::remove_file(&staged);

    let confirmation = match automation {
        Ok(output) => output,
        Err(error) => {
            let _ = fs::remove_file(&produced);
            let _ = fs::remove_dir_all(&workspace);
            return Err(error);
        }
    };

    if !confirmation.contains("AIOS_EXCEL_PDF_EXPORTED") {
        let _ = fs::remove_file(&produced);
        let _ = fs::remove_dir_all(&workspace);
        return Err(ExcelError::execution("Excel did not confirm the PDF export"));
    }

    let publish = publish_without_overwrite(&produced, Path::new(destination));
    let _ = fs::remove_file(&produced);
    let _ = fs::remove_dir_all(&workspace);
    publish?;

    // The source is an input and must come back unchanged.
    let after = fs::read(source)
        .map_err(|_| ExcelError::execution("Unable to re-read source workbook"))?;
    if after != original {
        return Err(ExcelError::execution(
            "Excel PDF export modified the source workbook",
        ));
    }

    Ok(json!({
        "capability": "spreadsheet.convert",
        "selectedProvider": "microsoft-excel",
        "resourceLocation": "local",
        "inputResource": source,
        "operationResult": {
            "status": "converted",
            "application": "Excel",
            "source": source,
            "destination": destination,
            "format": "pdf",
        },
        "warnings": [],
        "confirmationConsumed": true,
        "validationResult": "converted",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Excel writes the PDF itself, so .xlsx -> .pdf needs no other application.
    ///
    /// The fixture is written by the structured layer, so this proves the whole
    /// path without a second application anywhere in it. A temp directory is
    /// safe here precisely because the export stages the workbook into Excel's
    /// own container before opening it -- Excel is never handed this path.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires Microsoft Excel"]
    fn excel_exports_a_pdf_real_e2e() {
        let root = tempfile::tempdir().unwrap();
        let workbook = root.path().join("figures.xlsx");
        let pdf = root.path().join("figures.pdf");

        crate::document::structured::create_structured_spreadsheet(&json!({
            "path": workbook.to_str().unwrap(),
            "content": "Region\tTotal\nNorth\t42",
        }))
        .unwrap();

        let before = fs::read(&workbook).unwrap();

        let exported = export_excel_pdf(&json!({
            "source": workbook.to_str().unwrap(),
            "destination": pdf.to_str().unwrap(),
        }))
        .unwrap();

        assert_eq!(exported["operationResult"]["format"], "pdf");
        assert_eq!(exported["selectedProvider"], "microsoft-excel");

        // A PDF, not an empty file with the right name.
        let produced = fs::read(&pdf).unwrap();
        assert!(produced.len() > 0);
        assert_eq!(
            &produced[..4],
            b"%PDF",
            "the destination is not a PDF at all"
        );

        // The source is an input.
        assert_eq!(fs::read(&workbook).unwrap(), before);

        // And a second export refuses rather than replacing the first.
        let refused = export_excel_pdf(&json!({
            "source": workbook.to_str().unwrap(),
            "destination": pdf.to_str().unwrap(),
        }))
        .unwrap_err();
        assert!(refused.invalid_request);
        assert_eq!(fs::read(&pdf).unwrap(), produced);
    }

    #[test]
    fn pdf_export_paths_fail_closed_before_excel_is_asked() {
        let root = tempfile::tempdir().unwrap();
        let workbook = root.path().join("book.xlsx");
        fs::write(&workbook, b"placeholder").unwrap();
        let taken = root.path().join("taken.pdf");
        fs::write(&taken, b"placeholder").unwrap();

        let source = workbook.to_str().unwrap().to_owned();

        for (label, request) in [
            ("no source", json!({"destination": "/safe/out.pdf"})),
            ("no destination", json!({"source": source.clone()})),
            (
                "relative source",
                json!({"source": "book.xlsx", "destination": "/safe/out.pdf"}),
            ),
            (
                "source is not a workbook",
                json!({
                    "source": root.path().join("book.numbers").to_str().unwrap(),
                    "destination": "/safe/out.pdf"
                }),
            ),
            (
                "destination is not a pdf",
                json!({"source": source.clone(), "destination": "/safe/out.docx"}),
            ),
            (
                "destination already exists",
                json!({"source": source.clone(), "destination": taken.to_str().unwrap()}),
            ),
        ] {
            let error = export_excel_pdf(&request).unwrap_err();
            assert!(error.invalid_request, "{label} should be a request problem");
        }

        assert_eq!(fs::read(&taken).unwrap(), b"placeholder");
    }
    use tempfile::tempdir;

    fn request(operation: Value) -> Value {
        json!({"operations": [operation]})
    }

    #[test]
    #[ignore]
    fn excel_phase_f_chart_real_e2e() {
        let fixture = std::env::var("AI_OS_EXCEL_EDIT_FIXTURE").expect("preserved fixture path");

        let original = fs::read(&fixture).unwrap();
        let root = tempdir().unwrap();
        let output = root.path().join("phase-f-chart-output.xlsx");

        // Charts are found by name rather than by position, and nothing here
        // moves cells, so all four can share one worksheet.
        let result = edit_excel_workbook(&json!({
            "source": fixture,
            "destination": output,
            "operations": [
                {"type":"add_worksheet","name":"Charted"},
                {"type":"set_cell","sheet":"Charted","row":1,"column":1,"value":"Name"},
                {"type":"set_cell","sheet":"Charted","row":1,"column":2,"value":"Score"},
                {"type":"set_cell","sheet":"Charted","row":2,"column":1,"value":"Bravo"},
                {"type":"set_cell","sheet":"Charted","row":2,"column":2,"value":2},
                {"type":"set_cell","sheet":"Charted","row":3,"column":1,"value":"Alpha"},
                {"type":"set_cell","sheet":"Charted","row":3,"column":2,"value":1},

                {"type":"add_chart","sheet":"Charted","startRow":1,"startColumn":1,
                 "endRow":3,"endColumn":2,"chartType":"column","name":"ByColumn"},
                {"type":"add_chart","sheet":"Charted","startRow":1,"startColumn":1,
                 "endRow":3,"endColumn":2,"chartType":"bar","name":"ByBar"},
                {"type":"add_chart","sheet":"Charted","startRow":1,"startColumn":1,
                 "endRow":3,"endColumn":2,"chartType":"line","name":"ByLine"},
                {"type":"add_chart","sheet":"Charted","startRow":1,"startColumn":1,
                 "endRow":3,"endColumn":2,"chartType":"pie","name":"ByPie"}
            ]
        }))
        .unwrap();

        assert_eq!(result["operationResult"]["status"], "edited-copy");
        assert_eq!(
            fs::read(&fixture).unwrap(),
            original,
            "the source workbook must be preserved"
        );

        let validation: Vec<String> = result["validation"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect();

        let entry = |name: &str| -> String {
            let prefix = format!("add_chart:Charted!A1:B3:name={name},");
            validation
                .iter()
                .find(|value| value.starts_with(&prefix))
                .unwrap_or_else(|| panic!("missing chart {name} in {validation:?}"))
                .clone()
        };

        // The series formula is the strongest evidence available: it survives
        // the reopen and names the ranges the chart actually plots, so a chart
        // that exists but plots nothing cannot pass.
        let plotted = "series=1,f1==SERIES(Charted!$B$1,Charted!$A$2:$A$3,Charted!$B$2:$B$3,1)";

        // Excel stores its own constant, which is not always the one that was
        // set: `line markers` for a line chart, `pie chart` for a pie.
        assert_eq!(
            entry("ByColumn"),
            format!("add_chart:Charted!A1:B3:name=ByColumn,type=column clustered,{plotted}")
        );
        assert_eq!(
            entry("ByBar"),
            format!("add_chart:Charted!A1:B3:name=ByBar,type=bar clustered,{plotted}")
        );
        assert_eq!(
            entry("ByLine"),
            format!("add_chart:Charted!A1:B3:name=ByLine,type=line markers,{plotted}")
        );
        assert_eq!(
            entry("ByPie"),
            format!("add_chart:Charted!A1:B3:name=ByPie,type=pie chart,{plotted}")
        );
    }

    #[test]
    fn chart_operations_require_a_known_type_and_an_explicit_name() {
        assert_eq!(
            parse_operations(&request(json!({
                "type":"add_chart","sheet":"Sheet1",
                "startRow":1,"startColumn":1,"endRow":3,"endColumn":2,
                "chartType":"line","name":"Trend"
            })))
            .unwrap()[0],
            EditOperation::AddChart {
                sheet: "Sheet1".to_owned(),
                start_row: 1,
                start_column: 1,
                end_row: 3,
                end_column: 2,
                chart_type: ChartKind::Line,
                name: "Trend".to_owned(),
            }
        );

        for rejected in [
            // A type Excel was never proved to accept.
            json!({"type":"add_chart","sheet":"S","startRow":1,"startColumn":1,"endRow":3,"endColumn":2,"chartType":"scatter","name":"T"}),
            json!({"type":"add_chart","sheet":"S","startRow":1,"startColumn":1,"endRow":3,"endColumn":2,"chartType":"pie chart","name":"T"}),
            json!({"type":"add_chart","sheet":"S","startRow":1,"startColumn":1,"endRow":3,"endColumn":2,"name":"T"}),
            // Without a name the reopened copy cannot be searched for it.
            json!({"type":"add_chart","sheet":"S","startRow":1,"startColumn":1,"endRow":3,"endColumn":2,"chartType":"pie"}),
            json!({"type":"add_chart","sheet":"S","startRow":1,"startColumn":1,"endRow":3,"endColumn":2,"chartType":"pie","name":"  "}),
            // The source range is bounded like every other range.
            json!({"type":"add_chart","sheet":"S","startRow":3,"startColumn":1,"endRow":1,"endColumn":2,"chartType":"pie","name":"T"}),
            json!({"type":"add_chart","sheet":"S","startRow":1,"startColumn":1,"endRow":3,"endColumn":257,"chartType":"pie","name":"T"}),
        ] {
            assert!(
                parse_operations(&request(rejected.clone())).is_err(),
                "{rejected} should have been rejected"
            );
        }
    }

    #[test]
    fn chart_encoding_carries_both_the_set_and_the_stored_constant() {
        let separator = FIELD_SEPARATOR.to_string();

        // Excel does not always store the constant that was set, so the record
        // carries both: one selects the AppleScript branch, the other is what
        // the reopened copy is compared against.
        for (requested, selector, stored) in [
            ("column", "column", "column clustered"),
            ("bar", "bar", "bar clustered"),
            ("line", "line", "line markers"),
            ("pie", "pie", "pie chart"),
        ] {
            let encoded = encode_operations(
                &parse_operations(&request(json!({
                    "type":"add_chart","sheet":"Sheet1",
                    "startRow":1,"startColumn":1,"endRow":3,"endColumn":2,
                    "chartType":requested,"name":"Trend"
                })))
                .unwrap(),
            );

            assert_eq!(
                encoded.split(&separator).collect::<Vec<_>>(),
                vec!["add_chart", "Sheet1", "A1:B3", "Trend", selector, stored]
            );
        }
    }

    #[test]
    #[ignore]
    fn excel_phase_e_sort_filter_real_e2e() {
        let fixture = std::env::var("AI_OS_EXCEL_EDIT_FIXTURE").expect("preserved fixture path");

        let original = fs::read(&fixture).unwrap();
        let root = tempdir().unwrap();
        let output = root.path().join("phase-e-sort-filter-output.xlsx");

        // Sorting moves rows, so a sorted worksheet cannot also carry a filter
        // assertion about a fixed row. Each capability gets its own worksheet,
        // the same rule Phase C had to learn.
        //
        //   Sorted   Name/Score, Bravo 2, Alpha 1  -> sort by Name ascending
        //   Filtered Name/Score, Bravo 2, Alpha 1  -> filter Score > 1, then clear
        let result = edit_excel_workbook(&json!({
            "source": fixture,
            "destination": output,
            "operations": [
                {"type":"add_worksheet","name":"Sorted"},
                {"type":"set_cell","sheet":"Sorted","row":1,"column":1,"value":"Name"},
                {"type":"set_cell","sheet":"Sorted","row":1,"column":2,"value":"Score"},
                {"type":"set_cell","sheet":"Sorted","row":2,"column":1,"value":"Bravo"},
                {"type":"set_cell","sheet":"Sorted","row":2,"column":2,"value":2},
                {"type":"set_cell","sheet":"Sorted","row":3,"column":1,"value":"Alpha"},
                {"type":"set_cell","sheet":"Sorted","row":3,"column":2,"value":1},

                {"type":"add_worksheet","name":"Filtered"},
                {"type":"set_cell","sheet":"Filtered","row":1,"column":1,"value":"Name"},
                {"type":"set_cell","sheet":"Filtered","row":1,"column":2,"value":"Score"},
                {"type":"set_cell","sheet":"Filtered","row":2,"column":1,"value":"Bravo"},
                {"type":"set_cell","sheet":"Filtered","row":2,"column":2,"value":2},
                {"type":"set_cell","sheet":"Filtered","row":3,"column":1,"value":"Alpha"},
                {"type":"set_cell","sheet":"Filtered","row":3,"column":2,"value":1},

                {"type":"add_worksheet","name":"Cleared"},
                {"type":"set_cell","sheet":"Cleared","row":1,"column":1,"value":"Name"},
                {"type":"set_cell","sheet":"Cleared","row":1,"column":2,"value":"Score"},
                {"type":"set_cell","sheet":"Cleared","row":2,"column":1,"value":"Bravo"},
                {"type":"set_cell","sheet":"Cleared","row":2,"column":2,"value":2},
                {"type":"set_cell","sheet":"Cleared","row":3,"column":1,"value":"Alpha"},
                {"type":"set_cell","sheet":"Cleared","row":3,"column":2,"value":1},

                {"type":"sort_range","sheet":"Sorted",
                 "startRow":1,"startColumn":1,"endRow":3,"endColumn":2,
                 "keyColumn":1,"order":"ascending","hasHeader":true},

                {"type":"apply_filter","sheet":"Filtered",
                 "startRow":1,"startColumn":1,"endRow":3,"endColumn":2,
                 "field":2,"criteria":">1"},

                {"type":"apply_filter","sheet":"Cleared",
                 "startRow":1,"startColumn":1,"endRow":3,"endColumn":2,
                 "field":2,"criteria":">1"},
                {"type":"clear_filter","sheet":"Cleared",
                 "startRow":1,"startColumn":1,"endRow":3,"endColumn":2,
                 "hasHeader":true}
            ]
        }))
        .unwrap();

        assert_eq!(result["operationResult"]["status"], "edited-copy");
        assert_eq!(
            fs::read(&fixture).unwrap(),
            original,
            "the source workbook must be preserved"
        );

        let validation: Vec<String> = result["validation"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect();

        let entry = |prefix: &str| -> String {
            validation
                .iter()
                .find(|value| value.starts_with(prefix))
                .unwrap_or_else(|| panic!("missing {prefix} in {validation:?}"))
                .clone()
        };

        // Alpha sorts above Bravo, and the header stayed at row 1 rather than
        // being sorted into the data.
        assert_eq!(
            entry("sort_range:Sorted!A1:B3:"),
            "sort_range:Sorted!A1:B3:A2=Alpha,A3=Bravo"
        );

        // Score > 1 keeps Bravo in row 2 and hides Alpha in row 3. Both the
        // filter and the hidden row survived the save and reopen.
        assert_eq!(
            entry("apply_filter:Filtered!A1:B3:"),
            "apply_filter:Filtered!A1:B3:mode=true,row2hidden=false,row3hidden=true"
        );

        // Clearing put the hidden row back without removing the filter itself.
        assert_eq!(
            entry("clear_filter:Cleared!A1:B3:"),
            "clear_filter:Cleared!A1:B3:mode=true,row2hidden=false,row3hidden=false"
        );

        // A sort moves values, so the addresses written before it are stale and
        // are reported as such rather than asserted.
        assert!(
            validation
                .iter()
                .any(|value| value == "set_cell:Sorted!A2=displaced-by-moved-content"),
            "a sorted sheet's earlier writes must be reported as displaced: {validation:?}"
        );

        // A filter only hides rows, so nothing moved and the ordinary
        // address-based validation still applies on those worksheets. This is
        // one of the two assertions that keep the displacement rule from
        // quietly widening into "skip validation whenever anything happened".
        assert_eq!(entry("set_cell:Filtered!A2="), "set_cell:Filtered!A2=Bravo");
        assert_eq!(entry("set_cell:Cleared!A3="), "set_cell:Cleared!A3=Alpha");
    }

    #[test]
    fn sort_and_filter_operations_fail_closed_on_ambiguous_input() {
        assert_eq!(
            parse_operations(&request(json!({
                "type":"sort_range","sheet":"Sheet1",
                "startRow":1,"startColumn":1,"endRow":9,"endColumn":3,
                "keyColumn":2,"order":"descending","hasHeader":false
            })))
            .unwrap()[0],
            EditOperation::SortRange {
                sheet: "Sheet1".to_owned(),
                start_row: 1,
                start_column: 1,
                end_row: 9,
                end_column: 3,
                key_column: 2,
                descending: true,
                has_header: false,
            }
        );

        for rejected in [
            // A key outside the range would sort by a column the caller did not
            // include.
            json!({"type":"sort_range","sheet":"S","startRow":1,"startColumn":2,"endRow":3,"endColumn":3,"keyColumn":1,"order":"ascending","hasHeader":true}),
            json!({"type":"sort_range","sheet":"S","startRow":1,"startColumn":1,"endRow":3,"endColumn":2,"keyColumn":3,"order":"ascending","hasHeader":true}),
            // Guessing either of these wrong is destructive, so both are required.
            json!({"type":"sort_range","sheet":"S","startRow":1,"startColumn":1,"endRow":3,"endColumn":2,"keyColumn":1,"hasHeader":true}),
            json!({"type":"sort_range","sheet":"S","startRow":1,"startColumn":1,"endRow":3,"endColumn":2,"keyColumn":1,"order":"ascending"}),
            json!({"type":"sort_range","sheet":"S","startRow":1,"startColumn":1,"endRow":3,"endColumn":2,"keyColumn":1,"order":"sideways","hasHeader":true}),
            // A header with no data row below it.
            json!({"type":"sort_range","sheet":"S","startRow":1,"startColumn":1,"endRow":1,"endColumn":2,"keyColumn":1,"order":"ascending","hasHeader":true}),
            // A field outside the filtered range's own width.
            json!({"type":"apply_filter","sheet":"S","startRow":1,"startColumn":1,"endRow":3,"endColumn":2,"field":3,"criteria":">1"}),
            json!({"type":"apply_filter","sheet":"S","startRow":1,"startColumn":1,"endRow":3,"endColumn":2,"field":0,"criteria":">1"}),
            // A filter with nothing to filter by.
            json!({"type":"apply_filter","sheet":"S","startRow":1,"startColumn":1,"endRow":3,"endColumn":2,"field":1}),
            json!({"type":"apply_filter","sheet":"S","startRow":1,"startColumn":1,"endRow":3,"endColumn":2,"field":1,"criteria":"  "}),
            json!({"type":"clear_filter","sheet":"S","startRow":1,"startColumn":1,"endRow":3,"endColumn":2}),
        ] {
            assert!(
                parse_operations(&request(rejected.clone())).is_err(),
                "{rejected} should have been rejected"
            );
        }
    }

    #[test]
    fn sort_and_filter_encode_probe_rows_that_skip_a_header() {
        let separator = FIELD_SEPARATOR.to_string();

        let encoded = encode_operations(
            &parse_operations(&request(json!({
                "type":"sort_range","sheet":"Sheet1",
                "startRow":1,"startColumn":1,"endRow":3,"endColumn":2,
                "keyColumn":1,"order":"ascending","hasHeader":true
            })))
            .unwrap(),
        );

        // The sort key anchors on the range's first row, but the probes read the
        // first two DATA rows -- reading the header back would prove nothing.
        assert_eq!(
            encoded.split(&separator).collect::<Vec<_>>(),
            vec!["sort_range", "Sheet1", "A1:B3", "A1", "ascending", "yes", "A2", "A3"]
        );

        let encoded = encode_operations(
            &parse_operations(&request(json!({
                "type":"sort_range","sheet":"Sheet1",
                "startRow":1,"startColumn":1,"endRow":3,"endColumn":2,
                "keyColumn":2,"order":"descending","hasHeader":false
            })))
            .unwrap(),
        );
        assert_eq!(
            encoded.split(&separator).collect::<Vec<_>>(),
            vec!["sort_range", "Sheet1", "A1:B3", "B1", "descending", "no", "B1", "B2"]
        );

        let encoded = encode_operations(
            &parse_operations(&request(json!({
                "type":"apply_filter","sheet":"Sheet1",
                "startRow":1,"startColumn":1,"endRow":3,"endColumn":2,
                "field":2,"criteria":">1"
            })))
            .unwrap(),
        );
        assert_eq!(
            encoded.split(&separator).collect::<Vec<_>>(),
            vec!["apply_filter", "Sheet1", "A1:B3", "2", ">1", "2", "3"]
        );

        let encoded = encode_operations(
            &parse_operations(&request(json!({
                "type":"clear_filter","sheet":"Sheet1",
                "startRow":1,"startColumn":1,"endRow":3,"endColumn":2,
                "hasHeader":true
            })))
            .unwrap(),
        );
        assert_eq!(
            encoded.split(&separator).collect::<Vec<_>>(),
            vec!["clear_filter", "Sheet1", "A1:B3", "", "", "2", "3"]
        );
    }

    #[test]
    #[ignore]
    fn excel_phase_d_formatting_real_e2e() {
        let fixture = std::env::var("AI_OS_EXCEL_EDIT_FIXTURE").expect("preserved fixture path");

        let original = fs::read(&fixture).unwrap();
        let root = tempdir().unwrap();
        let output = root.path().join("phase-d-formatting-output.xlsx");

        // Every attribute here was proved on real Excel 16.78 to survive
        // save-as-xlsx, close and reopen, which is the only reason validation
        // can read them back off disk at all.
        let result = edit_excel_workbook(&json!({
            "source": fixture,
            "destination": output,
            "operations": [
                {"type":"add_worksheet","name":"Formatted"},
                {"type":"set_cell","sheet":"Formatted","row":1,"column":1,"value":"Name"},
                {"type":"set_cell","sheet":"Formatted","row":1,"column":2,"value":"Amount"},
                {"type":"set_cell","sheet":"Formatted","row":2,"column":1,"value":"Bravo"},
                {"type":"set_cell","sheet":"Formatted","row":2,"column":2,"value":1234.5},

                // A header row: several attributes in one operation.
                {"type":"format_cells","sheet":"Formatted",
                 "startRow":1,"startColumn":1,"endRow":1,"endColumn":2,
                 "bold":true,"italic":true,"fontSize":14,"fontName":"Courier New",
                 "fillColorIndex":6},

                // A data column: number format only, and deliberately not bold,
                // so a formatter that painted the whole sheet would be caught.
                {"type":"format_cells","sheet":"Formatted",
                 "startRow":2,"startColumn":2,"endRow":2,"endColumn":2,
                 "numberFormat":"0.00"},

                {"type":"set_column_width","sheet":"Formatted","column":1,"width":24},
                {"type":"set_row_height","sheet":"Formatted","row":1,"height":30}
            ]
        }))
        .unwrap();

        assert_eq!(result["operationResult"]["status"], "edited-copy");
        assert_eq!(
            fs::read(&fixture).unwrap(),
            original,
            "the source workbook must be preserved"
        );

        let validation: Vec<String> = result["validation"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect();

        let entry = |prefix: &str| -> String {
            validation
                .iter()
                .find(|value| value.starts_with(prefix))
                .unwrap_or_else(|| panic!("missing {prefix} in {validation:?}"))
                .clone()
        };

        // Read out of the reopened saved copy at the top-left cell of the range.
        assert_eq!(
            entry("format_cells:Formatted!A1:B1:"),
            "format_cells:Formatted!A1:B1:bold=true;italic=true;size=14;font=Courier New;fill=6;"
        );
        assert_eq!(
            entry("format_cells:Formatted!B2:B2:"),
            "format_cells:Formatted!B2:B2:number=0.00;"
        );

        // Width and height come back as reals and may settle a fraction away
        // from the requested whole number, so the observed value is reported and
        // the assertion is the same tolerance the adapter applies.
        for (prefix, requested) in [
            ("set_column_width:Formatted!1=", 24.0_f64),
            ("set_row_height:Formatted!1=", 30.0_f64),
        ] {
            let observed = entry(prefix);
            let measured: f64 = observed
                .trim_start_matches(prefix)
                .parse()
                .unwrap_or_else(|_| panic!("unparseable measure in {observed}"));
            assert!(
                (measured - requested).abs() <= 0.5,
                "{observed} is more than half a unit from {requested}"
            );
        }
    }

    #[test]
    fn formatting_operations_validate_ranges_and_attributes() {
        let formatted = parse_operations(&request(json!({
            "type":"format_cells","sheet":"Sheet1",
            "startRow":1,"startColumn":1,"endRow":2,"endColumn":3,
            "bold":true,"numberFormat":"0.00"
        })))
        .unwrap();

        assert_eq!(
            formatted[0],
            EditOperation::FormatCells {
                sheet: "Sheet1".to_owned(),
                start_row: 1,
                start_column: 1,
                end_row: 2,
                end_column: 3,
                bold: Some(true),
                italic: None,
                font_size: None,
                font_name: None,
                fill_color_index: None,
                number_format: Some("0.00".to_owned()),
            }
        );

        for rejected in [
            // An inverted range.
            json!({"type":"format_cells","sheet":"S","startRow":3,"startColumn":1,"endRow":2,"endColumn":1,"bold":true}),
            json!({"type":"format_cells","sheet":"S","startRow":1,"startColumn":3,"endRow":1,"endColumn":2,"bold":true}),
            // Out of bounds on either dimension.
            json!({"type":"format_cells","sheet":"S","startRow":0,"startColumn":1,"endRow":1,"endColumn":1,"bold":true}),
            json!({"type":"format_cells","sheet":"S","startRow":1,"startColumn":1,"endRow":1,"endColumn":257,"bold":true}),
            // Nothing to do: this would change nothing and then validate clean.
            json!({"type":"format_cells","sheet":"S","startRow":1,"startColumn":1,"endRow":1,"endColumn":1}),
            // Attributes outside Excel's own limits, or of the wrong shape.
            json!({"type":"format_cells","sheet":"S","startRow":1,"startColumn":1,"endRow":1,"endColumn":1,"fontSize":0}),
            json!({"type":"format_cells","sheet":"S","startRow":1,"startColumn":1,"endRow":1,"endColumn":1,"fontSize":410}),
            json!({"type":"format_cells","sheet":"S","startRow":1,"startColumn":1,"endRow":1,"endColumn":1,"fillColorIndex":57}),
            json!({"type":"format_cells","sheet":"S","startRow":1,"startColumn":1,"endRow":1,"endColumn":1,"bold":"yes"}),
            json!({"type":"format_cells","sheet":"S","startRow":1,"startColumn":1,"endRow":1,"endColumn":1,"numberFormat":"   "}),
            json!({"type":"format_cells","sheet":"S","startRow":1,"startColumn":1,"endRow":1,"endColumn":1,"fontName":""}),
            // Zero hides a line rather than resizing it, so it is not accepted
            // as a width or a height.
            json!({"type":"set_column_width","sheet":"S","column":1,"width":0}),
            json!({"type":"set_column_width","sheet":"S","column":1,"width":256}),
            json!({"type":"set_row_height","sheet":"S","row":1,"height":0}),
            json!({"type":"set_row_height","sheet":"S","row":1,"height":410}),
        ] {
            assert!(
                parse_operations(&request(rejected.clone())).is_err(),
                "{rejected} should have been rejected"
            );
        }
    }

    #[test]
    fn formatting_encodes_fixed_arity_fields_with_a_terminator() {
        let separator = FIELD_SEPARATOR.to_string();

        let encoded = encode_operations(
            &parse_operations(&request(json!({
                "type":"format_cells","sheet":"Sheet1",
                "startRow":2,"startColumn":2,"endRow":4,"endColumn":3,
                "italic":true,"fillColorIndex":6
            })))
            .unwrap(),
        );
        let fields: Vec<&str> = encoded.split(&separator).collect();

        // Absent attributes are empty fields rather than missing ones, so every
        // record has the same shape, and the terminator keeps an optional value
        // from ever being the trailing field.
        assert_eq!(
            fields,
            vec![
                "format_cells",
                "Sheet1",
                "B2:C4",
                "B2",
                "",
                "true",
                "",
                "",
                "6",
                "",
                "end",
            ]
        );

        let encoded = encode_operations(
            &parse_operations(&request(
                json!({"type":"set_column_width","sheet":"Sheet1","column":3,"width":24}),
            ))
            .unwrap(),
        );
        assert_eq!(
            encoded.split(&separator).collect::<Vec<_>>(),
            vec!["set_column_width", "Sheet1", "3", "24"]
        );

        let encoded = encode_operations(
            &parse_operations(&request(
                json!({"type":"set_row_height","sheet":"Sheet1","row":5,"height":30}),
            ))
            .unwrap(),
        );
        assert_eq!(
            encoded.split(&separator).collect::<Vec<_>>(),
            vec!["set_row_height", "Sheet1", "5", "30"]
        );
    }

    #[test]
    #[ignore]
    fn excel_phase_c_structural_real_e2e() {
        let fixture = std::env::var("AI_OS_EXCEL_EDIT_FIXTURE").expect("preserved fixture path");

        let original = fs::read(&fixture).unwrap();
        let root = tempdir().unwrap();
        let output = root.path().join("phase-c-structural-output.xlsx");

        // Validation reads the saved copy once, at the end, so every probe sees
        // the final state of its worksheet and not the state right after its
        // own operation. Each structural operation therefore gets a worksheet
        // to itself: the final state IS the post-operation state, and the
        // read-back proves that one operation rather than a composite.
        //
        // "Workflow" is the exception, and is deliberately the sequence the
        // previous Phase C attempt failed on.
        //
        //   InsCol   A1=R1 B1=C1 C1=C2   insert column B
        //   DelCol   A1=R1 B1=C1 C1=C2   delete column A
        //   InsRow   A1=X1 A2=R2 A3=R3   insert row 2
        //   DelRow   A1=X1 A2=R2 A3=R3   delete row 1
        //   Workflow A1=R1 B1=C1 C1=C2   insert column B, then delete column A
        let result = edit_excel_workbook(&json!({
            "source": fixture,
            "destination": output,
            "operations": [
                {"type":"add_worksheet","name":"InsCol"},
                {"type":"set_cell","sheet":"InsCol","row":1,"column":1,"value":"R1"},
                {"type":"set_cell","sheet":"InsCol","row":1,"column":2,"value":"C1"},
                {"type":"set_cell","sheet":"InsCol","row":1,"column":3,"value":"C2"},

                {"type":"add_worksheet","name":"DelCol"},
                {"type":"set_cell","sheet":"DelCol","row":1,"column":1,"value":"R1"},
                {"type":"set_cell","sheet":"DelCol","row":1,"column":2,"value":"C1"},
                {"type":"set_cell","sheet":"DelCol","row":1,"column":3,"value":"C2"},

                {"type":"add_worksheet","name":"InsRow"},
                {"type":"set_cell","sheet":"InsRow","row":1,"column":1,"value":"X1"},
                {"type":"set_cell","sheet":"InsRow","row":2,"column":1,"value":"R2"},
                {"type":"set_cell","sheet":"InsRow","row":3,"column":1,"value":"R3"},

                {"type":"add_worksheet","name":"DelRow"},
                {"type":"set_cell","sheet":"DelRow","row":1,"column":1,"value":"X1"},
                {"type":"set_cell","sheet":"DelRow","row":2,"column":1,"value":"R2"},
                {"type":"set_cell","sheet":"DelRow","row":3,"column":1,"value":"R3"},

                {"type":"add_worksheet","name":"Workflow"},
                {"type":"set_cell","sheet":"Workflow","row":1,"column":1,"value":"R1"},
                {"type":"set_cell","sheet":"Workflow","row":1,"column":2,"value":"C1"},
                {"type":"set_cell","sheet":"Workflow","row":1,"column":3,"value":"C2"},

                {"type":"insert_column","sheet":"InsCol","column":2},
                {"type":"delete_column","sheet":"DelCol","column":1},
                {"type":"insert_row","sheet":"InsRow","row":2},
                {"type":"delete_row","sheet":"DelRow","row":1},

                {"type":"insert_column","sheet":"Workflow","column":2},
                {"type":"delete_column","sheet":"Workflow","column":1}
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

        // Two entries share a prefix: the probe read out of the reopened saved
        // copy, and the used-range snapshot the adapter recorded live while
        // making the change. Matching the exact probe string keeps this
        // judging the file on disk. The snapshot is reported, not asserted,
        // because Excel's used range does not always shrink on deletion.
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

        // R1 C1 C2 -> R1 [] C1 C2. The row moved right by exactly one column.
        probed(
            "insert_column:InsCol!B:B:",
            "insert_column:InsCol!B:B:B1=,C1=C1",
        );

        // R1 C1 C2 -> C1 C2. The row moved left by exactly one column and R1
        // is gone with the column that held it.
        probed(
            "delete_column:DelCol!A:A:",
            "delete_column:DelCol!A:A:A1=C1,B1=C2",
        );

        // X1 R2 R3 -> X1 [] R2 R3. The column moved down by exactly one row.
        probed("insert_row:InsRow!2:2:", "insert_row:InsRow!2:2:A2=,A3=R2");

        // X1 R2 R3 -> R2 R3. The column moved up by exactly one row and X1 is
        // gone with the row that held it.
        probed("delete_row:DelRow!1:1:", "delete_row:DelRow!1:1:A1=R2,A2=R3");

        // The workflow the previous attempt failed on, read off disk: an empty
        // leading column, then the original data intact behind it.
        probed(
            "delete_column:Workflow!A:A:",
            "delete_column:Workflow!A:A:A1=,B1=C1",
        );

        // Those cell writes were moved by the structural changes, so their
        // original addresses are stale. The adapter says so instead of
        // asserting an old address and calling a working shift a failed write.
        // The probes above are what evidences the writes actually landed.
        for displaced in [
            "set_cell:InsCol!B1=displaced-by-moved-content",
            "set_cell:DelCol!A1=displaced-by-moved-content",
            "set_cell:InsRow!A2=displaced-by-moved-content",
            "set_cell:DelRow!A1=displaced-by-moved-content",
            "set_cell:Workflow!C1=displaced-by-moved-content",
        ] {
            assert!(
                validation.iter().any(|value| value == displaced),
                "expected `{displaced}` among {validation:?}"
            );
        }
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
