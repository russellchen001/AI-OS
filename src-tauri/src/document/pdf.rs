//! PDF, on any machine.
//!
//! The Office skill could write a PDF from every application it drives and read
//! none, so handing it a PDF got nothing. This closes that -- and it does so in
//! Rust, because PDF is a FORMAT, not an application.
//!
//! The first version of this module drove macOS PDFKit, which made a whole
//! capability depend on which operating system the person happens to run. That
//! is the same mistake as making it depend on which application they installed,
//! and the owner rejected it for the same reason: *"你不能要求用户用什么系统"*.
//! `structured.rs` already showed the shape -- read the file itself, in Rust,
//! everywhere -- and this follows it.
//!
//! Recognition is the one part that genuinely cannot be portable: reading words
//! out of a picture needs a trained model, and no such thing is guaranteed on
//! an arbitrary machine. So it is an ENHANCEMENT, never a requirement. Where
//! the platform provides one it is used; where it does not, the page is
//! reported as an image with no text rather than silently returned empty, and
//! everything else -- text, page counts, merging, splitting -- works regardless.

use lopdf::{Document, Object, ObjectId};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// A document far longer than anything a caller should be handed whole.
const MAX_PAGES: usize = 500;
/// A page whose content stream decompresses past this is refused rather than
/// expanded: a small file can otherwise claim an unbounded amount of memory.
const MAX_PAGE_BYTES: usize = 16 * 1024 * 1024;
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

/// Open a PDF, refusing the ones that cannot honestly be read.
fn open(path: &str, operation: &str) -> Result<Document, PdfError> {
    let document = Document::load(path)
        .map_err(|error| PdfError::invalid(format!("{path} is not a readable PDF: {error}")))?;

    // An encrypted document is refused rather than reported as an empty one.
    if document.is_encrypted() {
        return Err(PdfError::invalid(format!(
            "{operation} cannot read {path}: it is encrypted"
        )));
    }

    Ok(document)
}

fn page_numbers(document: &Document) -> Vec<u32> {
    document.get_pages().keys().copied().collect()
}

pub(crate) fn read_pdf_document(input: &Value) -> Result<Value, PdfError> {
    let path = existing_pdf(input, "path", "document.read")?;
    let document = open(path, "document.read")?;

    let numbers = page_numbers(&document);

    if numbers.len() > MAX_PAGES {
        return Err(PdfError::invalid(format!(
            "The PDF has {} pages, more than this reader will return",
            numbers.len()
        )));
    }

    // Recognition is on by default, because a caller who asks to read a
    // document means the words in it, and a scan that reads as empty is the
    // most useless possible answer. It can be turned off for a caller that
    // wants only what the file itself declares.
    let allow_recognition = input.get("ocr").and_then(Value::as_bool).unwrap_or(true);

    let mut pages = Vec::new();
    let mut without_text = Vec::new();

    for number in &numbers {
        // Asked for one page at a time on purpose. The chunked extractor
        // flat-maps, so a single page can produce several chunks and the
        // results do NOT line up one-to-one with the pages asked for. Indexing
        // into a multi-page call silently attributes one page's text to
        // another -- which showed up as a merged page reading as empty while
        // its content stream plainly contained the words.
        let text = document
            .extract_text_chunks_with_limit(&[*number], MAX_PAGE_BYTES)
            .into_iter()
            .filter_map(Result::ok)
            .collect::<Vec<_>>()
            .join("");
        // Page layout leaves a tail of padding spaces and blank lines. They
        // carry nothing and make every page read as mostly whitespace.
        let text = text.trim_end().to_owned();

        if text.trim().is_empty() {
            // No text of its own. That is what a scanned page is, and returning
            // it as empty text would be a silent lie about the document.
            without_text.push(*number);
            pages.push(json!({"page": number, "text": "", "source": "image-only"}));
        } else {
            pages.push(json!({"page": number, "text": text, "source": "text-layer"}));
        }
    }

    let mut warnings: Vec<String> = Vec::new();
    let mut recognised = 0usize;

    if !without_text.is_empty() && allow_recognition {
        match recognise(path, &without_text) {
            Recognition::Unavailable => warnings.push(format!(
                "{} page(s) have no text of their own, which is what a scan is. Reading words \
                 out of a picture needs recognition, which this platform does not provide.",
                without_text.len()
            )),
            Recognition::Done(recognised_pages) => {
                for (number, text) in recognised_pages {
                    if text.trim().is_empty() {
                        continue;
                    }

                    if let Some(entry) = pages
                        .iter_mut()
                        .find(|page| page["page"].as_u64() == Some(u64::from(number)))
                    {
                        *entry = json!({"page": number, "text": text, "source": "ocr"});
                        recognised += 1;
                    }
                }

                if recognised > 0 {
                    // Said plainly, because recognised text and text the file
                    // declares are not the same kind of evidence.
                    warnings.push(format!(
                        "{recognised} page(s) had no text of their own and were read by \
                         recognition, which can misread."
                    ));
                }
            }
        }
    }

    let image_only = pages
        .iter()
        .filter(|page| page["source"] == "image-only")
        .count();

    if image_only > 0 && !allow_recognition {
        warnings.push(format!(
            "{image_only} page(s) have no text of their own, which is what a scan is, and \
             recognition was not run."
        ));
    }

    let full_text: String = pages
        .iter()
        .filter_map(|page| page["text"].as_str())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let text: String = full_text.chars().take(MAX_TEXT_CHARS).collect();

    if full_text.chars().count() > MAX_TEXT_CHARS {
        warnings.push("PDF text was truncated at the deterministic extraction limit.".to_owned());
    }

    Ok(json!({
        "capability": "document.read",
        "selectedProvider": "local-pdf",
        "resourceLocation": "local",
        "inputResource": path,
        "operationResult": {
            "text": text,
            "pageCount": pages.len(),
            "pages": pages,
            "recognisedPages": recognised,
            "imageOnlyPages": image_only,
            "encrypted": false,
        },
        "warnings": warnings,
        "confirmationConsumed": true,
        "validationResult": "read",
    }))
}

