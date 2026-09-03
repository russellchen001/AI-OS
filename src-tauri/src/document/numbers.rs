//! Apple Numbers adapter.
//!
//! Every AppleScript form here was proved on real Numbers 15.3.1 by
//! `verify/probe_iwork_pages_numbers_semantics.sh` before it was written. What
//! the probe settled, and what it forces:
//!
//! * **A new table is already 22 rows x 7 columns.** Numbers has no "used
//!   range", so a read has to trim trailing empty rows and columns itself or it
//!   returns a wall of blanks.
//! * **An empty cell reads back as `missing value`**, not as an empty string.
//! * **Sheet and table names are localized** (`工作表 1`, `表格 1`). Everything
//!   is addressed by index; names are read only to report them.
//! * **`make new sheet` fails with -10000.** A Numbers create is single-sheet,
//!   and that is declared as a provider limit rather than worked around.
//! * `add row below last row` / `add column after last column` do work, so a
//!   create is not capped at the default table size.
//! * `tab` inside a Numbers `tell` block really is ASCII 9 — unlike Excel,
//!   whose dictionary takes the word over. `ASCII character 9` is used anyway,
//!   because relying on an application's dictionary *not* defining a term is
//!   how the Excel adapter got a literal "tab" in its output.

use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const MAX_SHEETS: usize = 16;
const MAX_ROWS: usize = 200;
const MAX_COLUMNS: usize = 64;
const MAX_CELL_CHARS: usize = 256;
const MAX_CREATE_CHARS: usize = 65_536;

#[derive(Debug)]
pub(crate) struct NumbersError {
    pub(crate) invalid_request: bool,
    pub(crate) message: String,
}

impl NumbersError {
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

fn require_local_path<'a>(
    input: &'a Value,
    operation: &str,
    must_exist: bool,
) -> Result<&'a str, NumbersError> {
    let path = input
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| NumbersError::invalid(format!("{operation} requires path")))?;

    let target = Path::new(path);

    if !target.is_absolute() {
        return Err(NumbersError::invalid(format!(
            "{operation} requires an absolute path"
        )));
    }

    let extension = target
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    if extension != "numbers" {
        return Err(NumbersError::invalid(format!(
            "{operation} requires a .numbers path"
        )));
    }

    if must_exist {
        if !target.is_file() {
            return Err(NumbersError::invalid(format!(
                "{operation} path does not exist"
            )));
        }

        return Ok(path);
    }

    if target.exists() {
        return Err(NumbersError::invalid(format!(
            "{operation} refuses to overwrite an existing path"
        )));
    }

    let parent = target
        .parent()
        .ok_or_else(|| NumbersError::invalid(format!("{operation} requires an absolute path")))?;

    if !parent.is_dir() {
        return Err(NumbersError::invalid(format!(
            "{operation} parent directory does not exist"
        )));
    }

    Ok(path)
}

