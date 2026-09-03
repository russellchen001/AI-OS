//! Structured Office file access, with no application involved.
//!
//! `.xlsx`, `.docx` and `.pptx` are ZIP archives of XML. Reading one needs no
//! Excel, no Numbers and no WPS — which is the point. The application adapters
//! give the highest fidelity when their application is installed; this layer is
//! what makes the Office skill a capability rather than a set of per-application
//! integrations, because it works on any machine.
//!
//! It is deliberately conservative: it reports what the file says, applies the
//! same bounds the application adapters do, and refuses rather than guesses when
//! the archive is not shaped the way the format requires.

use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;

const MAX_SHEETS: usize = 16;
const MAX_ROWS: usize = 200;
const MAX_COLUMNS: usize = 64;
const MAX_CELL_CHARS: usize = 256;
/// A part far larger than any legitimate worksheet is refused rather than
/// decompressed: a small archive can expand without limit otherwise.
const MAX_PART_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct StructuredError {
    pub(crate) invalid_request: bool,
    pub(crate) message: String,
}

impl StructuredError {
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

fn require_existing_path<'a>(
    input: &'a Value,
    operation: &str,
    extension: &str,
) -> Result<&'a str, StructuredError> {
    let path = input
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| StructuredError::invalid(format!("{operation} requires path")))?;

    let target = Path::new(path);

    if !target.is_absolute() {
        return Err(StructuredError::invalid(format!(
            "{operation} requires an absolute path"
        )));
    }

    let actual = target
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    if actual != extension {
        return Err(StructuredError::invalid(format!(
            "{operation} requires a .{extension} path"
        )));
    }

    if !target.is_file() {
        return Err(StructuredError::invalid(format!(
            "{operation} path does not exist"
        )));
    }

    Ok(path)
}

fn require_new_path<'a>(
    input: &'a Value,
    operation: &str,
    extension: &str,
) -> Result<&'a str, StructuredError> {
    let path = input
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| StructuredError::invalid(format!("{operation} requires path")))?;

    let target = Path::new(path);

    if !target.is_absolute() {
        return Err(StructuredError::invalid(format!(
            "{operation} requires an absolute path"
        )));
    }

    let actual = target
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    if actual != extension {
        return Err(StructuredError::invalid(format!(
            "{operation} requires a .{extension} path"
        )));
    }

    if target.exists() {
        return Err(StructuredError::invalid(format!(
            "{operation} refuses to overwrite an existing path"
        )));
    }

    let parent = target
        .parent()
        .ok_or_else(|| StructuredError::invalid(format!("{operation} requires an absolute path")))?;

    if !parent.is_dir() {
        return Err(StructuredError::invalid(format!(
            "{operation} parent directory does not exist"
        )));
    }

    Ok(path)
}

/// `B12` -> `(12, 2)`. Returns None for a reference the format does not permit.
fn parse_cell_reference(reference: &str) -> Option<(usize, usize)> {
    let mut column = 0usize;
    let mut row = 0usize;
    let mut seen_digit = false;

    for character in reference.chars() {
        if character.is_ascii_alphabetic() {
            if seen_digit {
                return None;
            }
            column = column * 26 + (character.to_ascii_uppercase() as usize - 'A' as usize + 1);
        } else if character.is_ascii_digit() {
            seen_digit = true;
            row = row * 10 + character.to_digit(10)? as usize;
        } else {
            return None;
        }
    }

    (row > 0 && column > 0).then_some((row, column))
}

fn column_letters(mut column: usize) -> String {
    let mut letters = String::new();

    while column > 0 {
        let remainder = (column - 1) % 26;
        letters.insert(0, (b'A' + remainder as u8) as char);
        column = (column - 1) / 26;
    }

    letters
}

/// Text made safe to place inside an XML element.
///
/// Only the characters that would change the document's structure are escaped;
/// `>` is included because a literal `]]>` sequence is not permitted in content.
fn encode_entities(value: &str) -> String {
    let mut output = String::with_capacity(value.len());

    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            // Control characters other than tab, newline and carriage return
            // are not legal XML at all, and a spreadsheet cell has no use for
            // them.
            character if (character as u32) < 0x20 && !matches!(character, '\t' | '\n' | '\r') => {}
            character => output.push(character),
        }
    }

    output
}