/// Whether this build can read words out of a picture, and what it read.
enum Recognition {
    /// No recogniser on this platform. Not a failure -- everything else still
    /// works, and the caller is told rather than handed a blank page.
    Unavailable,
    Done(Vec<(u32, String)>),
}

#[cfg(not(target_os = "macos"))]
fn recognise(_path: &str, _pages: &[u32]) -> Recognition {
    Recognition::Unavailable
}

/// macOS ships Vision, so a scan can be read there. This is the only
/// platform-specific code in the module, and nothing else depends on it.
#[cfg(target_os = "macos")]
fn recognise(path: &str, pages: &[u32]) -> Recognition {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // Vision runs synchronously through performRequests, which is why
    // recognition fits here at all: no callbacks and no background queue.
    const SCRIPT: &str = r#"
ObjC.import('Quartz');
ObjC.import('AppKit');
ObjC.import('Vision');

function run(argv) {
  const doc = $.PDFDocument.alloc.initWithURL($.NSURL.fileURLWithPath(argv[0]));
  if (!doc.js) return JSON.stringify([]);

  const wanted = argv[1].split(',').map(function (v) { return parseInt(v, 10); });
  const out = [];

  for (const number of wanted) {
    const page = doc.pageAtIndex(number - 1);
    if (!page.js) continue;

    const bounds = page.boundsForBox($.kPDFDisplayBoxMediaBox);
    // Rendered above natural size: recognising a thumbnail measures the
    // thumbnail, not the page.
    const size = $.NSMakeSize(bounds.size.width * 3, bounds.size.height * 3);
    const image = page.thumbnailOfSizeForBox(size, $.kPDFDisplayBoxMediaBox);
    if (!image.js) continue;

    const cg = image.CGImageForProposedRectContextHints($(), $(), $());
    if (!cg) continue;

    const request = $.VNRecognizeTextRequest.alloc.init;
    request.recognitionLevel = 1;
    const handler = $.VNImageRequestHandler.alloc.initWithCGImageOptions(cg, $({}));
    if (!handler.performRequestsError($([request]), $())) continue;

    const lines = [];
    for (let i = 0; i < request.results.count; i++) {
      lines.push(ObjC.unwrap(request.results.objectAtIndex(i).topCandidates(1).objectAtIndex(0).string));
    }
    out.push({ page: number, text: lines.join('\n') });
  }

  return JSON.stringify(out);
}
"#;

    let wanted = pages
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");

    let Ok(mut child) = Command::new("/usr/bin/osascript")
        .args(["-l", "JavaScript", "-", path, &wanted])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Recognition::Unavailable;
    };

    if let Some(stdin) = child.stdin.as_mut() {
        if stdin.write_all(SCRIPT.as_bytes()).is_err() {
            return Recognition::Unavailable;
        }
    }

    let Ok(output) = child.wait_with_output() else {
        return Recognition::Unavailable;
    };

    if !output.status.success() {
        return Recognition::Unavailable;
    }

    let Ok(parsed) = serde_json::from_slice::<Value>(&output.stdout) else {
        return Recognition::Unavailable;
    };

    Recognition::Done(
        parsed
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|entry| {
                Some((
                    u32::try_from(entry["page"].as_u64()?).ok()?,
                    entry["text"].as_str()?.to_owned(),
                ))
            })
            .collect(),
    )
}

