use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_SLIDES: usize = 50;
const MAX_TEXT: usize = 16 * 1024;
const MAX_TABLE_DIMENSION: usize = 10;
const MAX_CELL_TEXT: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PowerPointError {
    pub invalid_request: bool,
    pub message: String,
}

impl PowerPointError {
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

fn require_path<'a>(
    input: &'a Value,
    field: &str,
    must_exist: bool,
) -> Result<&'a str, PowerPointError> {
    let value = input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            PowerPointError::invalid(format!("PowerPoint operation requires {field}"))
        })?;
    let path = Path::new(value);
    if !path.is_absolute() {
        return Err(PowerPointError::invalid(format!(
            "{field} must be an absolute path"
        )));
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "ppt" | "pptx" | "pdf") {
        return Err(PowerPointError::invalid(format!(
            "unsupported PowerPoint file format: {extension}"
        )));
    }
    if must_exist && !path.is_file() {
        return Err(PowerPointError::invalid(format!("{field} does not exist")));
    }
    if !must_exist && path.exists() {
        return Err(PowerPointError::invalid(format!(
            "{field} already exists; overwrite is not permitted"
        )));
    }
    Ok(value)
}

fn require_image(input: &Value) -> Result<&str, PowerPointError> {
    let value = input
        .get("imagePath")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| PowerPointError::invalid("presentation operation requires imagePath"))?;
    let path = Path::new(value);
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !path.is_absolute()
        || !path.is_file()
        || !matches!(
            extension.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "tif" | "tiff"
        )
    {
        return Err(PowerPointError::invalid(
            "imagePath must be an existing absolute supported local image",
        ));
    }
    Ok(value)
}

fn validate_table(input: &Value) -> Result<(), PowerPointError> {
    let table = input
        .get("table")
        .and_then(Value::as_object)
        .ok_or_else(|| PowerPointError::invalid("presentation operation requires a table"))?;
    let rows = table.get("rows").and_then(Value::as_u64).unwrap_or(0) as usize;
    let columns = table.get("columns").and_then(Value::as_u64).unwrap_or(0) as usize;
    if rows == 0
        || columns == 0
        || rows > MAX_TABLE_DIMENSION
        || columns > MAX_TABLE_DIMENSION
        || rows != 2
        || columns != 2
    {
        return Err(PowerPointError::invalid(
            "PowerPoint v1 table must be bounded to the deterministic 2x2 operation",
        ));
    }
    Ok(())
}