/// XML text with the five predefined entities resolved.
///
/// A hand-rolled reader is used rather than a parser dependency: the parts this
/// layer reads are machine-written by the producing application, and the shapes
/// it needs are small and fixed.
fn decode_entities(value: &str) -> String {
    if !value.contains('&') {
        return value.to_owned();
    }

    let mut output = String::with_capacity(value.len());
    let mut rest = value;

    while let Some(index) = rest.find('&') {
        output.push_str(&rest[..index]);
        let tail = &rest[index..];

        let (decoded, consumed) = if let Some(entity) = tail.strip_prefix("&amp;") {
            let _ = entity;
            ("&".to_owned(), 5)
        } else if tail.starts_with("&lt;") {
            ("<".to_owned(), 4)
        } else if tail.starts_with("&gt;") {
            (">".to_owned(), 4)
        } else if tail.starts_with("&quot;") {
            ("\"".to_owned(), 6)
        } else if tail.starts_with("&apos;") {
            ("'".to_owned(), 6)
        } else if let Some(end) = tail.find(';').filter(|end| *end <= 12) {
            let body = &tail[1..end];
            let code = body
                .strip_prefix("#x")
                .or_else(|| body.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| body.strip_prefix('#').and_then(|dec| dec.parse().ok()));

            match code.and_then(char::from_u32) {
                Some(character) => (character.to_string(), end + 1),
                None => ("&".to_owned(), 1),
            }
        } else {
            ("&".to_owned(), 1)
        };

        output.push_str(&decoded);
        rest = &tail[consumed..];
    }

    output.push_str(rest);
    output
}

/// The text content of every occurrence of one element, in document order.
fn element_texts(xml: &str, element: &str) -> Vec<String> {
    let open_exact = format!("<{element}>");
    let open_attrs = format!("<{element} ");
    let close = format!("</{element}>");
    let self_closing = format!("<{element}/>");

    let mut found = Vec::new();
    let mut rest = xml;

    loop {
        let exact = rest.find(&open_exact);
        let with_attrs = rest.find(&open_attrs);
        let empty = rest.find(&self_closing);

        let Some(start) = [exact, with_attrs, empty].into_iter().flatten().min() else {
            break;
        };

        if Some(start) == empty && exact.is_none_or(|other| start < other) {
            found.push(String::new());
            rest = &rest[start + self_closing.len()..];
            continue;
        }

        let Some(open_end) = rest[start..].find('>').map(|offset| start + offset + 1) else {
            break;
        };

        let Some(close_at) = rest[open_end..].find(&close).map(|offset| open_end + offset) else {
            break;
        };

        found.push(decode_entities(&rest[open_end..close_at]));
        rest = &rest[close_at + close.len()..];
    }

    found
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;
    Some(decode_entities(&tag[start..end]))
}

fn read_part(
    archive: &mut zip::ZipArchive<std::fs::File>,
    name: &str,
) -> Result<Option<String>, StructuredError> {
    let mut part = match archive.by_name(name) {
        Ok(part) => part,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(_) => {
            return Err(StructuredError::execution(format!(
                "{name} could not be read from the archive"
            )))
        }
    };

    if part.size() > MAX_PART_BYTES {
        return Err(StructuredError::invalid(format!(
            "{name} is larger than this reader will decompress"
        )));
    }

    let mut contents = String::new();
    part.read_to_string(&mut contents)
        .map_err(|_| StructuredError::execution(format!("{name} is not readable text")))?;

    Ok(Some(contents))
}

/// Worksheet names in workbook order, paired with the part each one lives in.
fn worksheet_parts(
    workbook: &str,
    relationships: &str,
) -> Result<Vec<(String, String)>, StructuredError> {
    let mut targets = Vec::new();

    for tag in workbook.split('<') {
        if !tag.starts_with("sheet ") {
            continue;
        }

        let name = attribute(tag, "name").ok_or_else(|| {
            StructuredError::execution("the workbook declares a worksheet with no name")
        })?;
        let relationship = attribute(tag, "r:id")
            .or_else(|| attribute(tag, "id"))
            .ok_or_else(|| {
                StructuredError::execution("the workbook declares a worksheet with no relationship")
            })?;

        targets.push((name, relationship));
    }

    let mut resolved = Vec::new();

    for (name, relationship) in targets {
        let part = relationships
            .split('<')
            .filter(|tag| tag.starts_with("Relationship "))
            .find(|tag| attribute(tag, "Id").as_deref() == Some(relationship.as_str()))
            .and_then(|tag| attribute(tag, "Target"))
            .ok_or_else(|| {
                StructuredError::execution(format!(
                    "the workbook references worksheet {name} through a relationship it does not define"
                ))
            })?;

        let part = part.trim_start_matches('/').trim_start_matches("xl/");
        resolved.push((name, format!("xl/{part}")));
    }

    Ok(resolved)
}

