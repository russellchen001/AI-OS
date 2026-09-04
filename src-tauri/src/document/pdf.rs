//! PDF, through macOS itself.
//!
//! The Office skill could write a PDF from every application it drives and read
//! none, which made PDF a one-way street: hand it a PDF and it could do
//! nothing. This is the other direction, and it needs no application and no
//! installed dependency -- PDFKit and Vision are part of macOS.
//!
//! Everything here was probed against real files first
//! (`verify/probe_pdf_native_capability.sh` and
//! `verify/probe_pdf_ocr_capability.sh`), and three findings shape it:
//!
//!   * PDFKit reads text per page, reports the page count, and says whether a
//!     document is encrypted or locked -- so a locked file is refused rather
//!     than read as empty.
//!   * A page that is only an image returns ZERO characters. That is what a
//!     scan is, and returning empty text for it would be a silent lie. Such a
//!     page is sent to Vision, and the result says which pages were read from
//!     the text layer and which were recognised, because those are not the same
//!     kind of evidence.
//!   * Vision runs SYNCHRONOUSLY through `performRequests`, so it fits the way
//!     every other adapter here works, with no callback plumbing and no
//!     background queue.

use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// A document far longer than anything a caller should be handed whole.
const MAX_PAGES: usize = 500;
/// Recognition is slow compared with reading a text layer, so it is bounded
/// separately and the result says when the bound was reached.
const MAX_OCR_PAGES: usize = 50;
const MAX_TEXT_CHARS: usize = 256 * 1024;
const MAX_MERGE_SOURCES: usize = 32;

#[derive(Debug)]
pub(crate) struct PdfError {
    pub(crate) invalid_request: bool,
    pub(crate) message: String,
}

impl PdfError {
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

fn existing_pdf<'a>(input: &'a Value, field: &str, operation: &str) -> Result<&'a str, PdfError> {
    let path = input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| PdfError::invalid(format!("{operation} requires {field}")))?;

    let target = Path::new(path);

    if !target.is_absolute() {
        return Err(PdfError::invalid(format!(
            "{operation} requires an absolute {field}"
        )));
    }

    if target
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
        != Some("pdf")
    {
        return Err(PdfError::invalid(format!("{operation} requires a .pdf {field}")));
    }

    if !target.is_file() {
        return Err(PdfError::invalid(format!("{operation} {field} does not exist")));
    }

    Ok(path)
}

fn new_pdf<'a>(input: &'a Value, field: &str, operation: &str) -> Result<&'a str, PdfError> {
    let path = input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| PdfError::invalid(format!("{operation} requires {field}")))?;

    let target = Path::new(path);

    if !target.is_absolute() {
        return Err(PdfError::invalid(format!(
            "{operation} requires an absolute {field}"
        )));
    }

    if target
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
        != Some("pdf")
    {
        return Err(PdfError::invalid(format!("{operation} requires a .pdf {field}")));
    }

    if target.exists() {
        return Err(PdfError::invalid(format!(
            "{operation} refuses to overwrite an existing {field}"
        )));
    }

    let parent = target
        .parent()
        .ok_or_else(|| PdfError::invalid(format!("{operation} requires an absolute {field}")))?;

    if !parent.is_dir() {
        return Err(PdfError::invalid(format!(
            "{operation} {field} parent directory does not exist"
        )));
    }

    Ok(path)
}

