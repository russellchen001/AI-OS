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
}