fn shared_strings(xml: &str) -> Vec<String> {
    // Each <si> is one string, and it may be split across several <t> runs.
    let mut strings = Vec::new();
    let mut rest = xml;

    while let Some(start) = rest.find("<si>").or_else(|| rest.find("<si ")) {
        let Some(open_end) = rest[start..].find('>').map(|offset| start + offset + 1) else {
            break;
        };
        let Some(close_at) = rest[open_end..].find("</si>").map(|offset| open_end + offset) else {
            break;
        };

        strings.push(element_texts(&rest[open_end..close_at], "t").join(""));
        rest = &rest[close_at + 5..];
    }

    strings
}

fn render_cell(tag: &str, body: &str, shared: &[String]) -> String {
    let kind = attribute(tag, "t").unwrap_or_default();

    let rendered = match kind.as_str() {
        "s" => element_texts(body, "v")
            .first()
            .and_then(|index| index.trim().parse::<usize>().ok())
            .and_then(|index| shared.get(index).cloned())
            .unwrap_or_default(),
        "inlineStr" => element_texts(body, "t").join(""),
        "b" => match element_texts(body, "v").first().map(String::as_str) {
            Some("1") => "true".to_owned(),
            Some("0") => "false".to_owned(),
            _ => String::new(),
        },
        // "str" is a formula's cached string result; everything else is a
        // number, which is reported exactly as the file stores it.
        _ => element_texts(body, "v").first().cloned().unwrap_or_default(),
    };

    let cleaned = rendered.replace(['\t', '\r', '\n'], " ");

    if cleaned.chars().count() > MAX_CELL_CHARS {
        return cleaned.chars().take(MAX_CELL_CHARS).collect();
    }

    cleaned
}

struct SheetContent {
    rows: usize,
    columns: usize,
    total_rows: usize,
    total_columns: usize,
    truncated: bool,
    content: String,
}

fn read_worksheet(xml: &str, shared: &[String]) -> SheetContent {
    let mut grid: Vec<Vec<String>> = Vec::new();
    let mut total_rows = 0usize;
    let mut total_columns = 0usize;
    let mut truncated = false;

    let mut rest = xml;

    while let Some(start) = rest.find("<c ") {
        let Some(open_end) = rest[start..].find('>').map(|offset| start + offset + 1) else {
            break;
        };

        let tag = &rest[start + 1..open_end - 1];
        let self_closed = tag.ends_with('/');
        let tag = tag.trim_end_matches('/');

        let (body, next) = if self_closed {
            ("", open_end)
        } else {
            match rest[open_end..].find("</c>").map(|offset| open_end + offset) {
                Some(close_at) => (&rest[open_end..close_at], close_at + 4),
                None => break,
            }
        };

        if let Some((row, column)) = attribute(tag, "r").as_deref().and_then(parse_cell_reference) {
            total_rows = total_rows.max(row);
            total_columns = total_columns.max(column);

            if row > MAX_ROWS || column > MAX_COLUMNS {
                truncated = true;
            } else {
                if grid.len() < row {
                    grid.resize(row, Vec::new());
                }
                let line = &mut grid[row - 1];
                if line.len() < column {
                    line.resize(column, String::new());
                }
                line[column - 1] = render_cell(tag, body, shared);
            }
        }

        rest = &rest[next..];
    }

    let columns = grid.iter().map(Vec::len).max().unwrap_or(0);
    for line in &mut grid {
        line.resize(columns, String::new());
    }

    let content = grid
        .iter()
        .map(|line| line.join("\t"))
        .collect::<Vec<_>>()
        .join("\n");

    SheetContent {
        rows: grid.len(),
        columns,
        total_rows,
        total_columns,
        truncated,
        content,
    }
}