/// Build one document out of pages taken from others.
///
/// Merging and extracting are the same operation with different inputs, so they
/// share this: renumber each source so its object ids cannot collide, collect
/// the pages that were asked for, and hang them all under one page tree. Doing
/// it once means a page arrives in the output the same way whichever capability
/// asked for it.
/// The page attributes a PDF lets a page inherit from its parent.
///
/// This is why a naive merge produces a document that LOOKS right and has lost
/// its text: `Resources` -- the fonts a page draws with -- may live on the page
/// tree node rather than on the page. Re-parent that page under a different
/// tree and its fonts are gone, extraction returns nothing, and the failure is
/// silent. Caught here by a merged page reading as empty when both sources read
/// fine on their own.
const INHERITABLE: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// Copy anything a page was inheriting onto the page itself.
///
/// After this the page is self-contained and can be hung under any tree, which
/// is exactly what assembling one document out of several requires.
fn flatten_inherited(document: &Document, page_id: ObjectId, page: &mut lopdf::Dictionary) {
    let mut current = page_id;
    let mut seen = std::collections::HashSet::new();

    loop {
        let Ok(node) = document.get_dictionary(current) else {
            break;
        };

        for key in INHERITABLE {
            if !page.has(key) {
                if let Ok(value) = node.get(key) {
                    page.set(key.to_vec(), value.clone());
                }
            }
        }

        let Ok(parent) = node.get(b"Parent").and_then(Object::as_reference) else {
            break;
        };

        // A malformed file can point a node at its own ancestor.
        if !seen.insert(parent) {
            break;
        }

        current = parent;
    }
}

fn assemble(sources: Vec<(Document, Vec<u32>)>, operation: &str) -> Result<Document, PdfError> {
    let mut next_id = 1u32;
    let mut ordered_pages: Vec<ObjectId> = Vec::new();
    let mut page_objects: BTreeMap<ObjectId, Object> = BTreeMap::new();
    let mut other_objects: BTreeMap<ObjectId, Object> = BTreeMap::new();
    let mut assembled = Document::with_version("1.5");

    for (mut document, wanted) in sources {
        document.renumber_objects_with(next_id);
        next_id = document.max_id + 1;

        let available = document.get_pages();

        for number in wanted {
            let id = available.get(&number).copied().ok_or_else(|| {
                PdfError::invalid(format!(
                    "{operation} was asked for page {number}, which the document does not have"
                ))
            })?;

            let mut page = document
                .get_dictionary(id)
                .map_err(|_| PdfError::execution(format!("page {number} could not be read")))?
                .clone();

            // Before the page leaves its own document, take what it was
            // inheriting with it.
            flatten_inherited(&document, id, &mut page);

            let object = Object::Dictionary(page);

            // A page asked for twice would put the same object in the tree
            // twice. The pages are taken in the order asked for; a repeat is
            // ignored rather than duplicated, because one object cannot be in
            // two places in a page tree.
            if page_objects.insert(id, object).is_none() {
                ordered_pages.push(id);
            }
        }

        other_objects.extend(document.objects);
    }

    let mut catalog: Option<(ObjectId, Object)> = None;
    let mut pages_root: Option<(ObjectId, Object)> = None;

    for (id, object) in other_objects {
        match object.type_name().unwrap_or(b"") {
            b"Catalog" => catalog = Some((catalog.map_or(id, |(id, _)| id), object)),
            b"Pages" => {
                if let Ok(dictionary) = object.as_dict() {
                    let mut dictionary = dictionary.clone();

                    if let Some((_, previous)) = pages_root.as_ref() {
                        if let Ok(previous) = previous.as_dict() {
                            dictionary.extend(previous);
                        }
                    }

                    pages_root = Some((
                        pages_root.map_or(id, |(id, _)| id),
                        Object::Dictionary(dictionary),
                    ));
                }
            }
            // Pages are placed under the single new tree below, and outlines
            // are not carried across because they point into trees that no
            // longer exist.
            b"Page" | b"Outlines" | b"Outline" => {}
            _ => {
                assembled.objects.insert(id, object);
            }
        }
    }

    let (catalog_id, catalog_object) = catalog
        .ok_or_else(|| PdfError::invalid(format!("{operation} found no catalog in the source")))?;
    let (pages_id, pages_object) = pages_root
        .ok_or_else(|| PdfError::invalid(format!("{operation} found no page tree in the source")))?;

    for id in &ordered_pages {
        if let Some(object) = page_objects.get(id) {
            if let Ok(dictionary) = object.as_dict() {
                let mut dictionary = dictionary.clone();
                dictionary.set("Parent", pages_id);
                assembled.objects.insert(*id, Object::Dictionary(dictionary));
            }
        }
    }

    if let Ok(dictionary) = pages_object.as_dict() {
        let mut dictionary = dictionary.clone();
        dictionary.set("Count", ordered_pages.len() as u32);
        dictionary.set(
            "Kids",
            ordered_pages
                .iter()
                .copied()
                .map(Object::Reference)
                .collect::<Vec<_>>(),
        );
        assembled.objects.insert(pages_id, Object::Dictionary(dictionary));
    }

    if let Ok(dictionary) = catalog_object.as_dict() {
        let mut dictionary = dictionary.clone();
        dictionary.set("Pages", pages_id);
        dictionary.remove(b"Outlines");
        assembled.objects.insert(catalog_id, Object::Dictionary(dictionary));
    }

    assembled.trailer.set("Root", catalog_id);
    assembled.max_id = assembled.objects.len() as u32;
    assembled.renumber_objects();
    assembled.adjust_zero_pages();

    Ok(assembled)
}