fn run_osascript(script: &str, args: &[&str]) -> Result<String, PowerPointError> {
    let mut child = Command::new("/usr/bin/osascript")
        .arg("-")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| PowerPointError::execution("Unable to start PowerPoint automation"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| PowerPointError::execution("Unable to open AppleScript input"))?
        .write_all(script.as_bytes())
        .map_err(|_| PowerPointError::execution("Unable to write PowerPoint AppleScript"))?;
    let output = child
        .wait_with_output()
        .map_err(|_| PowerPointError::execution("PowerPoint AppleScript did not complete"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(PowerPointError::execution(if stderr.is_empty() {
            "PowerPoint automation failed".to_owned()
        } else {
            format!("PowerPoint automation failed: {stderr}")
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn cache_path(extension: &str) -> Result<(PathBuf, PathBuf), PowerPointError> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| PowerPointError::execution("PowerPoint cache home is unavailable"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| PowerPointError::execution("System clock is unavailable"))?
        .as_nanos();
    let root = Path::new(&home)
        .join("Library/Containers/com.microsoft.Powerpoint/Data/Library/Caches")
        .join(format!("ai-os-powerpoint-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&root)
        .map_err(|_| PowerPointError::execution("Unable to create PowerPoint workspace"))?;
    Ok((root.join(format!("output.{extension}")), root))
}

fn publish(cache: &Path, destination: &Path) -> Result<(), PowerPointError> {
    if destination.exists() {
        return Err(PowerPointError::invalid(
            "destination already exists; overwrite is not permitted",
        ));
    }
    fs::rename(cache, destination)
        .map_err(|_| PowerPointError::execution("Unable to publish PowerPoint output"))
}

const READ_SCRIPT: &str = r#"
on cleanText(valueText)
    set valueText to valueText as text
    set AppleScript's text item delimiters to ASCII character 29
    set valueText to text items of valueText as text
    set AppleScript's text item delimiters to ASCII character 30
    set valueText to text items of valueText as text
    set AppleScript's text item delimiters to ""
    return valueText
end cleanText

on run argv
    set stagedPath to item 1 of argv
    set maxSlides to (item 2 of argv) as integer
    set beforeCount to 0
    set ownsPresentation to false
    tell application id "com.microsoft.Powerpoint"
        try
            set beforeCount to count of presentations
            open (POSIX file stagedPath)
            repeat 100 times
                if (count of presentations) > beforeCount then exit repeat
                delay 0.1
            end repeat
            if (count of presentations) is not (beforeCount + 1) then error "PowerPoint read count did not increase deterministically"
            if (full name of active presentation as text) is not stagedPath then error "PowerPoint read identity mismatch"
            set ownsPresentation to true
            set slideCount to count of slides of active presentation
            if slideCount > maxSlides then error "PowerPoint slide count exceeds extraction bound"
            set slideRecords to {}
            repeat with slideIndex from 1 to slideCount
                set currentSlide to slide slideIndex of active presentation
                set titleText to ""
                set bodyText to ""
                set allText to ""
                set pictureCount to 0
                set tableCount to 0
                set basicShapeCount to 0
                repeat with shapeIndex from 1 to (count of shapes of currentSlide)
                    set currentShape to shape shapeIndex of currentSlide
                    try
                        if shape type of currentShape is shape type picture then set pictureCount to pictureCount + 1
                        if has table of currentShape then set tableCount to tableCount + 1
                        if shape type of currentShape is shape type auto then set basicShapeCount to basicShapeCount + 1
                        if has text frame of currentShape and has text of text frame of currentShape then
                            set shapeText to my cleanText(content of text range of text frame of currentShape)
                            if titleText is "" then
                                set titleText to shapeText
                            else if bodyText is "" then
                                set bodyText to shapeText
                            end if
                            set allText to allText & shapeText & linefeed
                        end if
                    end try
                end repeat
                set tableCells to ""
                if tableCount > 0 then
                    repeat with shapeIndex from 1 to (count of shapes of currentSlide)
                        set currentShape to shape shapeIndex of currentSlide
                        try
                            if has table of currentShape then
                                set currentTable to table object of currentShape
                                repeat with rowIndex from 1 to (count of rows of currentTable)
                                    repeat with columnIndex from 1 to (count of columns of currentTable)
                                        set tableCells to tableCells & my cleanText(content of text range of text frame of shape of cell columnIndex of row rowIndex of currentTable) & ASCII character 30
                                    end repeat
                                end repeat
                                exit repeat
                            end if
                        end try
                    end repeat
                end if
                set recordText to (slideIndex as text) & ASCII character 30 & titleText & ASCII character 30 & bodyText & ASCII character 30 & allText & ASCII character 30 & (pictureCount as text) & ASCII character 30 & (tableCount as text) & ASCII character 30 & (basicShapeCount as text) & ASCII character 30 & tableCells
                copy recordText to end of slideRecords
            end repeat
            set AppleScript's text item delimiters to ASCII character 29
            set payload to slideRecords as text
            set AppleScript's text item delimiters to ""
            close active presentation saving no
            set ownsPresentation to false
            repeat 100 times
                if (count of presentations) is beforeCount then exit repeat
                delay 0.1
            end repeat
            if (count of presentations) is not beforeCount then error "PowerPoint read count was not restored"
            return "AIOS_SLIDE_COUNT=" & slideCount & linefeed & "AIOS_SLIDES=" & payload
        on error m number n
            if ownsPresentation then
                try
                    close active presentation saving no
                end try
            end if
            error m number n
        end try
    end tell
end run
"#;

const CREATE_SCRIPT: &str = r#"
on run argv
    set outputPath to item 1 of argv
    set imagePath to item 2 of argv
    set titles to {item 3 of argv, item 5 of argv, item 7 of argv}
    set bodies to {item 4 of argv, item 6 of argv, item 8 of argv}
    set beforeCount to 0
    set ownsPresentation to false
    tell application id "com.microsoft.Powerpoint"
        try
            set beforeCount to count of presentations
            make new presentation
            set ownsPresentation to true
            repeat with slideIndex from 1 to 3
                set currentSlide to make new slide at end of active presentation with properties {layout:slide layout title slide}
                set content of text range of text frame of shape 1 of currentSlide to item slideIndex of titles
                set content of text range of text frame of shape 2 of currentSlide to item slideIndex of bodies
            end repeat
            set objectSlide to slide 3 of active presentation
            set textBoxShape to make new text box at objectSlide with properties {left position:40, top:260, width:220, height:45}
            set content of text range of text frame of textBoxShape to "AI-OS text box"
            make new picture at objectSlide with properties {file name:imagePath, link to file:false, save with document:true, left position:420, top:100, width:100, height:100}
            make new shape at objectSlide with properties {auto shape type:autoshape rectangle, left position:420, top:230, width:120, height:70}
            set tableShape to make new shape table at objectSlide with properties {number of rows:2, number of columns:2, left position:40, top:330, width:320, height:120}
            set tableObject to table object of tableShape
            set content of text range of text frame of shape of cell 1 of row 1 of tableObject to "A1"
            set content of text range of text frame of shape of cell 2 of row 1 of tableObject to "B1"
            set content of text range of text frame of shape of cell 1 of row 2 of tableObject to "A2"
            set content of text range of text frame of shape of cell 2 of row 2 of tableObject to "B2"
            save active presentation in (POSIX file outputPath) as save as Open XML presentation
            close active presentation saving no
            set ownsPresentation to false
            repeat 100 times
                if (count of presentations) is beforeCount then exit repeat
                delay 0.1
            end repeat
            if (count of presentations) is not beforeCount then error "PowerPoint create count was not restored"
            return "AIOS_POWERPOINT_CREATED"
        on error m number n
            if ownsPresentation then
                try
                    close active presentation saving no
                end try
            end if
            error m number n
        end try
    end tell
end run
"#;

const EDIT_SCRIPT: &str = r#"
on run argv
    set stagedPath to item 1 of argv
    set outputPath to item 2 of argv
    set imagePath to item 3 of argv
    set newTitle to item 4 of argv
    set newBody to item 5 of argv
    set beforeCount to 0
    set ownsPresentation to false
    tell application id "com.microsoft.Powerpoint"
        try
            set beforeCount to count of presentations
            open (POSIX file stagedPath)
            repeat 100 times
                if (count of presentations) > beforeCount then exit repeat
                delay 0.1
            end repeat
            if (count of presentations) is not (beforeCount + 1) then error "PowerPoint edit count did not increase deterministically"
            if (full name of active presentation as text) is not stagedPath then error "PowerPoint edit identity mismatch"
            set ownsPresentation to true
            if (count of slides of active presentation) < 3 then error "PowerPoint edit requires at least three slides"
            set content of text range of text frame of shape 1 of slide 1 of active presentation to newTitle
            set content of text range of text frame of shape 2 of slide 1 of active presentation to newBody
            set temporarySlide to make new slide at end of active presentation with properties {layout:slide layout blank}
            set countWithTemporary to count of slides of active presentation
            delete temporarySlide
            if (count of slides of active presentation) is not (countWithTemporary - 1) then error "PowerPoint temporary slide delete failed"
            set objectSlide to make new slide at end of active presentation with properties {layout:slide layout title slide}
            set content of text range of text frame of shape 1 of objectSlide to "Added Slide"
            set content of text range of text frame of shape 2 of objectSlide to "Added Body"
            set textBoxShape to make new text box at objectSlide with properties {left position:40, top:260, width:220, height:45}
            set content of text range of text frame of textBoxShape to "AI-OS edited text box"
            make new picture at objectSlide with properties {file name:imagePath, link to file:false, save with document:true, left position:420, top:100, width:100, height:100}
            make new shape at objectSlide with properties {auto shape type:autoshape rectangle, left position:420, top:230, width:120, height:70}
            set tableShape to make new shape table at objectSlide with properties {number of rows:2, number of columns:2, left position:40, top:330, width:320, height:120}
            set tableObject to table object of tableShape
            set content of text range of text frame of shape of cell 1 of row 1 of tableObject to "A1"
            set content of text range of text frame of shape of cell 2 of row 1 of tableObject to "B1"
            set content of text range of text frame of shape of cell 1 of row 2 of tableObject to "A2"
            set content of text range of text frame of shape of cell 2 of row 2 of tableObject to "B2"
            move slide 3 of active presentation to before slide 2 of active presentation
            save active presentation in (POSIX file outputPath) as save as Open XML presentation
            close active presentation saving no
            set ownsPresentation to false
            repeat 100 times
                if (count of presentations) is beforeCount then exit repeat
                delay 0.1
            end repeat
            if (count of presentations) is not beforeCount then error "PowerPoint edit count was not restored"
            return "AIOS_POWERPOINT_EDITED"
        on error m number n
            if ownsPresentation then
                try
                    close active presentation saving no
                end try
            end if
            error m number n
        end try
    end tell
end run
"#;

const EXPORT_SCRIPT: &str = r#"
on run argv
    set stagedPath to item 1 of argv
    set outputPath to item 2 of argv
    set beforeCount to 0
    set ownsPresentation to false
    tell application id "com.microsoft.Powerpoint"
        try
            set beforeCount to count of presentations
            open (POSIX file stagedPath)
            repeat 100 times
                if (count of presentations) > beforeCount then exit repeat
                delay 0.1
            end repeat
            if (count of presentations) is not (beforeCount + 1) then error "PowerPoint export count did not increase deterministically"
            if (full name of active presentation as text) is not stagedPath then error "PowerPoint export identity mismatch"
            set ownsPresentation to true
            save active presentation in (POSIX file outputPath) as save as PDF
            close active presentation saving no
            set ownsPresentation to false
            repeat 100 times
                if (count of presentations) is beforeCount then exit repeat
                delay 0.1
            end repeat
            if (count of presentations) is not beforeCount then error "PowerPoint export count was not restored"
            return "AIOS_POWERPOINT_PDF_EXPORTED"
        on error m number n
            if ownsPresentation then
                try
                    close active presentation saving no
                end try
            end if
            error m number n
        end try
    end tell
end run
"#;

fn parse_read(path: &str, output: &str) -> Result<Value, PowerPointError> {
    let count_line = output
        .lines()
        .find_map(|line| line.strip_prefix("AIOS_SLIDE_COUNT="))
        .ok_or_else(|| PowerPointError::execution("PowerPoint read returned no slide count"))?;
    let slide_count = count_line.parse::<usize>().unwrap_or(0);
    let payload = output
        .split_once("AIOS_SLIDES=")
        .map(|(_, payload)| payload)
        .unwrap_or("");
    let slides = if payload.is_empty() {
        Vec::new()
    } else {
        payload
            .split('\u{1d}')
            .map(|record| {
                let fields = record.split('\u{1e}').collect::<Vec<_>>();
                json!({
                    "index": fields.first().and_then(|value| value.parse::<usize>().ok()).unwrap_or(0),
                    "title": fields.get(1).copied().unwrap_or(""),
                    "body": fields.get(2).copied().unwrap_or(""),
                    "text": fields.get(3).copied().unwrap_or("").chars().take(MAX_TEXT).collect::<String>(),
                    "imageCount": fields.get(4).and_then(|value| value.parse::<usize>().ok()).unwrap_or(0),
                    "tableCount": fields.get(5).and_then(|value| value.parse::<usize>().ok()).unwrap_or(0),
                    "basicShapeCount": fields.get(6).and_then(|value| value.parse::<usize>().ok()).unwrap_or(0),
                    "tableCells": fields.iter().skip(7).filter(|value| !value.is_empty()).collect::<Vec<_>>()
                })
            })
            .collect()
    };
    Ok(json!({
        "capability": "presentation.read",
        "selectedProvider": "microsoft-powerpoint",
        "resourceLocation": "local",
        "inputResource": path,
        "operationResult": {"slideCount": slide_count, "slides": slides},
        "warnings": [],
        "confirmationConsumed": true,
        "validationResult": "read"
    }))
}

pub(crate) fn read_powerpoint_presentation(input: &Value) -> Result<Value, PowerPointError> {
    let path = require_path(input, "path", true)?;
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("pptx");
    if !matches!(extension.to_ascii_lowercase().as_str(), "ppt" | "pptx") {
        return Err(PowerPointError::invalid(
            "presentation.read requires PPT/PPTX",
        ));
    }
    let (staged, root) = cache_path(extension)?;
    fs::copy(path, &staged)
        .map_err(|_| PowerPointError::execution("Unable to stage presentation"))?;
    let staged_string = staged.to_string_lossy().to_string();
    let result = run_osascript(READ_SCRIPT, &[&staged_string, &MAX_SLIDES.to_string()])
        .and_then(|output| parse_read(path, &output));
    let _ = fs::remove_dir_all(root);
    result
}

fn slide_texts(input: &Value) -> Result<Vec<(String, String)>, PowerPointError> {
    let slides = input
        .get("slides")
        .and_then(Value::as_array)
        .ok_or_else(|| PowerPointError::invalid("presentation.create requires slides"))?;
    if slides.len() != 3 || slides.len() > MAX_SLIDES {
        return Err(PowerPointError::invalid(
            "presentation.create requires exactly three bounded slides",
        ));
    }
    slides
        .iter()
        .map(|slide| {
            let title = slide
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let body = slide
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if title.is_empty()
                || body.is_empty()
                || title.chars().count() > MAX_CELL_TEXT
                || body.chars().count() > MAX_TEXT
            {
                return Err(PowerPointError::invalid(
                    "slide title/body is empty or unbounded",
                ));
            }
            Ok((title.to_owned(), body.to_owned()))
        })
        .collect()
}

pub(crate) fn create_powerpoint_presentation(input: &Value) -> Result<Value, PowerPointError> {
    let destination = require_path(input, "path", false)?;
    if Path::new(destination)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        != "pptx"
    {
        return Err(PowerPointError::invalid(
            "presentation.create requires PPTX output",
        ));
    }
    let image = require_image(input)?;
    validate_table(input)?;
    let slides = slide_texts(input)?;
    let (cache, root) = cache_path("pptx")?;
    let cache_string = cache.to_string_lossy().to_string();
    let result = run_osascript(
        CREATE_SCRIPT,
        &[
            &cache_string,
            image,
            &slides[0].0,
            &slides[0].1,
            &slides[1].0,
            &slides[1].1,
            &slides[2].0,
            &slides[2].1,
        ],
    );
    if !cache
        .metadata()
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false)
    {
        let _ = fs::remove_dir_all(&root);
        return Err(result.err().unwrap_or_else(|| {
            PowerPointError::execution("PowerPoint create produced no output")
        }));
    }
    publish(&cache, Path::new(destination))?;
    let _ = fs::remove_dir_all(root);
    let validation = read_powerpoint_presentation(&json!({"path": destination}))?;
    if validation["operationResult"]["slideCount"] != 3 {
        return Err(PowerPointError::execution(
            "PowerPoint create read-back validation failed",
        ));
    }
    Ok(json!({
        "capability": "presentation.create",
        "selectedProvider": "microsoft-powerpoint",
        "resourceLocation": "local",
        "outputResource": destination,
        "operationResult": "created",
        "warnings": [],
        "confirmationConsumed": true,
        "validationResult": validation["operationResult"]
    }))
}

pub(crate) fn edit_powerpoint_presentation(input: &Value) -> Result<Value, PowerPointError> {
    let source = require_path(input, "source", true)?;
    let destination = require_path(input, "destination", false)?;
    if Path::new(source)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        != "pptx"
        || Path::new(destination)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            != "pptx"
    {
        return Err(PowerPointError::invalid(
            "presentation.edit requires PPTX source and destination",
        ));
    }
    let image = require_image(input)?;
    validate_table(input)?;
    let title = input
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let body = input
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if title.is_empty()
        || body.is_empty()
        || title.chars().count() > MAX_CELL_TEXT
        || body.chars().count() > MAX_TEXT
    {
        return Err(PowerPointError::invalid(
            "presentation.edit title/body is invalid or unbounded",
        ));
    }
    let (staged, staged_root) = cache_path("pptx")?;
    let (cache, output_root) = cache_path("pptx")?;
    fs::copy(source, &staged)
        .map_err(|_| PowerPointError::execution("Unable to stage presentation edit"))?;
    let staged_string = staged.to_string_lossy().to_string();
    let cache_string = cache.to_string_lossy().to_string();
    let result = run_osascript(
        EDIT_SCRIPT,
        &[&staged_string, &cache_string, image, title, body],
    );
    if !cache
        .metadata()
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false)
    {
        let _ = fs::remove_dir_all(&staged_root);
        let _ = fs::remove_dir_all(&output_root);
        return Err(result
            .err()
            .unwrap_or_else(|| PowerPointError::execution("PowerPoint edit produced no output")));
    }
    publish(&cache, Path::new(destination))?;
    let _ = fs::remove_dir_all(staged_root);
    let _ = fs::remove_dir_all(output_root);
    let validation = read_powerpoint_presentation(&json!({"path": destination}))?;
    let slides = validation["operationResult"]["slides"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let has_objects = slides.iter().any(|slide| {
        slide["imageCount"].as_u64().unwrap_or(0) > 0
            && slide["tableCount"].as_u64().unwrap_or(0) > 0
            && slide["basicShapeCount"].as_u64().unwrap_or(0) > 0
            && slide["tableCells"] == json!(["A1", "B1", "A2", "B2"])
    });
    let mut mismatches = Vec::new();
    if validation["operationResult"]["slideCount"] != 4 {
        mismatches.push("slide-count");
    }
    if slides.first().and_then(|slide| slide["title"].as_str()) != Some(title) {
        mismatches.push("title");
    }
    if slides.first().and_then(|slide| slide["body"].as_str()) != Some(body) {
        mismatches.push("body");
    }
    if slides.get(1).and_then(|slide| slide["title"].as_str()) != Some("Slide 3") {
        mismatches.push("reorder");
    }
    if !has_objects {
        mismatches.push("objects");
    }
    if !mismatches.is_empty() {
        return Err(PowerPointError::execution(format!(
            "PowerPoint edit read-back validation failed for: {}",
            mismatches.join(", ")
        )));
    }
    Ok(json!({
        "capability": "presentation.edit",
        "selectedProvider": "microsoft-powerpoint",
        "resourceLocation": "local",
        "inputResource": source,
        "outputResource": destination,
        "operationResult": "edited-copy",
        "warnings": [],
        "confirmationConsumed": true,
        "validationResult": validation["operationResult"]
    }))
}

pub(crate) fn export_powerpoint_pdf(input: &Value) -> Result<Value, PowerPointError> {
    let source = require_path(input, "source", true)?;
    let destination = require_path(input, "destination", false)?;
    if Path::new(source)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        != "pptx"
        || Path::new(destination)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            != "pdf"
    {
        return Err(PowerPointError::invalid(
            "presentation.export requires PPTX source and PDF destination",
        ));
    }
    let (staged, staged_root) = cache_path("pptx")?;
    let (cache, output_root) = cache_path("pdf")?;
    fs::copy(source, &staged)
        .map_err(|_| PowerPointError::execution("Unable to stage PDF export"))?;
    let staged_string = staged.to_string_lossy().to_string();
    let cache_string = cache.to_string_lossy().to_string();
    let result = run_osascript(EXPORT_SCRIPT, &[&staged_string, &cache_string]);
    for _ in 0..100 {
        if cache
            .metadata()
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if !cache
        .metadata()
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false)
    {
        let _ = fs::remove_dir_all(&staged_root);
        let _ = fs::remove_dir_all(&output_root);
        return Err(result
            .err()
            .unwrap_or_else(|| PowerPointError::execution("PowerPoint export produced no PDF")));
    }
    publish(&cache, Path::new(destination))?;
    let _ = fs::remove_dir_all(staged_root);
    let _ = fs::remove_dir_all(output_root);
    Ok(json!({
        "capability": "presentation.export",
        "selectedProvider": "microsoft-powerpoint",
        "resourceLocation": "local",
        "inputResource": source,
        "outputResource": destination,
        "operationResult": "exported-pdf",
        "warnings": [],
        "confirmationConsumed": true,
        "validationResult": "non-empty-pdf"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAFE_TEST_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xb5,
        0x1c, 0x0c, 0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64,
        0xf8, 0x0f, 0x00, 0x01, 0x05, 0x01, 0x01, 0x27, 0x18, 0xe3, 0x66, 0x00, 0x00, 0x00, 0x00,
        0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    fn slides() -> Value {
        json!([
            {"title":"Slide 1","body":"Body 1"},
            {"title":"Slide 2","body":"Body 2"},
            {"title":"Slide 3","body":"Body 3"}
        ])
    }

    #[test]
    fn validation_is_bounded_and_fail_closed() {
        assert!(
            read_powerpoint_presentation(&json!({"path":"relative.pptx"}))
                .unwrap_err()
                .invalid_request
        );
        let root =
            std::env::temp_dir().join(format!("ai-os-powerpoint-unit-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.pptx");
        let image = root.join("image.svg");
        let existing = root.join("existing.pptx");
        fs::write(&source, b"fixture").unwrap();
        fs::write(&image, b"<svg/>").unwrap();
        fs::write(&existing, b"preserve").unwrap();
        assert!(
            create_powerpoint_presentation(
                &json!({"path":existing,"imagePath":image,"slides":slides(),"table":{"rows":2,"columns":2}})
            )
            .unwrap_err()
            .invalid_request
        );
        assert!(edit_powerpoint_presentation(&json!({"source":source,"destination":root.join("output.pptx"),"imagePath":image,"title":"x","body":"y","table":{"rows":2,"columns":2}})).unwrap_err().invalid_request);
        assert!(
            export_powerpoint_pdf(&json!({"source":source,"destination":root.join("wrong.pptx")}))
                .unwrap_err()
                .invalid_request
        );
        assert!(
            validate_table(&json!({"table":{"rows":11,"columns":2}}))
                .unwrap_err()
                .invalid_request
        );
        let existing_pdf = root.join("existing.pdf");
        fs::write(&existing_pdf, b"preserve").unwrap();
        assert!(
            export_powerpoint_pdf(&json!({"source":source,"destination":existing_pdf}))
                .unwrap_err()
                .invalid_request
        );
        assert_eq!(fs::read(root.join("existing.pdf")).unwrap(), b"preserve");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "requires Microsoft PowerPoint and macOS Automation authorization"]
    fn powerpoint_realistic_workflow_real_e2e() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ai-os-powerpoint-e2e-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("Input.pptx");
        let output = root.join("Output.pptx");
        let pdf = root.join("Output.pdf");
        // PowerPoint's own container, for the same reason Word's test uses
        // Word's: a file handed to a sandboxed application from a temp
        // directory raises a grant panel that no automated run can answer, and
        // it then blocks every later automation of that application. This one
        // has been passing on luck.
        let (image, image_root) = cache_path("png").unwrap();
        fs::write(&image, SAFE_TEST_PNG).unwrap();
        let before = run_osascript("tell application id \"com.microsoft.Powerpoint\" to return (count of presentations) as text", &[]).unwrap();
        create_powerpoint_presentation(&json!({"path":source,"imagePath":image,"slides":slides(),"table":{"rows":2,"columns":2}}))
            .unwrap();
        let original = fs::read(&source).unwrap();
        edit_powerpoint_presentation(&json!({"source":source,"destination":output,"imagePath":image,"title":"Modified Slide 1","body":"Modified Body 1","table":{"rows":2,"columns":2}})).unwrap();
        assert_eq!(fs::read(root.join("Input.pptx")).unwrap(), original);
        export_powerpoint_pdf(&json!({"source":output,"destination":pdf})).unwrap();
        assert!(fs::metadata(root.join("Output.pdf")).unwrap().len() > 0);
        assert_eq!(run_osascript("tell application id \"com.microsoft.Powerpoint\" to return (count of presentations) as text", &[]).unwrap(), before);
        let _ = fs::remove_dir_all(image_root);
        fs::remove_dir_all(root).unwrap();
    }
}