pub(crate) fn read_structured_spreadsheet(input: &Value) -> Result<Value, StructuredError> {
    let path = require_existing_path(input, "spreadsheet.read", "xlsx")?;

    let file = std::fs::File::open(path)
        .map_err(|_| StructuredError::execution("the workbook could not be opened"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|_| StructuredError::invalid("the file is not a readable XLSX archive"))?;

    let workbook = read_part(&mut archive, "xl/workbook.xml")?
        .ok_or_else(|| StructuredError::invalid("the archive has no workbook part"))?;
    let relationships = read_part(&mut archive, "xl/_rels/workbook.xml.rels")?
        .ok_or_else(|| StructuredError::invalid("the archive has no workbook relationships"))?;

    let shared = read_part(&mut archive, "xl/sharedStrings.xml")?
        .map(|xml| shared_strings(&xml))
        .unwrap_or_default();

    let parts = worksheet_parts(&workbook, &relationships)?;

    if parts.is_empty() {
        return Err(StructuredError::invalid("the workbook has no worksheets"));
    }

    let worksheet_names: Vec<String> = parts.iter().map(|(name, _)| name.clone()).collect();
    let emitted = parts.len().min(MAX_SHEETS);
    let mut selection_truncated = parts.len() > emitted;
    let mut sheets = Vec::new();

    for (name, part) in parts.into_iter().take(emitted) {
        let xml = read_part(&mut archive, &part)?.ok_or_else(|| {
            StructuredError::execution(format!("worksheet {name} is missing from the archive"))
        })?;

        let sheet = read_worksheet(&xml, &shared);
        selection_truncated = selection_truncated || sheet.truncated;

        sheets.push(json!({
            "name": name,
            "rows": sheet.rows,
            "columns": sheet.columns,
            "totalRows": sheet.total_rows,
            "totalColumns": sheet.total_columns,
            "truncated": sheet.truncated,
            "content": sheet.content,
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
            "worksheetCount": worksheet_names.len(),
            "worksheetNames": worksheet_names,
            "truncated": selection_truncated,
            "content": sheet["content"],
            "provider": "local-structured",
        }));
    }

    Ok(json!({
        "path": path,
        "status": "workbook",
        "worksheetCount": worksheet_names.len(),
        "worksheetNames": worksheet_names,
        "sheets": sheets,
        "truncated": selection_truncated,
        "provider": "local-structured",
    }))
}

/// The parts of a minimal but conformant XLSX package.
///
/// Excel is strict about the package even when it is lenient about the sheet:
/// omit the content types or the workbook relationships and it refuses the file
/// outright rather than opening it partially. `styles.xml` is included for the
/// same reason -- a workbook without one is rejected by some readers even
/// though nothing in it is styled.
fn spreadsheet_parts(rows: &[Vec<String>]) -> Vec<(&'static str, String)> {
    let mut shared: Vec<String> = Vec::new();
    let mut sheet = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#,
    );

    for (row_index, row) in rows.iter().enumerate() {
        let row_number = row_index + 1;
        sheet.push_str(&format!("<row r=\"{row_number}\">"));

        for (column_index, cell) in row.iter().enumerate() {
            if cell.is_empty() {
                continue;
            }

            let reference = format!("{}{row_number}", column_letters(column_index + 1));

            // A value that is a number is written as one, so the file says the
            // same thing a spreadsheet application would have written. Anything
            // else is a shared string, which is how Excel stores text.
            match cell.parse::<f64>() {
                Ok(number) if number.is_finite() && !cell.trim().is_empty() => {
                    sheet.push_str(&format!("<c r=\"{reference}\"><v>{cell}</v></c>"));
                }
                _ => {
                    let index = match shared.iter().position(|existing| existing == cell) {
                        Some(index) => index,
                        None => {
                            shared.push(cell.clone());
                            shared.len() - 1
                        }
                    };
                    sheet.push_str(&format!(
                        "<c r=\"{reference}\" t=\"s\"><v>{index}</v></c>"
                    ));
                }
            }
        }

        sheet.push_str("</row>");
    }

    sheet.push_str("</sheetData></worksheet>");

    let mut strings = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="{}" uniqueCount="{}">"#,
        shared.len(),
        shared.len()
    );

    for value in &shared {
        strings.push_str(&format!(
            "<si><t xml:space=\"preserve\">{}</t></si>",
            encode_entities(value)
        ));
    }

    strings.push_str("</sst>");

    vec![
        (
            "[Content_Types].xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>"#
                .to_owned(),
        ),
        (
            "_rels/.rels",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
                .to_owned(),
        ),
        (
            "xl/workbook.xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#
                .to_owned(),
        ),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#
                .to_owned(),
        ),
        (
            "xl/styles.xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs></styleSheet>"#
                .to_owned(),
        ),
        ("xl/sharedStrings.xml", strings),
        ("xl/worksheets/sheet1.xml", sheet),
    ]
}