fn write(document: &mut Document, destination: &str) -> Result<usize, PdfError> {
    document
        .save(destination)
        .map_err(|error| PdfError::execution(format!("the PDF could not be written: {error}")))?;

    Ok(document.get_pages().len())
}

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

    let mut loaded = Vec::new();
    let mut contributed = Vec::new();
    let mut total = 0usize;

    for path in &paths {
        let document = open(path, "document.merge")?;
        let numbers = page_numbers(&document);
        total += numbers.len();

        if total > MAX_PAGES {
            return Err(PdfError::invalid(format!(
                "document.merge would produce more than {MAX_PAGES} pages"
            )));
        }

        contributed.push(json!({"source": path, "pages": numbers.len()}));
        loaded.push((document, numbers));
    }

    let mut assembled = assemble(loaded, "document.merge")?;
    let page_count = write(&mut assembled, destination)?;

    Ok(json!({
        "capability": "document.merge",
        "selectedProvider": "local-pdf",
        "resourceLocation": "local",
        "inputResource": paths,
        "operationResult": {
            "status": "merged",
            "destination": destination,
            "pageCount": page_count,
            "sources": contributed,
        },
        "warnings": [],
        "confirmationConsumed": true,
        "validationResult": "merged",
    }))
}

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

    let wanted = parse_pages(spec)?;
    let document = open(source, "document.split")?;
    let source_page_count = page_numbers(&document).len();

    let numbers: Vec<u32> = wanted
        .iter()
        .map(|page| u32::try_from(*page).unwrap_or(u32::MAX))
        .collect();

    let mut assembled = assemble(vec![(document, numbers)], "document.split")?;
    let page_count = write(&mut assembled, destination)?;

    Ok(json!({
        "capability": "document.split",
        "selectedProvider": "local-pdf",
        "resourceLocation": "local",
        "inputResource": source,
        "operationResult": {
            "status": "split",
            "destination": destination,
            "pages": wanted,
            "pageCount": page_count,
            "sourcePageCount": source_page_count,
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

    /// A one-page PDF that is only a picture of legible words.
    ///
    /// Returns how much text of its own the result has, so the test can assert
    /// that the fixture really is a scan before relying on it being one.
    ///
    /// This is the only macOS-specific thing in these tests, and it is here to
    /// MAKE a scan, not to read one: the reader under test is portable, and
    /// recognition is checked only where the platform provides it.
    #[cfg(target_os = "macos")]
    fn build_scan_fixture(destination: &std::path::Path) -> usize {
        use std::io::Write;
        use std::process::{Command, Stdio};

        const SCRIPT: &str = r#"
ObjC.import('Quartz');
ObjC.import('AppKit');

function run(argv) {
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
  return String((ObjC.unwrap(back.string) || '').trim().length);
}
"#;

        let mut child = Command::new("/usr/bin/osascript")
            .args(["-l", "JavaScript", "-"])
            .arg(destination)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("osascript is part of macOS");

        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(SCRIPT.as_bytes())
            .unwrap();

        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "the scan fixture could not be built");

        String::from_utf8_lossy(&output.stdout).trim().parse().unwrap()
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
            let produced = std::process::Command::new("/usr/sbin/cupsfilter")
                .arg(source)
                .output()
                .expect("cupsfilter is part of macOS");
            assert!(produced.status.success(), "cupsfilter refused the fixture");
            std::fs::write(target, produced.stdout).unwrap();
        }

        // The scan: a page that is only a picture of legible words, which is
        // what a scanned document is. Built here rather than checked in, and it
        // asserts its own premise below before the test relies on it.
        let scan_text_layer = build_scan_fixture(&scan_pdf);

        // The scan must genuinely have no text of its own, or the recognition
        // assertion below would be proving nothing.
        assert_eq!(scan_text_layer, 0, "the scan fixture is not a scan");

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

        assert_eq!(read["selectedProvider"], "local-pdf");
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