fn run_osascript(script: &str, args: &[&str]) -> Result<String, NumbersError> {
    let mut child = Command::new("/usr/bin/osascript")
        .arg("-")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| NumbersError::execution("Unable to start Numbers AppleScript automation"))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| NumbersError::execution("Unable to open AppleScript input"))?;

        stdin
            .write_all(script.as_bytes())
            .map_err(|_| NumbersError::execution("Unable to write Numbers AppleScript"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|_| NumbersError::execution("Numbers AppleScript did not complete"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(NumbersError::execution(if stderr.is_empty() {
            "Numbers AppleScript automation failed".to_owned()
        } else {
            format!("Numbers AppleScript automation failed: {stderr}")
        }));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

const READ_SCRIPT: &str = r#"
on safeCell(cellValue, maximumLength)
    if cellValue is missing value then return ""

    set outputText to cellValue as text

    set oldDelimiters to AppleScript's text item delimiters
    repeat with separatorValue in {return, linefeed, (ASCII character 9)}
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
end safeCell

on joinCells(cellValues, columnLimit, maximumLength)
    set renderedValues to {}

    repeat with columnIndex from 1 to columnLimit
        if columnIndex <= (count of cellValues) then
            set end of renderedValues to my safeCell(contents of item columnIndex of cellValues, maximumLength)
        else
            set end of renderedValues to ""
        end if
    end repeat

    set oldDelimiters to AppleScript's text item delimiters
    set AppleScript's text item delimiters to (ASCII character 9)
    set rowText to renderedValues as text
    set AppleScript's text item delimiters to oldDelimiters

    return rowText
end joinCells

on run argv
    set targetPath to item 1 of argv
    set sheetLimit to (item 2 of argv) as integer
    set rowLimit to (item 3 of argv) as integer
    set columnLimit to (item 4 of argv) as integer
    set cellChars to (item 5 of argv) as integer

    set targetAlias to POSIX file targetPath as alias
    set openedDocument to missing value
    set wasAlreadyOpen to false

    tell application id "com.apple.Numbers"
        try
            -- A document the caller already has open is theirs: read it where
            -- it is and leave it open.
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

            set totalSheets to count of sheets of openedDocument
            set emittedSheets to totalSheets
            if emittedSheets > sheetLimit then
                set emittedSheets to sheetLimit
            end if

            -- Sheet names are localized, so they are reported but never used to
            -- find anything.
            set sheetNames to {}
            repeat with sheetIndex from 1 to totalSheets
                set end of sheetNames to my safeCell(name of sheet sheetIndex of openedDocument, cellChars)
            end repeat

            set oldDelimiters to AppleScript's text item delimiters
            set AppleScript's text item delimiters to (ASCII character 31)
            set joinedNames to sheetNames as text
            set AppleScript's text item delimiters to oldDelimiters

            set outputText to "AIOS_SHEET_COUNT=" & totalSheets & linefeed
            set outputText to outputText & "AIOS_SHEETS=" & joinedNames & linefeed

            if totalSheets > emittedSheets then
                set outputText to outputText & "AIOS_SELECTION_TRUNCATED=true" & linefeed
            else
                set outputText to outputText & "AIOS_SELECTION_TRUNCATED=false" & linefeed
            end if

            repeat with sheetIndex from 1 to emittedSheets
                set currentSheet to sheet sheetIndex of openedDocument
                set tableTotal to count of tables of currentSheet

                set sheetTruncated to false
                if tableTotal > 1 then set sheetTruncated to true

                set rowTotal to 0
                set columnTotal to 0
                set sheetContent to ""

                if tableTotal > 0 then
                    tell table 1 of currentSheet
                        set rowTotal to count of rows
                        set columnTotal to count of columns

                        set emittedRows to rowTotal
                        if emittedRows > rowLimit then
                            set emittedRows to rowLimit
                            set sheetTruncated to true
                        end if

                        set emittedColumns to columnTotal
                        if emittedColumns > columnLimit then
                            set emittedColumns to columnLimit
                            set sheetTruncated to true
                        end if

                        repeat with rowIndex from 1 to emittedRows
                            set rowValues to value of every cell of row rowIndex
                            set sheetContent to sheetContent & my joinCells(rowValues, emittedColumns, cellChars) & linefeed
                        end repeat
                    end tell
                end if

                set outputText to outputText & linefeed & "AIOS_SHEET_BEGIN" & linefeed
                set outputText to outputText & "AIOS_SHEET=" & (item sheetIndex of sheetNames) & linefeed
                set outputText to outputText & "AIOS_TABLES=" & tableTotal & linefeed
                set outputText to outputText & "AIOS_TOTAL_ROWS=" & rowTotal & linefeed
                set outputText to outputText & "AIOS_TOTAL_COLUMNS=" & columnTotal & linefeed

                if sheetTruncated then
                    set outputText to outputText & "AIOS_TRUNCATED=true" & linefeed
                else
                    set outputText to outputText & "AIOS_TRUNCATED=false" & linefeed
                end if

                set outputText to outputText & "AIOS_CONTENT_BEGIN" & linefeed
                set outputText to outputText & sheetContent
                set outputText to outputText & "AIOS_SHEET_END"
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
on splitText(sourceText, delimiterText)
    if sourceText is "" then return {}

    set oldDelimiters to AppleScript's text item delimiters
    set AppleScript's text item delimiters to delimiterText
    set resultItems to text items of sourceText
    set AppleScript's text item delimiters to oldDelimiters

    return resultItems
end splitText

on run argv
    set targetPath to item 1 of argv
    set sourceContent to item 2 of argv
    set createdDocument to missing value

    set sourceRows to my splitText(sourceContent, linefeed)
    set neededRows to count of sourceRows
    set neededColumns to 1

    repeat with rowText in sourceRows
        set rowCells to my splitText(contents of rowText, (ASCII character 9))
        if (count of rowCells) > neededColumns then
            set neededColumns to count of rowCells
        end if
    end repeat

    tell application id "com.apple.Numbers"
        try
            -- Single sheet on purpose: `make new sheet` fails with -10000 on
            -- real Numbers, so a Numbers create cannot be multi-sheet and does
            -- not pretend to be.
            set createdDocument to make new document

            tell table 1 of sheet 1 of createdDocument
                -- A new table is 22 x 7. Grow it when the content needs more;
                -- it is never shrunk, because removing rows down to the header
                -- is not a form the probe established.
                repeat while (count of rows) < neededRows
                    add row below last row
                end repeat

                repeat while (count of columns) < neededColumns
                    add column after last column
                end repeat

                repeat with rowIndex from 1 to neededRows
                    set rowCells to my splitText(item rowIndex of sourceRows, (ASCII character 9))

                    repeat with columnIndex from 1 to (count of rowCells)
                        set cellText to contents of item columnIndex of rowCells

                        if cellText is not "" then
                            set value of cell columnIndex of row rowIndex to cellText
                        end if
                    end repeat
                end repeat
            end tell

            save createdDocument in POSIX file targetPath
            close createdDocument saving no
            set createdDocument to missing value

            return "AIOS_NUMBERS_CREATED"
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

/// Numbers has no used range, so trailing empty rows and columns are removed
/// here. Without this every read of a small table returns the 22x7 of blanks a
/// new table starts life with.
fn trim_grid(content: &str) -> (Vec<Vec<String>>, u64, u64) {
    let mut grid: Vec<Vec<String>> = content
        .lines()
        .map(|line| line.split('\t').map(str::to_owned).collect())
        .collect();

    while grid
        .last()
        .is_some_and(|row| row.iter().all(|cell| cell.trim().is_empty()))
    {
        grid.pop();
    }

    let width = grid.iter().map(Vec::len).max().unwrap_or(0);
    let mut columns = width;

    while columns > 0
        && grid
            .iter()
            .all(|row| row.get(columns - 1).is_none_or(|cell| cell.trim().is_empty()))
    {
        columns -= 1;
    }

    for row in &mut grid {
        row.truncate(columns);
        row.resize(columns, String::new());
    }

    let rows = grid.len() as u64;
    (grid, rows, columns as u64)
}

fn parse_read_output(path: &str, output: &str) -> Result<Value, NumbersError> {
    let (_, payload) = output
        .split_once("AIOS_SHEET_COUNT=")
        .ok_or_else(|| NumbersError::execution("Numbers read returned no sheet count"))?;
    let (sheet_count, payload) = payload
        .split_once("\nAIOS_SHEETS=")
        .ok_or_else(|| NumbersError::execution("Numbers read returned no sheet names"))?;
    let (sheet_names, payload) = payload
        .split_once("\nAIOS_SELECTION_TRUNCATED=")
        .ok_or_else(|| NumbersError::execution("Numbers read returned no truncation flag"))?;
    let (selection_truncated, blocks) = payload
        .split_once("\nAIOS_SHEET_BEGIN\n")
        .ok_or_else(|| NumbersError::execution("Numbers read returned no sheets"))?;

    let sheet_count = sheet_count
        .trim()
        .parse::<u64>()
        .map_err(|_| NumbersError::execution("Numbers read returned an unreadable sheet count"))?;

    let sheet_names: Vec<String> = if sheet_names.trim().is_empty() {
        Vec::new()
    } else {
        sheet_names
            .split('\u{1f}')
            .map(str::trim)
            .map(str::to_owned)
            .collect()
    };

    let mut selection_truncated = selection_truncated.trim() == "true";
    let mut sheets = Vec::new();

    for block in blocks.split("\nAIOS_SHEET_BEGIN\n") {
        let (block, _) = block
            .split_once("\nAIOS_SHEET_END")
            .ok_or_else(|| NumbersError::execution("Numbers read returned an unterminated sheet"))?;

        let (_, payload) = block
            .split_once("AIOS_SHEET=")
            .ok_or_else(|| NumbersError::execution("Numbers read returned a nameless sheet"))?;
        let (name, payload) = payload
            .split_once("\nAIOS_TABLES=")
            .ok_or_else(|| NumbersError::execution("Numbers read returned no table count"))?;
        let (tables, payload) = payload
            .split_once("\nAIOS_TOTAL_ROWS=")
            .ok_or_else(|| NumbersError::execution("Numbers read returned no row total"))?;
        let (total_rows, payload) = payload
            .split_once("\nAIOS_TOTAL_COLUMNS=")
            .ok_or_else(|| NumbersError::execution("Numbers read returned no column total"))?;
        let (total_columns, payload) = payload
            .split_once("\nAIOS_TRUNCATED=")
            .ok_or_else(|| NumbersError::execution("Numbers read returned no sheet truncation"))?;
        let (sheet_truncated, content) = payload
            .split_once("\nAIOS_CONTENT_BEGIN\n")
            .ok_or_else(|| NumbersError::execution("Numbers read returned no content"))?;

        let parse = |value: &str, label: &str| {
            value.trim().parse::<u64>().map_err(|_| {
                NumbersError::execution(format!("Numbers read returned an unreadable {label}"))
            })
        };

        let (grid, rows, columns) = trim_grid(content);
        let rendered = grid
            .iter()
            .map(|row| row.join("\t"))
            .collect::<Vec<_>>()
            .join("\n");

        let sheet_truncated = sheet_truncated.trim() == "true";
        selection_truncated = selection_truncated || sheet_truncated;

        sheets.push(json!({
            "name": name.trim(),
            "tableCount": parse(tables, "table count")?,
            "rows": rows,
            "columns": columns,
            "totalRows": parse(total_rows, "row total")?,
            "totalColumns": parse(total_columns, "column total")?,
            "truncated": sheet_truncated,
            "content": rendered,
        }));
    }

    if sheets.len() == 1 {
        let sheet = &sheets[0];

        return Ok(json!({
            "path": path,
            "sheet": sheet["name"],
            "status": "table",
            "rows": sheet["rows"],
            "columns": sheet["columns"],
            "totalRows": sheet["totalRows"],
            "totalColumns": sheet["totalColumns"],
            "tableCount": sheet["tableCount"],
            "worksheetCount": sheet_count,
            "worksheetNames": sheet_names,
            "truncated": selection_truncated,
            "content": sheet["content"],
            "provider": "apple-iwork",
        }));
    }

    Ok(json!({
        "path": path,
        "status": "workbook",
        "worksheetCount": sheet_count,
        "worksheetNames": sheet_names,
        "sheets": sheets,
        "truncated": selection_truncated,
        "provider": "apple-iwork",
    }))
}

pub(crate) fn read_numbers_spreadsheet(input: &Value) -> Result<Value, NumbersError> {
    let path = require_local_path(input, "spreadsheet.read", true)?;

    let output = run_osascript(
        READ_SCRIPT,
        &[
            path,
            &MAX_SHEETS.to_string(),
            &MAX_ROWS.to_string(),
            &MAX_COLUMNS.to_string(),
            &MAX_CELL_CHARS.to_string(),
        ],
    )?;

    parse_read_output(path, &output)
}

pub(crate) fn create_numbers_spreadsheet(input: &Value) -> Result<Value, NumbersError> {
    let path = require_local_path(input, "spreadsheet.create", false)?;

    let content = input
        .get("content")
        .and_then(Value::as_str)
        .map(|value| value.trim_end_matches('\n'))
        .unwrap_or("");

    if content.trim().is_empty() {
        return Err(NumbersError::invalid("spreadsheet.create requires content"));
    }

    if content.chars().count() > MAX_CREATE_CHARS {
        return Err(NumbersError::invalid(
            "spreadsheet.create content exceeds the supported length",
        ));
    }

    let rows: Vec<&str> = content.split('\n').collect();
    let columns = rows
        .iter()
        .map(|row| row.split('\t').count())
        .max()
        .unwrap_or(0);

    if rows.len() > MAX_ROWS || columns > MAX_COLUMNS {
        return Err(NumbersError::invalid(format!(
            "spreadsheet.create supports at most {MAX_ROWS} rows and {MAX_COLUMNS} columns"
        )));
    }

    let output = run_osascript(CREATE_SCRIPT, &[path, content])?;

    if !output.contains("AIOS_NUMBERS_CREATED") {
        return Err(NumbersError::execution(
            "Numbers did not confirm the new spreadsheet",
        ));
    }

    if !Path::new(path).is_file() {
        return Err(NumbersError::execution(
            "Numbers did not leave a spreadsheet at the path",
        ));
    }

    Ok(json!({
        "path": path,
        "status": "created",
        "rows": rows.len(),
        "columns": columns,
        "sheets": 1,
        "provider": "apple-iwork",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn numbers_trims_the_blank_grid_a_new_table_starts_with() {
        // A new Numbers table is 22 x 7 whether or not anything was written to
        // it, so an untrimmed read of a two-cell sheet is a wall of blanks.
        let mut wide = String::new();
        wide.push_str("Region\tTotal\t\t\t\t\t\n");
        wide.push_str("North\t42\t\t\t\t\t\n");
        for _ in 0..20 {
            wide.push_str("\t\t\t\t\t\t\n");
        }

        let (grid, rows, columns) = trim_grid(&wide);
        assert_eq!((rows, columns), (2, 2));
        assert_eq!(grid[0], vec!["Region", "Total"]);
        assert_eq!(grid[1], vec!["North", "42"]);

        // A blank cell inside the content is kept; only trailing blank lines
        // and columns go.
        let (grid, rows, columns) = trim_grid("a\t\tc\t\nd\te\tf\t\n\t\t\t\n");
        assert_eq!((rows, columns), (2, 3));
        assert_eq!(grid[0], vec!["a", "", "c"]);

        // An entirely empty sheet trims to nothing rather than to one blank row.
        assert_eq!(trim_grid("\t\t\n\t\t\n").1, 0);
        assert_eq!(trim_grid("").1, 0);
    }

    #[test]
    fn numbers_paths_and_create_bounds_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let existing = root.path().join("existing.numbers");
        fs::write(&existing, b"placeholder").unwrap();

        let read = |value: &Value| require_local_path(value, "spreadsheet.read", true).is_ok();
        let create = |value: &Value| require_local_path(value, "spreadsheet.create", false).is_ok();

        assert!(read(&json!({"path": existing.to_str().unwrap()})));
        assert!(create(&json!({"path": root.path().join("fresh.numbers").to_str().unwrap()})));

        for rejected in [
            json!({}),
            json!({"path": "   "}),
            json!({"path": "relative.numbers"}),
            json!({"path": root.path().join("wrong.xlsx").to_str().unwrap()}),
        ] {
            assert!(!read(&rejected), "{rejected} should be rejected");
            assert!(!create(&rejected), "{rejected} should be rejected");
        }

        assert!(!read(&json!({"path": root.path().join("missing.numbers").to_str().unwrap()})));
        assert!(!create(&json!({"path": existing.to_str().unwrap()})));

        // Content bounds are checked before Numbers is ever launched.
        let fresh = root.path().join("bounds.numbers");
        let fresh = fresh.to_str().unwrap();

        assert!(create_numbers_spreadsheet(&json!({"path": fresh})).is_err());
        assert!(create_numbers_spreadsheet(&json!({"path": fresh, "content": "   "})).is_err());

        let too_many_rows = (0..=MAX_ROWS).map(|_| "a").collect::<Vec<_>>().join("\n");
        assert!(
            create_numbers_spreadsheet(&json!({"path": fresh, "content": too_many_rows})).is_err()
        );

        let too_many_columns = (0..=MAX_COLUMNS).map(|_| "a").collect::<Vec<_>>().join("\t");
        assert!(
            create_numbers_spreadsheet(&json!({"path": fresh, "content": too_many_columns}))
                .is_err()
        );
    }

    #[test]
    fn numbers_read_parser_returns_a_trimmed_single_sheet() {
        let output = "AIOS_SHEET_COUNT=1\nAIOS_SHEETS=\u{5de5}\u{4f5c}\u{8868} 1\n\
AIOS_SELECTION_TRUNCATED=false\n\
AIOS_SHEET_BEGIN\nAIOS_SHEET=\u{5de5}\u{4f5c}\u{8868} 1\nAIOS_TABLES=1\n\
AIOS_TOTAL_ROWS=22\nAIOS_TOTAL_COLUMNS=7\nAIOS_TRUNCATED=false\nAIOS_CONTENT_BEGIN\n\
Region\tTotal\t\t\t\t\t\nNorth\t42\t\t\t\t\t\n\t\t\t\t\t\t\nAIOS_SHEET_END";

        let parsed = parse_read_output("/safe/a.numbers", output).unwrap();

        assert_eq!(parsed["status"], "table");
        assert_eq!(parsed["provider"], "apple-iwork");
        // The localized sheet name is reported, never used to find anything.
        assert_eq!(parsed["sheet"], "\u{5de5}\u{4f5c}\u{8868} 1");
        // Trimmed for the caller, but what Numbers actually holds is still
        // reported, so the 22x7 is visible rather than hidden.
        assert_eq!(parsed["rows"], 2);
        assert_eq!(parsed["columns"], 2);
        assert_eq!(parsed["totalRows"], 22);
        assert_eq!(parsed["totalColumns"], 7);
        assert_eq!(parsed["content"], "Region\tTotal\nNorth\t42");

        // More than one table on a sheet is reported as truncated: only the
        // first is read.
        let multi = output.replace("AIOS_TABLES=1", "AIOS_TABLES=3");
        let parsed = parse_read_output("/safe/a.numbers", &multi).unwrap();
        assert_eq!(parsed["tableCount"], 3);

        // Anything that is not the protocol is an execution failure.
        assert!(parse_read_output("/safe/a.numbers", "nonsense").is_err());
    }

    /// Numbers is sandboxed; the probe proved it can read back from the user's
    /// Documents folder. An arbitrary temp directory is not known to work, and a
    /// sandbox prompt in an unattended run blocks every later automation.
    #[cfg(target_os = "macos")]
    fn numbers_workspace() -> std::path::PathBuf {
        let root = std::path::Path::new(&std::env::var("HOME").unwrap())
            .join("Documents")
            .join(format!("ai-os-numbers-e2e-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires Apple Numbers"]
    fn numbers_spreadsheet_real_e2e() {
        let root = numbers_workspace();
        let spreadsheet = root.join("phase-i-numbers.numbers");

        let content = "Region\tQ1\tQ2\nNorth\t10\t20\nSouth\t50\t5\nEast\t30\t30";

        let created = create_numbers_spreadsheet(&json!({
            "path": spreadsheet.to_str().unwrap(),
            "content": content,
        }))
        .unwrap();

        assert_eq!(created["status"], "created");
        assert_eq!(created["rows"], 4);
        assert_eq!(created["columns"], 3);
        // Single sheet, because Numbers cannot add one through AppleScript.
        assert_eq!(created["sheets"], 1);
        assert!(spreadsheet.is_file(), "Numbers left no spreadsheet behind");

        // Read it back in its own osascript invocation, the way a caller would.
        let read = read_numbers_spreadsheet(&json!({"path": spreadsheet.to_str().unwrap()}))
            .unwrap();

        assert_eq!(read["status"], "table");
        assert_eq!(read["worksheetCount"], 1);
        assert_eq!(read["tableCount"], 1);

        // Trimmed back to the content, out of a table Numbers still holds as
        // 22 x 7 or larger.
        assert_eq!(read["rows"], 4, "read returned {read:#?}");
        assert_eq!(read["columns"], 3);
        assert!(read["totalRows"].as_u64().unwrap() >= 22);
        assert!(read["totalColumns"].as_u64().unwrap() >= 7);

        // Numbers stores 10 as a number and renders it back with a decimal, so
        // this checks the labels exactly and the numbers by prefix.
        let rendered = read["content"].as_str().unwrap();
        let rows: Vec<&str> = rendered.split('\n').collect();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0], "Region\tQ1\tQ2");
        assert!(rows[1].starts_with("North\t10"), "row was {}", rows[1]);
        assert!(rows[3].starts_with("East\t30"), "row was {}", rows[3]);

        // Creating over an existing spreadsheet is refused, not replaced.
        assert!(create_numbers_spreadsheet(&json!({
            "path": spreadsheet.to_str().unwrap(),
            "content": "replacement",
        }))
        .is_err());

        let _ = fs::remove_dir_all(&root);
    }
}