pub(crate) fn create_structured_spreadsheet(input: &Value) -> Result<Value, StructuredError> {
    let path = require_new_path(input, "spreadsheet.create", "xlsx")?;

    let content = input
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| StructuredError::invalid("spreadsheet.create requires content"))?;

    if content.trim().is_empty() {
        return Err(StructuredError::invalid("spreadsheet.create requires content"));
    }

    let rows: Vec<Vec<String>> = content
        .trim_end_matches('\n')
        .split('\n')
        .map(|line| {
            line.trim_end_matches('\r')
                .split('\t')
                .map(str::to_owned)
                .collect()
        })
        .collect();

    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);

    if rows.len() > MAX_ROWS || columns > MAX_COLUMNS {
        return Err(StructuredError::invalid(format!(
            "spreadsheet.create supports at most {MAX_ROWS} rows and {MAX_COLUMNS} columns"
        )));
    }

    if rows.iter().flatten().any(|cell| cell.chars().count() > MAX_CELL_CHARS) {
        return Err(StructuredError::invalid(format!(
            "spreadsheet.create supports at most {MAX_CELL_CHARS} characters per cell"
        )));
    }

    // Built beside the destination and renamed into place, so a failure part way
    // through cannot leave a half-written workbook at the path the caller asked
    // for.
    let staged = Path::new(path).with_extension(format!("partial-{}.xlsx", std::process::id()));

    let outcome = (|| -> Result<(), StructuredError> {
        let file = std::fs::File::create(&staged)
            .map_err(|_| StructuredError::execution("the workbook could not be created"))?;
        let mut archive = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        for (name, body) in spreadsheet_parts(&rows) {
            archive
                .start_file(name, options)
                .map_err(|_| StructuredError::execution(format!("{name} could not be written")))?;
            std::io::Write::write_all(&mut archive, body.as_bytes())
                .map_err(|_| StructuredError::execution(format!("{name} could not be written")))?;
        }

        archive
            .finish()
            .map_err(|_| StructuredError::execution("the workbook could not be finished"))?;

        Ok(())
    })();

    if let Err(error) = outcome {
        let _ = std::fs::remove_file(&staged);
        return Err(error);
    }

    std::fs::rename(&staged, path).map_err(|_| {
        let _ = std::fs::remove_file(&staged);
        StructuredError::execution("the finished workbook could not be moved into place")
    })?;

    Ok(json!({
        "path": path,
        "status": "created",
        "rows": rows.len(),
        "columns": columns,
        "sheets": 1,
        "provider": "local-structured",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_references_are_parsed_and_malformed_ones_refused() {
        assert_eq!(parse_cell_reference("A1"), Some((1, 1)));
        assert_eq!(parse_cell_reference("B12"), Some((12, 2)));
        assert_eq!(parse_cell_reference("Z1"), Some((1, 26)));
        assert_eq!(parse_cell_reference("AA1"), Some((1, 27)));
        assert_eq!(parse_cell_reference("BX999"), Some((999, 76)));

        for rejected in ["", "1A", "A", "1", "A1B", "A-1", "$A$1", "A0"] {
            assert_eq!(parse_cell_reference(rejected), None, "{rejected}");
        }
    }

    #[test]
    fn xml_entities_are_decoded_including_numeric_ones() {
        assert_eq!(decode_entities("plain"), "plain");
        assert_eq!(decode_entities("a &amp; b"), "a & b");
        assert_eq!(decode_entities("&lt;tag&gt;"), "<tag>");
        assert_eq!(decode_entities("&quot;q&quot; &apos;a&apos;"), "\"q\" 'a'");
        assert_eq!(decode_entities("&#65;&#x42;"), "AB");
        // An ampersand that begins nothing is kept, not swallowed.
        assert_eq!(decode_entities("Q&A"), "Q&A");
    }

    #[test]
    fn shared_strings_join_their_runs() {
        // Rich text splits one string across several runs; the cell holds the
        // whole thing, not the first fragment.
        let xml = r#"<sst count="3"><si><t>Region</t></si>
<si><r><t>Total</t></r><r><t> (USD)</t></r></si>
<si><t xml:space="preserve">  padded </t></si></sst>"#;

        assert_eq!(
            shared_strings(xml),
            vec!["Region", "Total (USD)", "  padded "]
        );
    }

    #[test]
    fn worksheet_cells_are_placed_by_reference_not_by_order() {
        let shared = vec!["Region".to_owned(), "North".to_owned()];

        // Deliberately sparse and out of order: a real sheet omits empty cells,
        // and a reader that trusts document order puts the values in the wrong
        // places.
        let xml = r#"<worksheet><sheetData>
<row r="2"><c r="C2" t="s"><v>1</v></c></row>
<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>42</v></c></row>
<row r="3"><c r="A3" t="b"><v>1</v></c><c r="B3" t="inlineStr"><is><t>inline</t></is></c></row>
</sheetData></worksheet>"#;

        let sheet = read_worksheet(xml, &shared);

        assert_eq!(sheet.rows, 3);
        assert_eq!(sheet.columns, 3);
        assert_eq!(sheet.total_rows, 3);
        assert_eq!(sheet.total_columns, 3);
        assert!(!sheet.truncated);
        assert_eq!(sheet.content, "Region\t42\t\n\t\tNorth\ntrue\tinline\t");
    }

    #[test]
    fn a_cell_beyond_the_bounds_truncates_rather_than_grows_the_grid() {
        let xml = format!(
            r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="A{}"><v>2</v></c></row></sheetData></worksheet>"#,
            MAX_ROWS + 1
        );

        let sheet = read_worksheet(&xml, &[]);

        assert!(sheet.truncated);
        assert_eq!(sheet.rows, 1);
        // What the file actually holds is still reported, so the truncation is
        // visible rather than hidden.
        assert_eq!(sheet.total_rows, MAX_ROWS + 1);
    }

    #[test]
    fn worksheet_parts_resolve_through_relationships_and_refuse_a_dangling_one() {
        let workbook = r#"<workbook><sheets>
<sheet name="Sales" sheetId="1" r:id="rId1"/>
<sheet name="Summary" sheetId="2" r:id="rId2"/>
</sheets></workbook>"#;

        let relationships = r#"<Relationships>
<Relationship Id="rId2" Target="worksheets/sheet2.xml"/>
<Relationship Id="rId1" Target="/xl/worksheets/sheet1.xml"/>
</Relationships>"#;

        // Workbook order wins, not relationship order, and both an absolute and
        // a relative target resolve to the same place.
        assert_eq!(
            worksheet_parts(workbook, relationships).unwrap(),
            vec![
                ("Sales".to_owned(), "xl/worksheets/sheet1.xml".to_owned()),
                ("Summary".to_owned(), "xl/worksheets/sheet2.xml".to_owned()),
            ]
        );

        // A worksheet whose relationship is not defined is refused, rather than
        // silently dropped from the workbook.
        let dangling = r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#;
        assert!(worksheet_parts(workbook, dangling).is_err());
    }

    #[test]
    fn paths_fail_closed_on_shape_and_existence() {
        let root = tempfile::tempdir().unwrap();
        let present = root.path().join("book.xlsx");
        std::fs::write(&present, b"not really a zip").unwrap();

        assert!(require_existing_path(
            &json!({"path": present.to_str().unwrap()}),
            "spreadsheet.read",
            "xlsx"
        )
        .is_ok());

        for rejected in [
            json!({}),
            json!({"path": "  "}),
            json!({"path": "relative.xlsx"}),
            json!({"path": root.path().join("book.numbers").to_str().unwrap()}),
            json!({"path": root.path().join("missing.xlsx").to_str().unwrap()}),
        ] {
            assert!(
                require_existing_path(&rejected, "spreadsheet.read", "xlsx").is_err(),
                "{rejected} should be rejected"
            );
        }

        // A file with the right name that is not an archive is refused as a
        // request problem, not reported as an empty workbook.
        let error = read_structured_spreadsheet(&json!({"path": present.to_str().unwrap()}))
            .unwrap_err();
        assert!(error.invalid_request);
    }

    #[test]
    fn column_letters_are_the_inverse_of_cell_references() {
        for (column, letters) in [(1, "A"), (26, "Z"), (27, "AA"), (52, "AZ"), (53, "BA")] {
            assert_eq!(column_letters(column), letters);
            assert_eq!(
                parse_cell_reference(&format!("{letters}1")),
                Some((1, column))
            );
        }
    }

    #[test]
    fn written_text_survives_being_read_back_by_this_same_layer() {
        let root = tempfile::tempdir().unwrap();
        let workbook = root.path().join("written.xlsx");

        // Every character that would otherwise break the XML, plus a value that
        // looks like a number and one that only nearly does.
        let content = "Region\tTotal\nA & B\t42\n<tag>\t3.5\n\"quoted\"\t007-not-a-number\n\tonly-second";

        let created = create_structured_spreadsheet(&json!({
            "path": workbook.to_str().unwrap(),
            "content": content,
        }))
        .unwrap();

        assert_eq!(created["status"], "created");
        assert_eq!(created["rows"], 5);
        assert_eq!(created["columns"], 2);
        assert!(workbook.is_file());

        let read = read_structured_spreadsheet(&json!({"path": workbook.to_str().unwrap()}))
            .unwrap();

        assert_eq!(read["status"], "table");
        assert_eq!(read["sheet"], "Sheet1");

        let rows: Vec<&str> = read["content"].as_str().unwrap().split('\n').collect();
        assert_eq!(rows[0], "Region\tTotal");
        assert_eq!(rows[1], "A & B\t42");
        assert_eq!(rows[2], "<tag>\t3.5");
        // A string of digits with a leading zero is text, not the number 7.
        assert_eq!(rows[3], "\"quoted\"\t007-not-a-number");
        // An empty leading cell stays empty rather than shifting the row left.
        assert_eq!(rows[4], "\tonly-second");
    }

    #[test]
    fn writing_fails_closed_on_overwrite_bounds_and_empty_content() {
        let root = tempfile::tempdir().unwrap();
        let existing = root.path().join("taken.xlsx");
        std::fs::write(&existing, b"already here").unwrap();

        let fresh = root.path().join("fresh.xlsx");
        let fresh = fresh.to_str().unwrap();

        // An existing file is never replaced.
        assert!(create_structured_spreadsheet(
            &json!({"path": existing.to_str().unwrap(), "content": "a"})
        )
        .is_err());
        assert_eq!(std::fs::read(&existing).unwrap(), b"already here");

        for rejected in [
            json!({"path": fresh}),
            json!({"path": fresh, "content": ""}),
            json!({"path": fresh, "content": "   "}),
            json!({"path": "relative.xlsx", "content": "a"}),
            json!({"path": root.path().join("wrong.numbers").to_str().unwrap(), "content": "a"}),
            json!({"path": root.path().join("no").join("dir.xlsx").to_str().unwrap(), "content": "a"}),
        ] {
            assert!(
                create_structured_spreadsheet(&rejected).is_err(),
                "{rejected} should be rejected"
            );
        }

        let too_many_rows = vec!["a"; MAX_ROWS + 1].join("\n");
        assert!(
            create_structured_spreadsheet(&json!({"path": fresh, "content": too_many_rows}))
                .is_err()
        );

        let too_wide = vec!["a"; MAX_COLUMNS + 1].join("\t");
        assert!(
            create_structured_spreadsheet(&json!({"path": fresh, "content": too_wide})).is_err()
        );

        let too_long = "x".repeat(MAX_CELL_CHARS + 1);
        assert!(
            create_structured_spreadsheet(&json!({"path": fresh, "content": too_long})).is_err()
        );

        // Nothing was left behind by any of the refusals, including the
        // staged part-file.
        let leftovers: Vec<_> = std::fs::read_dir(root.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name != "taken.xlsx")
            .collect();
        assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
    }

    /// The whole point of this layer: the same workbook, read by Excel and read
    /// without Excel, has to say the same thing.
    ///
    /// Anything less and "the Office skill works without the application" is a
    /// claim rather than a fact.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires Microsoft Excel and AI_OS_EXCEL_EDIT_FIXTURE"]
    fn structured_and_excel_agree_on_the_same_workbook() {
        let fixture = std::env::var("AI_OS_EXCEL_EDIT_FIXTURE").expect("preserved fixture path");

        let root = tempfile::tempdir().unwrap();
        let workbook = root.path().join("agreement.xlsx");

        // Built by Excel, so what is being compared is a real Excel file rather
        // than one this test invented.
        crate::document::excel::edit_excel_workbook(&json!({
            "source": fixture,
            "destination": workbook,
            "operations": [
                {"type":"add_worksheet","name":"Agree"},
                {"type":"set_cell","sheet":"Agree","row":1,"column":1,"value":"Region"},
                {"type":"set_cell","sheet":"Agree","row":1,"column":2,"value":"Total"},
                {"type":"set_cell","sheet":"Agree","row":2,"column":1,"value":"North"},
                {"type":"set_cell","sheet":"Agree","row":2,"column":2,"value":42},
                {"type":"set_cell","sheet":"Agree","row":3,"column":1,"value":"Ampersand & <tag>"},
                {"type":"set_cell","sheet":"Agree","row":3,"column":2,"value":true}
            ]
        }))
        .unwrap();

        let structured =
            read_structured_spreadsheet(&json!({"path": workbook.to_str().unwrap()})).unwrap();

        assert_eq!(structured["provider"], "local-structured");
        assert_eq!(structured["status"], "workbook");

        let sheets = structured["sheets"].as_array().unwrap();
        let agree = sheets
            .iter()
            .find(|sheet| sheet["name"] == "Agree")
            .unwrap_or_else(|| panic!("no Agree sheet in {structured:#?}"));

        let rows: Vec<&str> = agree["content"].as_str().unwrap().split('\n').collect();
        assert_eq!(rows.len(), 3, "content was {:?}", agree["content"]);
        assert_eq!(rows[0], "Region\tTotal");
        assert_eq!(rows[1], "North\t42");

        // Escaping survives the round trip through the file's XML.
        assert!(
            rows[2].starts_with("Ampersand & <tag>\t"),
            "row was {}",
            rows[2]
        );
        // Excel writes a boolean as a typed cell, not as the word.
        assert!(rows[2].ends_with("true"), "row was {}", rows[2]);

        // Worksheet identity agrees with what Excel reports, including the
        // sheet the fixture already had.
        let names: Vec<&str> = structured["worksheetNames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| name.as_str().unwrap())
            .collect();
        assert!(names.contains(&"Agree"), "names were {names:?}");
    }

    /// A directory Excel can always read from.
    ///
    /// This file is written with no application at all, so Excel has no
    /// implicit access to it. Opening it from a temp directory makes Excel
    /// raise a "please locate this file" grant prompt that no automated run
    /// can answer, and that prompt then blocks every later Excel automation
    /// until a person dismisses it. Excel's own container is inside its
    /// sandbox, so a workbook placed there needs no grant.
    #[cfg(target_os = "macos")]
    fn excel_readable_workspace(label: &str) -> std::path::PathBuf {
        let root = std::path::Path::new(&std::env::var("HOME").unwrap())
            .join("Library/Containers/com.microsoft.Excel/Data/Library/Caches/com.microsoft.Excel")
            .join(format!("ai-os-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// The other direction, and the one that decides whether this layer can be
    /// trusted to WRITE: a workbook produced with no application at all has to
    /// open in Excel and read back as what was written.
    ///
    /// A file that only this reader can read would be a private format wearing
    /// an .xlsx extension.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires Microsoft Excel"]
    fn excel_can_open_what_the_structured_layer_wrote() {
        let root = excel_readable_workspace("structured-write");
        let workbook = root.join("written-without-excel.xlsx");
        let workbook_path = workbook.to_str().unwrap();

        create_structured_spreadsheet(&json!({
            "path": workbook_path,
            "content": "Region\tTotal\nA & B\t42\n<tag>\t3.5",
        }))
        .unwrap();

        // Read it with Excel through the existing provider-neutral read path,
        // which is a separate osascript invocation and a separate reader.
        let output = std::process::Command::new("/bin/bash")
            .arg("-lc")
            .arg(crate::runtime::openclaw_gateway_adapter::spreadsheet_read_command_for_test(
                workbook_path,
            ))
            .output()
            .unwrap();

        assert!(output.status.success());
        let text = String::from_utf8_lossy(&output.stdout).to_string();

        assert!(
            !text.contains("AIOS_FAILED"),
            "Excel refused the workbook this layer wrote:\n{text}"
        );

        // Excel renders numbers with a decimal, so the labels are matched
        // exactly and the numbers by prefix.
        assert!(text.contains("Region\tTotal"), "Excel read:\n{text}");
        assert!(text.contains("A & B\t42"), "Excel read:\n{text}");
        assert!(text.contains("<tag>\t3.5"), "Excel read:\n{text}");

        // The workspace lives inside Excel's container, so it is removed here
        // rather than by a TempDir guard.
        let _ = std::fs::remove_dir_all(&root);
    }
}