fn run_jxa(script: &str, args: &[&str]) -> Result<String, PdfError> {
    let mut child = Command::new("/usr/bin/osascript")
        .args(["-l", "JavaScript", "-"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| PdfError::execution("Unable to start the PDF reader"))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| PdfError::execution("Unable to open the PDF reader input"))?;

        stdin
            .write_all(script.as_bytes())
            .map_err(|_| PdfError::execution("Unable to write the PDF script"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|_| PdfError::execution("The PDF reader did not complete"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();

        return Err(PdfError::execution(if stderr.is_empty() {
            "The PDF reader failed".to_owned()
        } else {
            format!("The PDF reader failed: {stderr}")
        }));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The payload every script returns: JSON, so the shape is checked once here
/// rather than parsed out of prose.
fn parse_payload(output: &str) -> Result<Value, PdfError> {
    let payload: Value = serde_json::from_str(output)
        .map_err(|_| PdfError::execution(format!("The PDF reader returned no result: {output}")))?;

    if let Some(error) = payload.get("error").and_then(Value::as_str) {
        // A locked or malformed file is the caller's problem to fix, not a
        // failure of the machinery.
        let invalid = payload
            .get("invalidRequest")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        return Err(if invalid {
            PdfError::invalid(error.to_owned())
        } else {
            PdfError::execution(error.to_owned())
        });
    }

    Ok(payload)
}

const READ_SCRIPT: &str = r#"
ObjC.import('Quartz');
ObjC.import('AppKit');
ObjC.import('Vision');

function fail(message, invalidRequest) {
  return JSON.stringify({ error: message, invalidRequest: !!invalidRequest });
}

// Vision runs synchronously through performRequests, which is why recognition
// fits here at all: no callbacks, no queue, one answer per page.
function recognise(page) {
  const bounds = page.boundsForBox($.kPDFDisplayBoxMediaBox);
  // Rendered above natural size: recognising a thumbnail measures the
  // thumbnail, not the page.
  const size = $.NSMakeSize(bounds.size.width * 3, bounds.size.height * 3);
  const image = page.thumbnailOfSizeForBox(size, $.kPDFDisplayBoxMediaBox);
  if (!image.js) return null;

  const cg = image.CGImageForProposedRectContextHints($(), $(), $());
  if (!cg) return null;

  const request = $.VNRecognizeTextRequest.alloc.init;
  request.recognitionLevel = 1;
  const handler = $.VNImageRequestHandler.alloc.initWithCGImageOptions(cg, $({}));
  if (!handler.performRequestsError($([request]), $())) return null;

  const results = request.results;
  const lines = [];
  for (let i = 0; i < results.count; i++) {
    const candidate = results.objectAtIndex(i).topCandidates(1).objectAtIndex(0);
    lines.push(ObjC.unwrap(candidate.string));
  }
  return lines.join('\n');
}

function run(argv) {
  const path = argv[0];
  const maxPages = parseInt(argv[1], 10);
  const maxOcrPages = parseInt(argv[2], 10);
  const allowOcr = argv[3] === 'true';

  const doc = $.PDFDocument.alloc.initWithURL($.NSURL.fileURLWithPath(path));
  if (!doc.js) return fail('The file is not a readable PDF', true);

  // A locked document is refused rather than reported as an empty one.
  if (doc.isLocked) return fail('The PDF is locked and cannot be read without its password', true);

  // Coerced, because the bridge hands back an object that JSON renders as a
  // string: a caller doing arithmetic on pageCount would get "2" + 1 = "21".
  const pageCount = Number(doc.pageCount);
  if (pageCount > maxPages) {
    return fail('The PDF has ' + pageCount + ' pages, more than this reader will return', true);
  }

  const pages = [];
  let recognised = 0;
  let ocrTruncated = false;

  for (let i = 0; i < pageCount; i++) {
    const page = doc.pageAtIndex(i);
    const layer = ObjC.unwrap(page.string) || '';

    if (layer.trim().length > 0) {
      pages.push({ page: i + 1, text: layer, source: 'text-layer' });
      continue;
    }

    // No text layer: this page is an image, which is what a scan is.
    if (!allowOcr) {
      pages.push({ page: i + 1, text: '', source: 'image-only' });
      continue;
    }

    if (recognised >= maxOcrPages) {
      ocrTruncated = true;
      pages.push({ page: i + 1, text: '', source: 'image-only' });
      continue;
    }

    const text = recognise(page);
    recognised += 1;
    pages.push({
      page: i + 1,
      text: text === null ? '' : text,
      source: text === null ? 'image-only' : 'ocr',
    });
  }

  return JSON.stringify({
    pageCount: pageCount,
    encrypted: !!doc.isEncrypted,
    pages: pages,
    recognisedPages: recognised,
    ocrTruncated: ocrTruncated,
  });
}
"#;

pub(crate) fn read_pdf_document(input: &Value) -> Result<Value, PdfError> {
    let path = existing_pdf(input, "path", "document.read")?;

    // Recognition is on by default, because a caller who asks to read a
    // document means the words in it, and a scan that reads as empty is the
    // most useless possible answer. It can be turned off for a caller that
    // wants only what the file itself declares.
    let allow_ocr = input
        .get("ocr")
        .and_then(Value::as_bool)
        .unwrap_or(true)
        .to_string();

    let payload = parse_payload(&run_jxa(
        READ_SCRIPT,
        &[
            path,
            &MAX_PAGES.to_string(),
            &MAX_OCR_PAGES.to_string(),
            &allow_ocr,
        ],
    )?)?;

    let pages = payload["pages"].as_array().cloned().unwrap_or_default();

    let full_text: String = pages
        .iter()
        .filter_map(|page| page["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let text: String = full_text.chars().take(MAX_TEXT_CHARS).collect();

    let image_only = pages
        .iter()
        .filter(|page| page["source"] == "image-only")
        .count();
    let recognised = payload["recognisedPages"].as_u64().unwrap_or(0);

    let mut warnings: Vec<String> = Vec::new();

    if full_text.chars().count() > MAX_TEXT_CHARS {
        warnings.push("PDF text was truncated at the deterministic extraction limit.".to_owned());
    }

    if recognised > 0 {
        // Said plainly, because recognised text and text the file declares are
        // not the same kind of evidence and a caller may care which it has.
        warnings.push(format!(
            "{recognised} page(s) had no text layer and were read by recognition, which can misread."
        ));
    }

    if payload["ocrTruncated"] == json!(true) {
        warnings.push(format!(
            "More than {MAX_OCR_PAGES} pages needed recognition; the rest were left unread."
        ));
    }

    if image_only > 0 && recognised == 0 {
        warnings.push(
            "The PDF has no text layer. It is a scan, and recognition was not run.".to_owned(),
        );
    }

    Ok(json!({
        "capability": "document.read",
        "selectedProvider": "macos-pdf",
        "resourceLocation": "local",
        "inputResource": path,
        "operationResult": {
            "text": text,
            "pageCount": payload["pageCount"],
            "pages": pages,
            "recognisedPages": recognised,
            "imageOnlyPages": image_only,
            "encrypted": payload["encrypted"],
        },
        "warnings": warnings,
        "confirmationConsumed": true,
        "validationResult": "read",
    }))
}

const MERGE_SCRIPT: &str = r#"
ObjC.import('Quartz');

function fail(message, invalidRequest) {
  return JSON.stringify({ error: message, invalidRequest: !!invalidRequest });
}

function run(argv) {
  const destination = argv[0];
  const sources = argv.slice(1);

  const merged = $.PDFDocument.alloc.init;
  const contributed = [];

  for (const source of sources) {
    const doc = $.PDFDocument.alloc.initWithURL($.NSURL.fileURLWithPath(source));
    if (!doc.js) return fail(source + ' is not a readable PDF', true);
    if (doc.isLocked) return fail(source + ' is locked and cannot be merged', true);

    const count = Number(doc.pageCount);
    for (let i = 0; i < count; i++) {
      merged.insertPageAtIndex(doc.pageAtIndex(i), merged.pageCount);
    }
    contributed.push({ source: source, pages: count });
  }

  if (!merged.writeToURL($.NSURL.fileURLWithPath(destination))) {
    return fail('The merged PDF could not be written');
  }

  return JSON.stringify({ pageCount: Number(merged.pageCount), sources: contributed });
}
"#;

pub(crate) fn merge_pdf_documents(input: &Value) -> Result<Value, PdfError> {
    let sources = input
        .get("sources")
        .and_then(Value::as_array)
        .filter(|sources| !sources.is_empty())
        .ok_or_else(|| PdfError::invalid("document.merge requires sources"))?;

    if sources.len() > MAX_MERGE_SOURCES {
        return Err(PdfError::invalid(format!(
            "document.merge accepts at most {MAX_MERGE_SOURCES} sources"
        )));
    }

    let mut paths = Vec::new();

    for (index, source) in sources.iter().enumerate() {
        let entry = json!({ "source": source });
        let path = existing_pdf(&entry, "source", &format!("document.merge source {}", index + 1))?;
        paths.push(path.to_owned());
    }

    let destination = new_pdf(input, "destination", "document.merge")?;

    if paths.iter().any(|source| source == destination) {
        return Err(PdfError::invalid(
            "document.merge destination must not be one of its sources",
        ));
    }

    let mut args: Vec<&str> = vec![destination];
    args.extend(paths.iter().map(String::as_str));

    let payload = parse_payload(&run_jxa(MERGE_SCRIPT, &args)?)?;

    if !Path::new(destination).is_file() {
        return Err(PdfError::execution("The merged PDF was not left at the destination"));
    }

    Ok(json!({
        "capability": "document.merge",
        "selectedProvider": "macos-pdf",
        "resourceLocation": "local",
        "inputResource": paths,
        "operationResult": {
            "status": "merged",
            "destination": destination,
            "pageCount": payload["pageCount"],
            "sources": payload["sources"],
        },
        "warnings": [],
        "confirmationConsumed": true,
        "validationResult": "merged",
    }))
}

const SPLIT_SCRIPT: &str = r#"
ObjC.import('Quartz');

function fail(message, invalidRequest) {
  return JSON.stringify({ error: message, invalidRequest: !!invalidRequest });
}

function run(argv) {
  const source = argv[0];
  const destination = argv[1];
  const wanted = argv[2].split(',').map(function (value) { return parseInt(value, 10); });

  const doc = $.PDFDocument.alloc.initWithURL($.NSURL.fileURLWithPath(source));
  if (!doc.js) return fail(source + ' is not a readable PDF', true);
  if (doc.isLocked) return fail(source + ' is locked and cannot be split', true);

  const out = $.PDFDocument.alloc.init;
  const sourcePageCount = Number(doc.pageCount);

  for (const page of wanted) {
    if (page < 1 || page > sourcePageCount) {
      return fail('Page ' + page + ' is outside a document of ' + sourcePageCount + ' pages', true);
    }
    out.insertPageAtIndex(doc.pageAtIndex(page - 1), out.pageCount);
  }

  if (!out.writeToURL($.NSURL.fileURLWithPath(destination))) {
    return fail('The extracted PDF could not be written');
  }

  return JSON.stringify({ pageCount: Number(out.pageCount), sourcePageCount: sourcePageCount });
}
"#;

/// `1-3,7` becomes `[1, 2, 3, 7]`.
///
/// Ranges are how people describe pages, and rejecting anything but a bare list
/// would push that parsing onto every caller.
fn parse_pages(spec: &str) -> Result<Vec<usize>, PdfError> {
    let mut pages = Vec::new();

    for part in spec.split(',').map(str::trim).filter(|part| !part.is_empty()) {
        if let Some((start, end)) = part.split_once('-') {
            let start: usize = start
                .trim()
                .parse()
                .map_err(|_| PdfError::invalid(format!("{part} is not a page range")))?;
            let end: usize = end
                .trim()
                .parse()
                .map_err(|_| PdfError::invalid(format!("{part} is not a page range")))?;

            if start == 0 || end < start {
                return Err(PdfError::invalid(format!("{part} is not a page range")));
            }

            if end - start + 1 > MAX_PAGES {
                return Err(PdfError::invalid(format!(
                    "{part} covers more than {MAX_PAGES} pages"
                )));
            }

            pages.extend(start..=end);
        } else {
            let page: usize = part
                .parse()
                .map_err(|_| PdfError::invalid(format!("{part} is not a page number")))?;

            if page == 0 {
                return Err(PdfError::invalid("page numbers start at 1"));
            }

            pages.push(page);
        }
    }

    if pages.is_empty() {
        return Err(PdfError::invalid("document.split requires pages"));
    }

    if pages.len() > MAX_PAGES {
        return Err(PdfError::invalid(format!(
            "document.split accepts at most {MAX_PAGES} pages"
        )));
    }

    Ok(pages)
}

pub(crate) fn split_pdf_document(input: &Value) -> Result<Value, PdfError> {
    let source = existing_pdf(input, "source", "document.split")?;
    let destination = new_pdf(input, "destination", "document.split")?;

    if source == destination {
        return Err(PdfError::invalid(
            "document.split source and destination must differ",
        ));
    }

    let spec = input
        .get("pages")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| PdfError::invalid("document.split requires pages"))?;

    let pages = parse_pages(spec)?;
    let encoded = pages
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",");

    let payload = parse_payload(&run_jxa(SPLIT_SCRIPT, &[source, destination, &encoded])?)?;

    if !Path::new(destination).is_file() {
        return Err(PdfError::execution("The extracted PDF was not left at the destination"));
    }

    Ok(json!({
        "capability": "document.split",
        "selectedProvider": "macos-pdf",
        "resourceLocation": "local",
        "inputResource": source,
        "operationResult": {
            "status": "split",
            "destination": destination,
            "pages": pages,
            "pageCount": payload["pageCount"],
            "sourcePageCount": payload["sourcePageCount"],
        },
        "warnings": [],
        "confirmationConsumed": true,
        "validationResult": "split",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_ranges_are_read_the_way_people_write_them() {
        assert_eq!(parse_pages("1").unwrap(), vec![1]);
        assert_eq!(parse_pages("1-3").unwrap(), vec![1, 2, 3]);
        assert_eq!(parse_pages("1-3,7").unwrap(), vec![1, 2, 3, 7]);
        assert_eq!(parse_pages(" 2 , 4 - 5 ").unwrap(), vec![2, 4, 5]);
        // Order and repetition are the caller's business: extracting page 3
        // twice, or 3 before 1, are both things people mean to do.
        assert_eq!(parse_pages("3,1,3").unwrap(), vec![3, 1, 3]);

        // A stray separator is sloppy but unambiguous, so it is tolerated
        // rather than refused: "1,," can only mean page 1, and rejecting it
        // would be strictness that costs the caller something and buys nothing.
        assert_eq!(parse_pages("1,,").unwrap(), vec![1]);
        assert_eq!(parse_pages(",2,").unwrap(), vec![2]);

        // What is refused is what cannot be read as pages at all, or names a
        // page that cannot exist.
        for rejected in ["", "   ", ",", "0", "0-2", "3-1", "a", "1-b", "-", "1-"] {
            assert!(
                parse_pages(rejected).is_err(),
                "{rejected:?} should not parse as pages"
            );
        }
    }

    #[test]
    fn paths_fail_closed_before_anything_is_opened() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("in.pdf");
        std::fs::write(&source, b"%PDF-1.4 placeholder").unwrap();
        let taken = root.path().join("taken.pdf");
        std::fs::write(&taken, b"placeholder").unwrap();

        let existing = source.to_str().unwrap().to_owned();

        for (label, request) in [
            ("no path", json!({})),
            ("relative path", json!({"path": "in.pdf"})),
            ("not a pdf", json!({"path": root.path().join("in.docx").to_str().unwrap()})),
            ("missing file", json!({"path": root.path().join("absent.pdf").to_str().unwrap()})),
        ] {
            let error = read_pdf_document(&request).unwrap_err();
            assert!(error.invalid_request, "{label} should be a request problem");
        }

        for (label, request) in [
            ("no sources", json!({"destination": root.path().join("out.pdf")})),
            (
                "empty sources",
                json!({"sources": [], "destination": root.path().join("out.pdf")}),
            ),
            (
                "destination exists",
                json!({"sources": [existing.clone()], "destination": taken.to_str().unwrap()}),
            ),
            (
                // Merging into one of its own inputs would destroy an input
                // half way through.
                "destination is a source",
                json!({"sources": [existing.clone()], "destination": existing.clone()}),
            ),
        ] {
            let error = merge_pdf_documents(&request).unwrap_err();
            assert!(error.invalid_request, "{label} should be a request problem");
        }

        for (label, request) in [
            ("no pages", json!({"source": existing.clone(), "destination": root.path().join("out.pdf").to_str().unwrap()})),
            (
                "same path twice",
                json!({"source": existing.clone(), "destination": existing.clone(), "pages": "1"}),
            ),
            (
                "destination exists",
                json!({"source": existing.clone(), "destination": taken.to_str().unwrap(), "pages": "1"}),
            ),
        ] {
            let error = split_pdf_document(&request).unwrap_err();
            assert!(error.invalid_request, "{label} should be a request problem");
        }

        assert_eq!(std::fs::read(&taken).unwrap(), b"placeholder");
    }

    /// Everything, against real PDFs, with the answer known in advance.
    ///
    /// The scan is built by rendering a text PDF to an image and rebuilding it
    /// as an image-only page, so what recognition must recover is known exactly
    /// rather than judged by eye -- which is the only way to assert on OCR
    /// output without hand-waving.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires macOS PDFKit and Vision"]
    fn pdf_is_read_split_merged_and_recognised_real_e2e() {
        let root = std::env::temp_dir().join(format!("ai-os-pdf-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();

        // Fixtures, built with no application at all and each proving its own
        // premise. `cupsfilter` turns plain text into a PDF that carries a real
        // text layer; drawing words into an image and wrapping that as a page
        // is what a scan is. Both were established by probe before this test
        // was written, along with what does NOT work: a FreeText annotation
        // produces no text layer at all, so a PDF built that way would have
        // made the text-layer assertions vacuous.
        let alpha_txt = root.join("alpha.txt");
        let bravo_txt = root.join("bravo.txt");
        let alpha_pdf = root.join("alpha.pdf");
        let bravo_pdf = root.join("bravo.pdf");
        let scan_pdf = root.join("scan.pdf");

        std::fs::write(&alpha_txt, b"Alpha paragraph.\n").unwrap();
        std::fs::write(&bravo_txt, b"Bravo paragraph.\n").unwrap();

        for (source, target) in [(&alpha_txt, &alpha_pdf), (&bravo_txt, &bravo_pdf)] {
            let produced = Command::new("/usr/sbin/cupsfilter")
                .arg(source)
                .output()
                .expect("cupsfilter is part of macOS");
            assert!(produced.status.success(), "cupsfilter refused the fixture");
            std::fs::write(target, produced.stdout).unwrap();
        }

        let built = parse_payload(
            &run_jxa(
                r#"
ObjC.import('Quartz');
ObjC.import('AppKit');

function run(argv) {
  // A page that is only an image of legible words, which is what a scan is.
  const size = $.NSMakeSize(612, 792);
  const image = $.NSImage.alloc.initWithSize(size);
  image.lockFocus;
  $.NSColor.whiteColor.set;
  $.NSBezierPath.fillRect($.NSMakeRect(0, 0, size.width, size.height));
  const attributes = $.NSDictionary.dictionaryWithObjectForKey(
    $.NSFont.fontWithNameSize('Helvetica', 48), $.NSFontAttributeName);
  $('Scanned heading').drawAtPointWithAttributes($.NSMakePoint(72, 600), attributes);
  image.unlockFocus;

  const doc = $.PDFDocument.alloc.init;
  doc.insertPageAtIndex($.PDFPage.alloc.initWithImage(image), 0);
  doc.writeToURL($.NSURL.fileURLWithPath(argv[0]));

  const back = $.PDFDocument.alloc.initWithURL($.NSURL.fileURLWithPath(argv[0]));
  return JSON.stringify({
    scanTextLayer: (ObjC.unwrap(back.string) || '').trim().length,
  });
}
"#,
                &[scan_pdf.to_str().unwrap()],
            )
            .unwrap(),
        )
        .unwrap();

        // The scan must genuinely have no text layer, or the recognition
        // assertion below would be proving nothing.
        assert_eq!(built["scanTextLayer"], 0, "the scan fixture is not a scan");

        // Merge first, so the split below has something with more than one page
        // to take from -- and so merge is proven on real files rather than by
        // its return value alone.
        let text_pdf = root.join("text.pdf");
        let merge = merge_pdf_documents(&json!({
            "sources": [alpha_pdf.to_str().unwrap(), bravo_pdf.to_str().unwrap()],
            "destination": text_pdf.to_str().unwrap(),
        }))
        .unwrap();

        assert_eq!(merge["operationResult"]["pageCount"], 2);

        // Reading a text PDF comes from the text layer, and says so.
        let read = read_pdf_document(&json!({"path": text_pdf.to_str().unwrap()})).unwrap();
        let result = &read["operationResult"];

        assert_eq!(read["selectedProvider"], "macos-pdf");
        assert_eq!(result["pageCount"], 2);
        assert_eq!(result["recognisedPages"], 0);
        assert_eq!(result["pages"][0]["source"], "text-layer");
        assert!(
            result["text"].as_str().unwrap().contains("Alpha paragraph."),
            "read back {result:#?}"
        );
        assert!(
            result["text"].as_str().unwrap().contains("Bravo paragraph."),
            "read back {result:#?}"
        );
        // Nothing was recognised, so nothing warns about recognition.
        assert!(read["warnings"].as_array().unwrap().is_empty());

        // Reading a scan recognises it, and says THAT, because recognised text
        // and declared text are not the same kind of evidence.
        let scanned = read_pdf_document(&json!({"path": scan_pdf.to_str().unwrap()})).unwrap();
        let scanned_result = &scanned["operationResult"];

        assert_eq!(scanned_result["recognisedPages"], 1);
        assert_eq!(scanned_result["pages"][0]["source"], "ocr");
        assert!(
            scanned_result["text"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("scanned heading"),
            "recognition returned {:?}",
            scanned_result["text"]
        );
        assert!(
            scanned["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|warning| warning.as_str().unwrap().contains("recognition")),
            "a recognised read must say so: {:#?}",
            scanned["warnings"]
        );

        // Turning recognition off leaves the scan honestly empty rather than
        // silently so.
        let declared_only =
            read_pdf_document(&json!({"path": scan_pdf.to_str().unwrap(), "ocr": false})).unwrap();
        assert_eq!(declared_only["operationResult"]["pages"][0]["source"], "image-only");
        assert!(declared_only["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().unwrap().contains("scan")));

        // Split takes the page asked for, and only that page -- checked by
        // what the extracted file says, not by its page count.
        let second = root.join("second.pdf");
        let split = split_pdf_document(&json!({
            "source": text_pdf.to_str().unwrap(),
            "destination": second.to_str().unwrap(),
            "pages": "2",
        }))
        .unwrap();

        assert_eq!(split["operationResult"]["pageCount"], 1);

        let only = read_pdf_document(&json!({"path": second.to_str().unwrap()})).unwrap();
        let only_text = only["operationResult"]["text"].as_str().unwrap().to_owned();
        assert!(only_text.contains("Bravo"), "extracted page said {only_text:?}");
        assert!(!only_text.contains("Alpha"), "extracted the wrong page: {only_text:?}");

        // And refusing to overwrite is proven against a real file, not only
        // against the path checks.
        let refused = merge_pdf_documents(&json!({
            "sources": [alpha_pdf.to_str().unwrap()],
            "destination": text_pdf.to_str().unwrap(),
        }))
        .unwrap_err();
        assert!(refused.invalid_request);

        std::fs::remove_dir_all(&root).unwrap();
    }
}
