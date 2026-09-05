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

use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::{
    Document, EncryptionState, EncryptionVersion, Object, ObjectId, Permissions, StringFormat,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use uuid::Uuid;

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
///
/// A password is used if one is supplied and never appears in the result or in
/// an error: it is the caller's secret, and a message that quotes it back would
/// put it wherever that message ends up.
fn open_with(path: &str, operation: &str, password: Option<&str>) -> Result<Document, PdfError> {
    // The password has to go in at LOAD time. An encrypted PDF hands back an
    // empty shell to a reader that has no password -- not encrypted objects
    // waiting to be unlocked afterwards -- so decrypting a document that was
    // already loaded without one produces a document with nothing in it, and
    // it does so without complaining. That silent emptiness is exactly the
    // kind of failure this module must not have.
    let loaded = match password {
        Some(password) => Document::load_with_password(path, password),
        None => Document::load(path),
    };

    let mut document = loaded.map_err(|error| {
        // A wrong password comes back from the loader as a load failure. Said
        // as "not a readable PDF" it would send the caller looking at the file
        // when the file is fine.
        if password.is_some() && declares_encryption(path) {
            PdfError::invalid(format!(
                "{operation} could not open {path}: the password did not match"
            ))
        } else {
            PdfError::invalid(format!("{path} is not a readable PDF: {error}"))
        }
    })?;

    if document.is_encrypted() {
        let Some(password) = password else {
            return Err(PdfError::invalid(format!(
                "{operation} cannot read {path}: it is encrypted and no password was given"
            )));
        };

        // Asked explicitly, because a password that does not open the file
        // leaves a document that merely LOOKS empty.
        document.authenticate_password(password).map_err(|_| {
            PdfError::invalid(format!(
                "{operation} could not open {path}: the password did not match"
            ))
        })?;

        drop_encryption(&mut document);
    }

    Ok(document)
}

/// Forget that a document was ever encrypted.
///
/// Its contents are in the clear in memory once the password has opened it, so
/// the `/Encrypt` entry left in the trailer describes a state the document is
/// no longer in. Written out as-is it would produce a file that claims to be
/// encrypted and is not, which no reader can open.
fn drop_encryption(document: &mut Document) {
    let reference = document
        .trailer
        .remove(b"Encrypt")
        .and_then(|value| value.as_reference().ok());

    if let Some(id) = reference {
        document.objects.remove(&id);
    }
}

/// Whether a file on disk announces itself as encrypted, without reading it all.
fn declares_encryption(path: &str) -> bool {
    lopdf::Document::load_metadata(path)
        .map(|metadata| metadata.encrypted)
        .unwrap_or(false)
}

fn open(path: &str, operation: &str) -> Result<Document, PdfError> {
    open_with(path, operation, None)
}

/// The password a request carries, if any.
///
/// Trimmed of nothing: a password's spaces are part of it.
fn password_of<'a>(input: &'a Value, field: &str) -> Option<&'a str> {
    input
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

fn page_numbers(document: &Document) -> Vec<u32> {
    document.get_pages().keys().copied().collect()
}

pub(crate) fn read_pdf_document(input: &Value) -> Result<Value, PdfError> {
    let path = existing_pdf(input, "path", "document.read")?;
    let document = open_with(path, "document.read", password_of(input, "password"))?;

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

    // What is written ON the document as well as in it. A note somebody left in
    // the margin and a value somebody typed into a field are both part of
    // reading the document, and returning the running text alone loses them.
    let annotations = annotations_of(&document);
    let fields = reported_fields(&document);

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
            "annotations": annotations,
            "formFields": fields,
            // The FILE, not the copy held here: a password opened it, and
            // saying it was never locked would be untrue about what is on disk.
            "encrypted": declares_encryption(path),
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
    // `/Size` is the one number in the trailer a reader checks against what it
    // actually finds, and removing an object (the encryption dictionary, say)
    // leaves the old count behind. The writer takes it from `max_id`, so that
    // is what has to be told the truth -- otherwise every reader opening the
    // file warns about a count that does not match.
    document.max_id = document.objects.keys().map(|(id, _)| *id).max().unwrap_or(0);

    document
        .save(destination)
        .map_err(|error| PdfError::execution(format!("the PDF could not be written: {error}")))?;

    Ok(document.get_pages().len())
}

/// Turn pages, clockwise, by a quarter turn at a time.
///
/// The turn is RELATIVE to how the page already sits, because that is what
/// "rotate this 90 degrees" means to a person looking at it. A page that was
/// already sideways ends up upside down, not sideways again.
pub(crate) fn rotate_pdf_document(input: &Value) -> Result<Value, PdfError> {
    let source = existing_pdf(input, "source", "document.rotate")?;
    let destination = new_pdf(input, "destination", "document.rotate")?;

    if source == destination {
        return Err(PdfError::invalid(
            "document.rotate source and destination must differ",
        ));
    }

    let degrees = input
        .get("degrees")
        .and_then(Value::as_i64)
        .ok_or_else(|| PdfError::invalid("document.rotate requires degrees"))?;

    if degrees % 90 != 0 {
        return Err(PdfError::invalid(
            "document.rotate turns pages in quarter turns: degrees must be a multiple of 90",
        ));
    }

    let mut document = open_with(source, "document.rotate", password_of(input, "password"))?;

    // No pages named means all of them, which is what "rotate this document"
    // means.
    let wanted: Vec<u32> = match input.get("pages").and_then(Value::as_str) {
        Some(spec) if !spec.trim().is_empty() => parse_pages(spec, "document.rotate")?
            .into_iter()
            .map(|page| u32::try_from(page).unwrap_or(u32::MAX))
            .collect(),
        _ => page_numbers(&document),
    };

    let pages = document.get_pages();
    let mut turned = Vec::new();

    for number in &wanted {
        let id = pages.get(number).copied().ok_or_else(|| {
            PdfError::invalid(format!(
                "document.rotate was asked for page {number}, which the document does not have"
            ))
        })?;

        let current = document
            .get_dictionary(id)
            .ok()
            .and_then(|page| page.get(b"Rotate").ok())
            .and_then(|value| value.as_i64().ok())
            .unwrap_or(0);

        // PDF stores rotation as 0, 90, 180 or 270, so the sum is brought back
        // into that range -- including for a negative turn.
        let next = (((current + degrees) % 360) + 360) % 360;

        if let Ok(page) = document.get_object_mut(id) {
            if let Ok(dictionary) = page.as_dict_mut() {
                dictionary.set("Rotate", next);
                turned.push(json!({"page": number, "rotation": next}));
            }
        }
    }

    let page_count = write(&mut document, destination)?;

    Ok(json!({
        "capability": "document.rotate",
        "selectedProvider": "local-pdf",
        "resourceLocation": "local",
        "inputResource": source,
        "operationResult": {
            "status": "rotated",
            "destination": destination,
            "degrees": degrees,
            "pages": turned,
            "pageCount": page_count,
        },
        "warnings": [],
        "confirmationConsumed": true,
        "validationResult": "rotated",
    }))
}

/// Give a document a file identifier if it has none.
///
/// The identifier is stirred into the encryption key, so a file without one
/// cannot be encrypted at all -- and plenty of files written by simple tools
/// have none. Refusing those would be a failure the person could do nothing
/// about, so one is made here instead. It identifies the file; it is not a
/// secret, and the format asks for exactly this.
fn ensure_file_id(document: &mut Document) {
    let present = document
        .trailer
        .get(b"ID")
        .ok()
        .and_then(|value| value.as_array().ok())
        .is_some_and(|identifiers| !identifiers.is_empty());

    if present {
        return;
    }

    let identifier = || Object::String(Uuid::new_v4().as_bytes().to_vec(), StringFormat::Hexadecimal);

    document
        .trailer
        .set("ID", Object::Array(vec![identifier(), identifier()]));
}

pub(crate) fn encrypt_pdf_document(input: &Value) -> Result<Value, PdfError> {
    let source = existing_pdf(input, "source", "document.encrypt")?;
    let destination = new_pdf(input, "destination", "document.encrypt")?;

    if source == destination {
        return Err(PdfError::invalid(
            "document.encrypt source and destination must differ",
        ));
    }

    let user_password = password_of(input, "password")
        .ok_or_else(|| PdfError::invalid("document.encrypt requires password"))?;

    // The owner password governs permissions rather than opening. When the
    // caller does not set one, it is the same as the open password, which is
    // what most tools do and what a caller who gave one password means.
    let owner_password = password_of(input, "ownerPassword").unwrap_or(user_password);

    // An already-encrypted source is not refused: opening it with its own
    // password and writing it out under a new one is how a password gets
    // changed, and refusing that would send the caller through a decrypted
    // copy on disk, which is worse.
    let mut document = open_with(source, "document.encrypt", password_of(input, "sourcePassword"))?;

    ensure_file_id(&mut document);

    // AES-128 through the standard security handler: widely readable, and it
    // needs no random file key of its own, so nothing here depends on a
    // generator this crate would have to be trusted with.
    let filter: std::sync::Arc<dyn CryptFilter> = std::sync::Arc::new(Aes128CryptFilter);

    let version = EncryptionVersion::V4 {
        document: &document,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password,
        user_password,
        // The password is the protection. Printing and copying are left alone
        // rather than quietly restricted, because a caller who wanted that
        // would have asked for it.
        permissions: Permissions::PRINTABLE
            | Permissions::COPYABLE
            | Permissions::COPYABLE_FOR_ACCESSIBILITY
            | Permissions::PRINTABLE_IN_HIGH_QUALITY,
    };

    let state = EncryptionState::try_from(version).map_err(|error| {
        PdfError::execution(format!("the encryption settings were refused: {error}"))
    })?;

    document
        .encrypt(&state)
        .map_err(|_| PdfError::execution("the PDF could not be encrypted"))?;

    let page_count = write(&mut document, destination)?;

    Ok(json!({
        "capability": "document.encrypt",
        "selectedProvider": "local-pdf",
        "resourceLocation": "local",
        "inputResource": source,
        "operationResult": {
            "status": "encrypted",
            "destination": destination,
            "pageCount": page_count,
            "algorithm": "AES-128",
        },
        // Deliberately no echo of the password anywhere in this result.
        "warnings": [],
        "confirmationConsumed": true,
        "validationResult": "encrypted",
    }))
}

pub(crate) fn decrypt_pdf_document(input: &Value) -> Result<Value, PdfError> {
    let source = existing_pdf(input, "source", "document.decrypt")?;
    let destination = new_pdf(input, "destination", "document.decrypt")?;

    if source == destination {
        return Err(PdfError::invalid(
            "document.decrypt source and destination must differ",
        ));
    }

    let password = password_of(input, "password")
        .ok_or_else(|| PdfError::invalid("document.decrypt requires password"))?;

    if !declares_encryption(source) {
        return Err(PdfError::invalid(format!(
            "document.decrypt was given {source}, which is not encrypted"
        )));
    }

    let mut document = open_with(source, "document.decrypt", Some(password))?;
    let page_count = write(&mut document, destination)?;

    Ok(json!({
        "capability": "document.decrypt",
        "selectedProvider": "local-pdf",
        "resourceLocation": "local",
        "inputResource": source,
        "operationResult": {
            "status": "decrypted",
            "destination": destination,
            "pageCount": page_count,
        },
        "warnings": [],
        "confirmationConsumed": true,
        "validationResult": "decrypted",
    }))
}

/// Replacing more than this in one pass is a rewrite, not an edit.
const MAX_REPLACEMENTS: usize = 200;

/// What to do when the new words are wider than the ones they replace.
///
/// A PDF does not reflow. Text sits at fixed places, so longer text has to
/// take that space from somewhere, and there is no answer that is right for
/// every document -- which is why this is the caller's choice and the default
/// is to do nothing and say so.
#[derive(Clone, Copy, PartialEq)]
enum WhenLonger {
    /// Refuse, and say by how much it did not fit.
    Refuse,
    /// Draw the replacement smaller so it occupies exactly the old space.
    Shrink,
    /// Let it be wider, and let the rest of the line move right.
    Push,
}

/// Change words on a page, in place.
///
/// This is NOT a word processor: a PDF has no paragraphs to reflow, so what
/// happens here is that specific glyphs are taken out and other glyphs are put
/// where they were. That makes it right for the things people actually need --
/// a date, a name, a number, a wrong figure -- and wrong for rewriting a
/// paragraph, which needs the page laid out again.
///
/// The replacement is drawn with the SAME font the old words used, so it looks
/// like the document rather than like a patch. That font is usually a subset
/// carrying only the glyphs the document already used, so a character it does
/// not have cannot be drawn at all -- and that is refused by name rather than
/// silently dropped.
pub(crate) fn replace_in_pdf_document(input: &Value) -> Result<Value, PdfError> {
    use crate::document::pdf_layout::{metrics_of, text_runs, writable_codes};
    use lopdf::content::{Content, Operation};

    let source = existing_pdf(input, "source", "document.replace")?;
    let destination = new_pdf(input, "destination", "document.replace")?;

    if source == destination {
        return Err(PdfError::invalid(
            "document.replace source and destination must differ",
        ));
    }

    let requested = input
        .get("replacements")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .ok_or_else(|| PdfError::invalid("document.replace requires replacements"))?;

    if requested.len() > MAX_REPLACEMENTS {
        return Err(PdfError::invalid(format!(
            "document.replace accepts at most {MAX_REPLACEMENTS} replacements at once"
        )));
    }

    let when_longer = match input.get("whenLonger").and_then(Value::as_str) {
        None | Some("refuse") => WhenLonger::Refuse,
        Some("shrink") => WhenLonger::Shrink,
        Some("push") => WhenLonger::Push,
        Some(other) => {
            return Err(PdfError::invalid(format!(
                "whenLonger is refuse, shrink or push -- not {other:?}"
            )));
        }
    };

    let mut wanted: Vec<(String, String, Option<Vec<u32>>)> = Vec::new();

    for item in requested {
        let find = item
            .get("find")
            .and_then(Value::as_str)
            .filter(|find| !find.is_empty())
            .ok_or_else(|| PdfError::invalid("every replacement needs find"))?;

        let with = item
            .get("with")
            .and_then(Value::as_str)
            .ok_or_else(|| PdfError::invalid("every replacement needs with"))?;

        let pages = match item.get("pages").and_then(Value::as_str) {
            Some(spec) if !spec.trim().is_empty() => Some(
                parse_pages(spec, "document.replace")?
                    .into_iter()
                    .map(|page| u32::try_from(page).unwrap_or(u32::MAX))
                    .collect(),
            ),
            _ => None,
        };

        wanted.push((find.to_owned(), with.to_owned(), pages));
    }

    let mut document = open_with(source, "document.replace", password_of(input, "password"))?;
    let pages = document.get_pages();
    let mut changed = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for (number, page_id) in pages {
        let runs = text_runs(&document, page_id);

        // What each font on this page can spell, and how wide it draws things.
        let fonts = document.get_page_fonts(page_id).unwrap_or_default();
        let spellable: BTreeMap<Vec<u8>, BTreeMap<String, Vec<u8>>> = fonts
            .iter()
            .map(|(name, font)| (name.clone(), writable_codes(&document, font)))
            .collect();
        let measures: BTreeMap<Vec<u8>, _> = fonts
            .iter()
            .map(|(name, font)| (name.clone(), metrics_of(&document, font)))
            .collect();

        // operation -> the edits inside it, as (first glyph, last glyph, new codes, new size)
        let mut edits: BTreeMap<usize, Vec<(usize, usize, Vec<u8>, f64)>> = BTreeMap::new();

        for run in &runs {
            for (find, with, only) in &wanted {
                if let Some(only) = only {
                    if !only.contains(&number) {
                        continue;
                    }
                }

                let spans = run.find_all(find);

                if spans.is_empty() {
                    continue;
                }

                if !run.measured {
                    return Err(PdfError::invalid(format!(
                        "document.replace cannot rewrite page {number}: its font declares no \
                         glyph widths, so nothing could be put back in the right place"
                    )));
                }

                let (Some(codes), Some(metric)) =
                    (spellable.get(&run.font), measures.get(&run.font))
                else {
                    return Err(PdfError::execution(format!(
                        "page {number} uses a font this reader could not open"
                    )));
                };

                // The replacement is drawn in the font the old words used. An
                // embedded subset carries only what the document already used,
                // so a missing character is refused by name -- drawing it as a
                // blank box is the failure this check exists to prevent.
                let mut encoded = Vec::new();

                for letter in with.chars() {
                    let Some(code) = codes.get(&letter.to_string()) else {
                        return Err(PdfError::invalid(format!(
                            "document.replace cannot write {letter:?} on page {number}: the font \
                             those words are set in does not carry that character"
                        )));
                    };

                    encoded.extend_from_slice(code);
                }

                for (first, last) in spans {
                    let old_advance: f64 = run.glyphs[first..last]
                        .iter()
                        .map(|glyph| glyph.advance)
                        .sum();

                    let new_advance = metric.advance(
                        &encoded,
                        run.size,
                        run.char_spacing,
                        run.word_spacing,
                        run.horizontal,
                    );

                    let mut size = run.size;

                    if new_advance > old_advance + 0.01 {
                        match when_longer {
                            WhenLonger::Refuse => {
                                return Err(PdfError::invalid(format!(
                                    "document.replace will not put {with:?} where {find:?} was on \
                                     page {number}: it is {:.1} points wider than the space it \
                                     has. A PDF does not reflow, so ask for whenLonger \"shrink\" \
                                     to fit it, or \"push\" to move the rest of the line.",
                                    new_advance - old_advance
                                )));
                            }
                            WhenLonger::Shrink => {
                                // Exactly the old width, by drawing smaller.
                                size = run.size * (old_advance / new_advance.max(f64::EPSILON));
                                warnings.push(format!(
                                    "{with:?} was drawn smaller on page {number} so it would fit \
                                     the space {find:?} occupied."
                                ));
                            }
                            WhenLonger::Push => {
                                warnings.push(format!(
                                    "{with:?} is wider than {find:?} on page {number}, so the rest \
                                     of that line moved right."
                                ));
                            }
                        }
                    }

                    edits
                        .entry(run.operation)
                        .or_default()
                        .push((first, last, encoded.clone(), size));

                    changed.push(json!({"page": number, "from": find, "to": with}));
                }
            }
        }

        if edits.is_empty() {
            continue;
        }

        let Ok(content) = document.get_and_decode_page_content(page_id) else {
            return Err(PdfError::execution(format!(
                "page {number} could not be read to change it"
            )));
        };

        let mut rebuilt: Vec<Operation> = vec![Operation::new("q", vec![])];

        for (index, operation) in content.operations.iter().enumerate() {
            let (Some(here), Some(run)) = (
                edits.get(&index),
                runs.iter().find(|run| run.operation == index),
            ) else {
                rebuilt.push(operation.clone());
                continue;
            };

            match operation.operator.as_str() {
                "'" => rebuilt.push(Operation::new("T*", vec![])),
                "\"" => {
                    if let Some(spacing) = operation.operands.first() {
                        rebuilt.push(Operation::new("Tw", vec![spacing.clone()]));
                    }
                    if let Some(spacing) = operation.operands.get(1) {
                        rebuilt.push(Operation::new("Tc", vec![spacing.clone()]));
                    }
                    rebuilt.push(Operation::new("T*", vec![]));
                }
                _ => {}
            }

            let scaled = run.size * run.horizontal;
            let mut array: Vec<Object> = Vec::new();
            let mut kept: Vec<u8> = Vec::new();
            let mut position = 0usize;

            let flush = |array: &mut Vec<Object>, kept: &mut Vec<u8>| {
                if !kept.is_empty() {
                    array.push(Object::String(
                        std::mem::take(kept),
                        lopdf::StringFormat::Literal,
                    ));
                }
            };

            let emit = |rebuilt: &mut Vec<Operation>, array: &mut Vec<Object>| {
                if !array.is_empty() {
                    rebuilt.push(Operation::new("TJ", vec![Object::Array(std::mem::take(array))]));
                }
            };

            while position < run.glyphs.len() {
                if let Some((first, last, encoded, size)) =
                    here.iter().find(|(first, ..)| *first == position)
                {
                    flush(&mut array, &mut kept);

                    let old_advance: f64 = run.glyphs[*first..*last]
                        .iter()
                        .map(|glyph| glyph.advance)
                        .sum();

                    // Drawing at a different size needs its own instruction,
                    // and an instruction ends the run of text. A reader then
                    // sees two runs with a gap and puts a SPACE between them,
                    // so the page reads "Invoice  2026" and a search for
                    // "Invoice 2026" no longer finds it. At the same size --
                    // which is every replacement that fits -- the new letters
                    // go into the run that is already being built, and the
                    // text layer stays one continuous line.
                    let resized = (size - run.size).abs() > f64::EPSILON;

                    if resized {
                        emit(&mut rebuilt, &mut array);

                        rebuilt.push(Operation::new(
                            "Tf",
                            vec![
                                Object::Name(run.font.clone()),
                                Object::Real(*size as f32),
                            ],
                        ));

                        rebuilt.push(Operation::new(
                            "TJ",
                            vec![Object::Array(vec![Object::String(
                                encoded.clone(),
                                lopdf::StringFormat::Literal,
                            )])],
                        ));

                        rebuilt.push(Operation::new(
                            "Tf",
                            vec![
                                Object::Name(run.font.clone()),
                                Object::Real(run.size as f32),
                            ],
                        ));
                    } else {
                        array.push(Object::String(
                            encoded.clone(),
                            lopdf::StringFormat::Literal,
                        ));
                    }

                    let Some(metric) = measures.get(&run.font) else {
                        return Err(PdfError::execution(format!(
                            "page {number} lost track of its own font"
                        )));
                    };

                    let new_advance = metric.advance(
                        encoded,
                        *size,
                        run.char_spacing,
                        run.word_spacing,
                        run.horizontal,
                    );

                    // Take back exactly the difference, so every letter after
                    // this one stays where the document put it. When the caller
                    // asked to push, the difference is left in place instead.
                    let leftover = old_advance - new_advance;

                    if when_longer != WhenLonger::Push || leftover > 0.0 {
                        if scaled.abs() > f64::EPSILON && leftover.abs() > 0.001 {
                            array.push(Object::Real((-leftover * 1000.0 / scaled) as f32));
                        }
                    }

                    position = *last;
                    continue;
                }

                let glyph = &run.glyphs[position];

                if glyph.kern_before != 0.0 {
                    flush(&mut array, &mut kept);
                    array.push(Object::Real(glyph.kern_before as f32));
                }

                kept.extend_from_slice(&glyph.codes);
                position += 1;
            }

            flush(&mut array, &mut kept);
            emit(&mut rebuilt, &mut array);
        }

        rebuilt.push(Operation::new("Q", vec![]));

        let encoded = Content { operations: rebuilt }
            .encode()
            .map_err(|error| {
                PdfError::execution(format!("page {number} could not be rewritten: {error}"))
            })?;

        document
            .change_page_content(page_id, encoded)
            .map_err(|error| {
                PdfError::execution(format!("page {number} could not be replaced: {error}"))
            })?;
    }

    if changed.is_empty() {
        return Err(PdfError::invalid(
            "document.replace found none of the words it was asked to change; nothing was written",
        ));
    }

    let page_count = write(&mut document, destination)?;

    Ok(json!({
        "capability": "document.replace",
        "selectedProvider": "local-pdf",
        "resourceLocation": "local",
        "inputResource": source,
        "operationResult": {
            "status": "replaced",
            "destination": destination,
            "pageCount": page_count,
            "replaced": changed,
        },
        "warnings": warnings,
        "confirmationConsumed": true,
        "validationResult": "replaced",
    }))
}

/// More marks than this on one page is not a stamp, it is a second document.
const MAX_STAMPS: usize = 100;

/// One thing to put on a page.
struct Stamp {
    pages: Option<Vec<u32>>,
    what: StampContent,
    x: f64,
    y: f64,
    size: f64,
    width: Option<f64>,
    height: Option<f64>,
    colour: [f64; 3],
    opacity: f64,
    rotate: f64,
}

enum StampContent {
    Words(String),
    Picture(String),
}

fn stamp_number(value: Option<&Value>, fallback: f64) -> f64 {
    value.and_then(Value::as_f64).unwrap_or(fallback)
}

/// Put something on top of a page: a watermark, a page number, a signature.
///
/// This adds to a page rather than changing it, which is why it is separate
/// from redaction and replacement -- nothing underneath is touched, so nothing
/// underneath can be broken.
///
/// Words are drawn with a font the page ALREADY has whenever one of them can
/// spell them, so the stamp matches the document and needs nothing embedded.
/// Where no such font exists, plain Latin falls back to Helvetica, which every
/// reader has. Anything else is refused, naming the character that cannot be
/// drawn, because a stamp that comes out as blank boxes is worse than one that
/// was not applied.
pub(crate) fn stamp_pdf_document(input: &Value) -> Result<Value, PdfError> {
    use crate::document::pdf_layout::writable_codes;
    use lopdf::content::{Content, Operation};
    use lopdf::dictionary;

    let source = existing_pdf(input, "source", "document.stamp")?;
    let destination = new_pdf(input, "destination", "document.stamp")?;

    if source == destination {
        return Err(PdfError::invalid(
            "document.stamp source and destination must differ",
        ));
    }

    let requested = input
        .get("stamps")
        .and_then(Value::as_array)
        .filter(|stamps| !stamps.is_empty())
        .ok_or_else(|| PdfError::invalid("document.stamp requires stamps"))?;

    if requested.len() > MAX_STAMPS {
        return Err(PdfError::invalid(format!(
            "document.stamp accepts at most {MAX_STAMPS} stamps at once"
        )));
    }

    let mut stamps = Vec::new();

    for stamp in requested {
        let words = stamp.get("text").and_then(Value::as_str);
        let picture = stamp.get("image").and_then(Value::as_str);

        let what = match (words, picture) {
            (Some(words), None) if !words.is_empty() => StampContent::Words(words.to_owned()),
            (None, Some(picture)) => {
                if !Path::new(picture).is_absolute() {
                    return Err(PdfError::invalid(format!(
                        "document.stamp needs a full path to the image, not {picture}"
                    )));
                }

                if !Path::new(picture).is_file() {
                    return Err(PdfError::invalid(format!(
                        "document.stamp cannot find the image {picture}"
                    )));
                }

                StampContent::Picture(picture.to_owned())
            }
            (Some(_), Some(_)) => {
                return Err(PdfError::invalid(
                    "a stamp is either words or a picture, not both",
                ));
            }
            _ => {
                return Err(PdfError::invalid("every stamp needs text or an image"));
            }
        };

        let pages = match stamp.get("pages").and_then(Value::as_str) {
            Some(spec) if !spec.trim().is_empty() => Some(
                parse_pages(spec, "document.stamp")?
                    .into_iter()
                    .map(|page| u32::try_from(page).unwrap_or(u32::MAX))
                    .collect(),
            ),
            _ => None,
        };

        let colour = stamp
            .get("color")
            .and_then(Value::as_array)
            .map(|values| values.iter().filter_map(Value::as_f64).collect::<Vec<f64>>())
            .filter(|values| values.len() == 3)
            .map(|values| [values[0], values[1], values[2]])
            // Mid grey, because a watermark that is black competes with the
            // document it is marking.
            .unwrap_or([0.5, 0.5, 0.5]);

        stamps.push(Stamp {
            pages,
            what,
            x: stamp_number(stamp.get("x"), 72.0),
            y: stamp_number(stamp.get("y"), 72.0),
            size: stamp_number(stamp.get("size"), 24.0),
            width: stamp.get("width").and_then(Value::as_f64),
            height: stamp.get("height").and_then(Value::as_f64),
            colour,
            opacity: stamp_number(stamp.get("opacity"), 1.0).clamp(0.0, 1.0),
            rotate: stamp_number(stamp.get("rotate"), 0.0),
        });
    }

    let mut document = open_with(source, "document.stamp", password_of(input, "password"))?;
    let pages = document.get_pages();
    let mut applied = Vec::new();

    for (number, page_id) in pages {
        let mine: Vec<&Stamp> = stamps
            .iter()
            .filter(|stamp| {
                stamp
                    .pages
                    .as_ref()
                    .map(|pages| pages.contains(&number))
                    .unwrap_or(true)
            })
            .collect();

        if mine.is_empty() {
            continue;
        }

        // What this page can already spell, and with which font.
        let spellable: Vec<(Vec<u8>, BTreeMap<String, Vec<u8>>)> = document
            .get_page_fonts(page_id)
            .unwrap_or_default()
            .into_iter()
            .map(|(name, font)| (name, writable_codes(&document, font)))
            .collect();

        let mut additions: Vec<Operation> = Vec::new();
        let mut resources: Vec<(&str, Vec<u8>, ObjectId)> = Vec::new();

        for stamp in mine {
            let angle = stamp.rotate.to_radians();
            let (sin, cos) = (angle.sin(), angle.cos());

            additions.push(Operation::new("q", vec![]));

            if stamp.opacity < 1.0 {
                let state = document.add_object(Object::Dictionary(dictionary! {
                    "Type" => "ExtGState",
                    "ca" => Object::Real(stamp.opacity as f32),
                    "CA" => Object::Real(stamp.opacity as f32),
                }));
                let name = format!("AiosGS{}", state.0).into_bytes();
                additions.push(Operation::new(
                    "gs",
                    vec![Object::Name(name.clone())],
                ));
                resources.push(("ExtGState", name, state));
            }

            match &stamp.what {
                StampContent::Words(words) => {
                    // A font the page already carries is preferred: it matches
                    // the document, and it needs nothing embedded.
                    let chosen = spellable.iter().find(|(_, codes)| {
                        words
                            .chars()
                            .all(|letter| codes.contains_key(&letter.to_string()))
                    });

                    let (font_name, encoded) = match chosen {
                        Some((name, codes)) => {
                            let mut encoded = Vec::new();
                            for letter in words.chars() {
                                encoded.extend_from_slice(&codes[&letter.to_string()]);
                            }
                            (name.clone(), encoded)
                        }
                        None => {
                            // Helvetica is in every reader, and covers Latin.
                            // Anything it cannot spell is refused by name.
                            if let Some(letter) =
                                words.chars().find(|letter| !letter.is_ascii_graphic() && *letter != ' ')
                            {
                                return Err(PdfError::invalid(format!(
                                    "document.stamp cannot draw {letter:?} on page {number}: no \
                                     font on that page can spell it, and the fallback font covers \
                                     only Latin"
                                )));
                            }

                            let helvetica = document.add_object(Object::Dictionary(
                                dictionary! {
                                    "Type" => "Font",
                                    "Subtype" => "Type1",
                                    "BaseFont" => "Helvetica",
                                    "Encoding" => "WinAnsiEncoding",
                                },
                            ));

                            let name = format!("AiosF{}", helvetica.0).into_bytes();
                            resources.push(("Font", name.clone(), helvetica));
                            (name, words.as_bytes().to_vec())
                        }
                    };

                    additions.push(Operation::new(
                        "rg",
                        vec![
                            Object::Real(stamp.colour[0] as f32),
                            Object::Real(stamp.colour[1] as f32),
                            Object::Real(stamp.colour[2] as f32),
                        ],
                    ));
                    additions.push(Operation::new("BT", vec![]));
                    additions.push(Operation::new(
                        "Tf",
                        vec![
                            Object::Name(font_name),
                            Object::Real(stamp.size as f32),
                        ],
                    ));
                    // Position and turn in one matrix, so a diagonal watermark
                    // is one instruction rather than a stack of them.
                    additions.push(Operation::new(
                        "Tm",
                        vec![
                            Object::Real(cos as f32),
                            Object::Real(sin as f32),
                            Object::Real(-sin as f32),
                            Object::Real(cos as f32),
                            Object::Real(stamp.x as f32),
                            Object::Real(stamp.y as f32),
                        ],
                    ));
                    additions.push(Operation::new(
                        "Tj",
                        vec![Object::String(encoded, lopdf::StringFormat::Literal)],
                    ));
                    additions.push(Operation::new("ET", vec![]));
                }
                StampContent::Picture(picture) => {
                    let image = lopdf::xobject::image(picture).map_err(|error| {
                        PdfError::invalid(format!("{picture} could not be read as an image: {error}"))
                    })?;

                    let natural = |key: &[u8]| {
                        image
                            .dict
                            .get(key)
                            .ok()
                            .and_then(|value| value.as_i64().ok())
                            .unwrap_or(1) as f64
                    };

                    // Given one side, the other follows the picture's own
                    // shape. Given neither, it is drawn at its own size.
                    let (natural_width, natural_height) = (natural(b"Width"), natural(b"Height"));
                    let ratio = natural_height / natural_width.max(1.0);

                    let (width, height) = match (stamp.width, stamp.height) {
                        (Some(width), Some(height)) => (width, height),
                        (Some(width), None) => (width, width * ratio),
                        (None, Some(height)) => (height / ratio.max(f64::EPSILON), height),
                        (None, None) => (natural_width, natural_height),
                    };

                    let drawn = document.add_object(image);
                    let name = format!("AiosX{}", drawn.0).into_bytes();
                    resources.push(("XObject", name.clone(), drawn));

                    additions.push(Operation::new(
                        "cm",
                        vec![
                            Object::Real((width * cos) as f32),
                            Object::Real((width * sin) as f32),
                            Object::Real((-height * sin) as f32),
                            Object::Real((height * cos) as f32),
                            Object::Real(stamp.x as f32),
                            Object::Real(stamp.y as f32),
                        ],
                    ));
                    additions.push(Operation::new("Do", vec![Object::Name(name)]));
                }
            }

            additions.push(Operation::new("Q", vec![]));

            applied.push(json!({
                "page": number,
                "what": match &stamp.what {
                    StampContent::Words(words) => words.clone(),
                    StampContent::Picture(picture) => picture.clone(),
                },
            }));
        }

        for (kind, name, id) in resources {
            let added = match kind {
                "Font" => {
                    // There is no add_font-by-name, so the page's own font
                    // dictionary is reached the same way the others are.
                    add_page_resource(&mut document, page_id, b"Font", &name, id)
                }
                "XObject" => document.add_xobject(page_id, name.clone(), id).map_err(|_| ()),
                _ => document
                    .add_graphics_state(page_id, name.clone(), id)
                    .map_err(|_| ()),
            };

            added.map_err(|_| {
                PdfError::execution(format!("page {number} would not take the stamp's resources"))
            })?;
        }

        let Ok(content) = document.get_and_decode_page_content(page_id) else {
            return Err(PdfError::execution(format!(
                "page {number} could not be read to stamp it"
            )));
        };

        // Wrapped, so the stamp is placed in page coordinates whatever state
        // the page's own drawing finished in.
        let mut rebuilt = vec![Operation::new("q", vec![])];
        rebuilt.extend(content.operations);
        rebuilt.push(Operation::new("Q", vec![]));
        rebuilt.extend(additions);

        let encoded = Content { operations: rebuilt }
            .encode()
            .map_err(|error| PdfError::execution(format!("page {number} could not be rewritten: {error}")))?;

        document
            .change_page_content(page_id, encoded)
            .map_err(|error| PdfError::execution(format!("page {number} could not be replaced: {error}")))?;
    }

    if applied.is_empty() {
        return Err(PdfError::invalid(
            "document.stamp put nothing anywhere; check the pages asked for",
        ));
    }

    let page_count = write(&mut document, destination)?;

    Ok(json!({
        "capability": "document.stamp",
        "selectedProvider": "local-pdf",
        "resourceLocation": "local",
        "inputResource": source,
        "operationResult": {
            "status": "stamped",
            "destination": destination,
            "pageCount": page_count,
            "stamps": applied,
        },
        "warnings": [],
        "confirmationConsumed": true,
        "validationResult": "stamped",
    }))
}

/// Add one named entry to a page's resource dictionary of the given kind.
fn add_page_resource(
    document: &mut Document,
    page_id: ObjectId,
    kind: &[u8],
    name: &[u8],
    id: ObjectId,
) -> Result<(), ()> {
    let resources = document
        .get_or_create_resources(page_id)
        .and_then(Object::as_dict_mut)
        .map_err(|_| ())?;

    if !resources.has(kind) {
        resources.set(kind, lopdf::Dictionary::new());
    }

    // The entry can be the dictionary or a pointer to it, and writing to the
    // wrong one loses the resource without complaining.
    let indirect = resources
        .get(kind)
        .ok()
        .and_then(|value| value.as_reference().ok());

    match indirect {
        Some(target) => document
            .get_object_mut(target)
            .and_then(Object::as_dict_mut)
            .map_err(|_| ())?
            .set(name.to_vec(), Object::Reference(id)),
        None => document
            .get_or_create_resources(page_id)
            .and_then(Object::as_dict_mut)
            .map_err(|_| ())?
            .get_mut(kind)
            .and_then(Object::as_dict_mut)
            .map_err(|_| ())?
            .set(name.to_vec(), Object::Reference(id)),
    }

    Ok(())
}

/// A page cannot be asked to lose more than this many pieces at once.
const MAX_REDACTIONS: usize = 500;

/// Take words off a page so they are GONE, not hidden.
///
/// The usual way this is done wrong is to draw a black box over the text. The
/// text is still in the file underneath: any reader can select it, and
/// `pdftotext` prints it. That is not a redaction, it is a picture of one, and
/// it has leaked real documents. So the drawing instructions for those glyphs
/// are removed from the content stream first, and the box goes on afterwards
/// only so the page does not have a hole in it.
///
/// The letters that stay must not move. A removed glyph is replaced by a
/// kerning step of exactly its own width, so everything after it sits where it
/// always sat -- and any kerning the run already carried is put back with it.
pub(crate) fn redact_pdf_document(input: &Value) -> Result<Value, PdfError> {
    use crate::document::pdf_layout::text_runs;
    use lopdf::content::{Content, Operation};

    let source = existing_pdf(input, "source", "document.redact")?;
    let destination = new_pdf(input, "destination", "document.redact")?;

    if source == destination {
        return Err(PdfError::invalid(
            "document.redact source and destination must differ",
        ));
    }

    let wanted: Vec<&str> = input
        .get("text")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|text| !text.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let areas = input.get("areas").and_then(Value::as_array);

    if wanted.is_empty() && areas.map(|areas| areas.is_empty()).unwrap_or(true) {
        return Err(PdfError::invalid(
            "document.redact requires text or areas to remove",
        ));
    }

    // A caller who names nothing that is there has almost certainly mistyped
    // the thing they wanted gone, and telling them "done" would be the worst
    // possible answer.
    let must_find = input
        .get("requireMatch")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let mut document = open_with(source, "document.redact", password_of(input, "password"))?;
    let pages = document.get_pages();
    let mut removed = Vec::new();

    for (number, page_id) in pages {
        let runs = text_runs(&document, page_id);

        // Which glyphs of which operation have to go.
        let mut doomed: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        let mut covers: Vec<(f64, f64, f64, f64)> = Vec::new();

        for run in &runs {
            let mut spans: Vec<(usize, usize)> = Vec::new();

            for text in &wanted {
                for span in run.find_all(text) {
                    spans.push(span);
                }
            }

            if let Some(areas) = areas {
                for area in areas {
                    let on_page = area
                        .get("page")
                        .and_then(Value::as_u64)
                        .map(|page| u32::try_from(page).unwrap_or(u32::MAX) == number)
                        .unwrap_or(true);

                    if !on_page {
                        continue;
                    }

                    let Some(rectangle) = area
                        .get("rect")
                        .and_then(Value::as_array)
                        .map(|values| {
                            values.iter().filter_map(Value::as_f64).collect::<Vec<f64>>()
                        })
                        .filter(|values| values.len() == 4)
                    else {
                        return Err(PdfError::invalid(
                            "every area needs a rect of four numbers",
                        ));
                    };

                    let (left, bottom) = (rectangle[0].min(rectangle[2]), rectangle[1].min(rectangle[3]));
                    let (right, top) = (rectangle[0].max(rectangle[2]), rectangle[1].max(rectangle[3]));

                    // A glyph counts as inside when its own box overlaps the
                    // area, so a word half in the region still goes.
                    let mut first: Option<usize> = None;

                    for (index, glyph) in run.glyphs.iter().enumerate() {
                        let overlaps = glyph.x < right
                            && glyph.x + glyph.advance > left
                            && run.y < top
                            && run.y + run.height > bottom;

                        match (overlaps, first) {
                            (true, None) => first = Some(index),
                            (false, Some(start)) => {
                                spans.push((start, index));
                                first = None;
                            }
                            _ => {}
                        }
                    }

                    if let Some(start) = first {
                        spans.push((start, run.glyphs.len()));
                    }
                }
            }

            if !spans.is_empty() && !run.measured {
                // Rewriting a line means stepping over exactly the space the
                // removed letters took. This file never said how wide they
                // are, so that step would be a guess, and a guess here moves
                // every letter that stays. Refused rather than quietly
                // rearranged.
                return Err(PdfError::invalid(format!(
                    "document.redact cannot rewrite page {number}: its font \
                     declares no glyph widths, so the letters that stay could \
                     not be kept in place"
                )));
            }

            for (first, last) in spans {
                if removed.len() + doomed.len() > MAX_REDACTIONS {
                    return Err(PdfError::invalid(format!(
                        "document.redact removes at most {MAX_REDACTIONS} pieces at once"
                    )));
                }

                let entry = doomed.entry(run.operation).or_default();
                entry.extend(first..last);

                let Some(head) = run.glyphs.get(first) else {
                    continue;
                };
                let Some(tail) = run.glyphs.get(last.saturating_sub(1)) else {
                    continue;
                };

                let text: String = run.glyphs[first..last]
                    .iter()
                    .map(|glyph| glyph.text.as_str())
                    .collect();

                covers.push((
                    head.x,
                    run.y,
                    tail.x + tail.advance - head.x,
                    run.height,
                ));

                removed.push(json!({"page": number, "text": text}));
            }
        }

        if doomed.is_empty() {
            continue;
        }

        let Ok(content) = document.get_and_decode_page_content(page_id) else {
            return Err(PdfError::execution(format!(
                "page {number} could not be read to change it"
            )));
        };

        let mut rebuilt: Vec<Operation> = Vec::new();

        // Wrapped so that whatever the page left the graphics state in, the
        // covers below are drawn in the same coordinates the runs were measured
        // in. An unbalanced stream would otherwise put them somewhere else.
        rebuilt.push(Operation::new("q", vec![]));

        for (index, operation) in content.operations.iter().enumerate() {
            let Some(gone) = doomed.get(&index) else {
                rebuilt.push(operation.clone());
                continue;
            };

            let Some(run) = runs.iter().find(|run| run.operation == index) else {
                rebuilt.push(operation.clone());
                continue;
            };

            // The quote operators move to the next line as a side effect. That
            // has to survive being rewritten, or the rest of the page moves up.
            match operation.operator.as_str() {
                "'" => rebuilt.push(Operation::new("T*", vec![])),
                "\"" => {
                    if let Some(spacing) = operation.operands.first() {
                        rebuilt.push(Operation::new("Tw", vec![spacing.clone()]));
                    }
                    if let Some(spacing) = operation.operands.get(1) {
                        rebuilt.push(Operation::new("Tc", vec![spacing.clone()]));
                    }
                    rebuilt.push(Operation::new("T*", vec![]));
                }
                _ => {}
            }

            let mut array: Vec<Object> = Vec::new();
            let mut kept: Vec<u8> = Vec::new();

            let flush = |array: &mut Vec<Object>, kept: &mut Vec<u8>| {
                if !kept.is_empty() {
                    array.push(Object::String(
                        std::mem::take(kept),
                        lopdf::StringFormat::Literal,
                    ));
                }
            };

            for (position, glyph) in run.glyphs.iter().enumerate() {
                if glyph.kern_before != 0.0 {
                    flush(&mut array, &mut kept);
                    array.push(Object::Real(glyph.kern_before as f32));
                }

                if gone.contains(&position) {
                    flush(&mut array, &mut kept);

                    // Step over exactly the space the glyph occupied, so the
                    // letters that stay do not move.
                    let scaled = run.size * run.horizontal;
                    if scaled.abs() > f64::EPSILON {
                        let step = -glyph.advance * 1000.0 / scaled;
                        array.push(Object::Real(step as f32));
                    }
                } else {
                    kept.extend_from_slice(&glyph.codes);
                }
            }

            flush(&mut array, &mut kept);
            rebuilt.push(Operation::new("TJ", vec![Object::Array(array)]));
        }

        rebuilt.push(Operation::new("Q", vec![]));

        // The cover. It goes on last, over a place that no longer has anything
        // under it -- which is the whole point.
        rebuilt.push(Operation::new("q", vec![]));
        rebuilt.push(Operation::new("0 g", vec![]));

        for (x, y, width, height) in covers {
            // A little below the baseline, because letters have descenders.
            let bottom = y - height * 0.25;
            rebuilt.push(Operation::new(
                "re",
                vec![
                    Object::Real(x as f32),
                    Object::Real(bottom as f32),
                    Object::Real(width as f32),
                    Object::Real(height as f32),
                ],
            ));
        }

        rebuilt.push(Operation::new("f", vec![]));
        rebuilt.push(Operation::new("Q", vec![]));

        let encoded = Content {
            operations: rebuilt,
        }
        .encode()
        .map_err(|error| {
            PdfError::execution(format!("page {number} could not be rewritten: {error}"))
        })?;

        document
            .change_page_content(page_id, encoded)
            .map_err(|error| {
                PdfError::execution(format!("page {number} could not be replaced: {error}"))
            })?;
    }

    if removed.is_empty() && must_find {
        return Err(PdfError::invalid(
            "document.redact found nothing to remove; nothing was written",
        ));
    }

    let page_count = write(&mut document, destination)?;

    Ok(json!({
        "capability": "document.redact",
        "selectedProvider": "local-pdf",
        "resourceLocation": "local",
        "inputResource": source,
        "operationResult": {
            "status": "redacted",
            "destination": destination,
            "pageCount": page_count,
            "removed": removed,
        },
        "warnings": [],
        "confirmationConsumed": true,
        "validationResult": "redacted",
    }))
}

/// A document with more marks than this on it is not being read by a person.
const MAX_ANNOTATIONS: usize = 500;
const MAX_FORM_FIELDS: usize = 500;
/// Form fields nest, and a file can claim to nest them forever.
const MAX_FIELD_DEPTH: usize = 16;
/// The side of the sticky note a reader draws, in points.
const NOTE_SIZE: f64 = 24.0;

/// Follow a reference until it is a thing rather than a pointer to one.
fn resolved<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Object> {
    match object {
        Object::Reference(id) => document.get_object(*id).ok(),
        other => Some(other),
    }
}

/// A PDF text string as a person would read it.
fn readable(object: &Object) -> Option<String> {
    lopdf::decode_text_string(object).ok()
}

fn name_of(object: &Object) -> Option<String> {
    object
        .as_name()
        .ok()
        .map(|name| String::from_utf8_lossy(name).into_owned())
}

fn rectangle_of(document: &Document, dictionary: &lopdf::Dictionary) -> Option<Vec<f64>> {
    let value = dictionary.get(b"Rect").ok()?;
    let array = resolved(document, value)?.as_array().ok()?;

    let numbers: Vec<f64> = array
        .iter()
        .filter_map(|value| match value {
            Object::Integer(number) => Some(*number as f64),
            Object::Real(number) => Some(f64::from(*number)),
            _ => None,
        })
        .collect();

    (numbers.len() == 4).then_some(numbers)
}

/// What has been written ON the pages: notes, highlights, links.
///
/// Widgets are left out on purpose. A widget is the FACE of a form field, not a
/// remark someone made, and reporting it in both lists would say a form has
/// been annotated when nobody has annotated anything.
fn annotations_of(document: &Document) -> Vec<Value> {
    let mut annotations = Vec::new();

    for (number, page_id) in document.get_pages() {
        let Ok(page) = document.get_dictionary(page_id) else {
            continue;
        };

        let Some(list) = page
            .get(b"Annots")
            .ok()
            .and_then(|value| resolved(document, value))
            .and_then(|value| value.as_array().ok())
        else {
            continue;
        };

        for entry in list {
            if annotations.len() >= MAX_ANNOTATIONS {
                return annotations;
            }

            let Some(annotation) = resolved(document, entry).and_then(|value| value.as_dict().ok())
            else {
                continue;
            };

            let kind = annotation
                .get(b"Subtype")
                .ok()
                .and_then(name_of)
                .unwrap_or_else(|| "Unknown".to_owned());

            if kind == "Widget" {
                continue;
            }

            let mut entry = json!({"page": number, "type": kind});

            match annotation.get(b"Contents").ok().and_then(readable) {
                Some(contents) if !contents.is_empty() => entry["contents"] = json!(contents),
                _ => {}
            }

            if let Some(author) = annotation.get(b"T").ok().and_then(readable) {
                entry["author"] = json!(author);
            }

            if let Some(modified) = annotation.get(b"M").ok().and_then(readable) {
                entry["modified"] = json!(modified);
            }

            if let Some(rectangle) = rectangle_of(document, annotation) {
                entry["rect"] = json!(rectangle);
            }

            annotations.push(entry);
        }
    }

    annotations
}

/// One place in the form that holds a value.
struct FormField {
    id: ObjectId,
    /// The full name, the way a form refers to its own field: parents joined
    /// to child by a dot, which is how `/T` is meant to be read.
    name: String,
    kind: Option<String>,
    value: Option<Object>,
}

/// The form's fields, in the order the document lists them.
fn form_fields(document: &Document) -> Vec<FormField> {
    let Some(roots) = document
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .and_then(|value| resolved(document, value))
        .and_then(|value| value.as_dict().ok())
        .and_then(|form| form.get(b"Fields").ok())
        .and_then(|value| resolved(document, value))
        .and_then(|value| value.as_array().ok())
    else {
        return Vec::new();
    };

    let mut fields = Vec::new();
    // Walked breadth-first so the order of the answer follows the order the
    // form declares, which is the order a person filling it in would see.
    let mut queue: Vec<(ObjectId, String, Option<String>, Option<Object>, usize)> = roots
        .iter()
        .filter_map(|entry| entry.as_reference().ok())
        .map(|id| (id, String::new(), None, None, 0))
        .collect();

    let mut index = 0;

    while index < queue.len() {
        if fields.len() >= MAX_FORM_FIELDS {
            break;
        }

        let (id, prefix, inherited_kind, inherited_value, depth) = queue[index].clone();
        index += 1;

        let Ok(node) = document.get_dictionary(id) else {
            continue;
        };

        let name = match node.get(b"T").ok().and_then(readable) {
            Some(partial) if prefix.is_empty() => partial,
            Some(partial) => format!("{prefix}.{partial}"),
            None => prefix,
        };

        let kind = node
            .get(b"FT")
            .ok()
            .and_then(name_of)
            .or(inherited_kind);

        let value = node
            .get(b"V")
            .ok()
            .and_then(|value| resolved(document, value))
            .cloned()
            .or(inherited_value);

        // A node with kids that name themselves is a branch of the form; a node
        // whose kids are unnamed has widgets for kids, and IS the field.
        let children: Vec<ObjectId> = node
            .get(b"Kids")
            .ok()
            .and_then(|value| resolved(document, value))
            .and_then(|value| value.as_array().ok())
            .map(|kids| kids.iter().filter_map(|kid| kid.as_reference().ok()).collect())
            .unwrap_or_default();

        let named_children: Vec<ObjectId> = children
            .iter()
            .copied()
            .filter(|kid| {
                document
                    .get_dictionary(*kid)
                    .map(|kid| kid.has(b"T"))
                    .unwrap_or(false)
            })
            .collect();

        if named_children.is_empty() {
            if !name.is_empty() {
                fields.push(FormField {
                    id,
                    name,
                    kind,
                    value,
                });
            }
            continue;
        }

        if depth >= MAX_FIELD_DEPTH {
            continue;
        }

        for child in named_children {
            queue.push((child, name.clone(), kind.clone(), value.clone(), depth + 1));
        }
    }

    fields
}

/// A field's value in the shape a caller can use without knowing PDF.
fn field_value(value: Option<&Object>) -> Value {
    match value {
        None | Some(Object::Null) => Value::Null,
        Some(Object::String(..)) => value.and_then(readable).map(Value::from).unwrap_or(Value::Null),
        Some(object @ Object::Name(_)) => name_of(object).map(Value::from).unwrap_or(Value::Null),
        Some(Object::Integer(number)) => json!(number),
        Some(Object::Real(number)) => json!(number),
        Some(Object::Boolean(flag)) => json!(flag),
        Some(Object::Array(items)) => Value::Array(
            items
                .iter()
                .map(|item| field_value(Some(item)))
                .collect(),
        ),
        Some(_) => Value::Null,
    }
}

fn reported_fields(document: &Document) -> Vec<Value> {
    form_fields(document)
        .into_iter()
        .map(|field| {
            let mut reported = json!({
                "name": field.name,
                "value": field_value(field.value.as_ref()),
            });

            if let Some(kind) = field.kind {
                reported["type"] = json!(match kind.as_str() {
                    "Tx" => "text",
                    "Btn" => "button",
                    "Ch" => "choice",
                    "Sig" => "signature",
                    other => other,
                });
            }

            reported
        })
        .collect()
}

/// The state a checkbox or radio button turns ON to.
///
/// It is not always `/Yes`: the name is whatever the document chose, and it is
/// written into the appearance dictionary. Guessing it produces a box that
/// silently stays empty, so it is read out of the file instead.
fn on_state(document: &Document, field_id: ObjectId) -> Option<String> {
    let mut places = vec![field_id];

    if let Some(kids) = document
        .get_dictionary(field_id)
        .ok()
        .and_then(|node| node.get(b"Kids").ok())
        .and_then(|value| resolved(document, value))
        .and_then(|value| value.as_array().ok())
    {
        places.extend(kids.iter().filter_map(|kid| kid.as_reference().ok()));
    }

    for place in places {
        let Ok(node) = document.get_dictionary(place) else {
            continue;
        };

        let Some(normal) = node
            .get(b"AP")
            .ok()
            .and_then(|value| resolved(document, value))
            .and_then(|value| value.as_dict().ok())
            .and_then(|appearance| appearance.get(b"N").ok())
            .and_then(|value| resolved(document, value))
            .and_then(|value| value.as_dict().ok())
        else {
            continue;
        };

        for (state, _) in normal.iter() {
            if state != b"Off" {
                return Some(String::from_utf8_lossy(state).into_owned());
            }
        }
    }

    None
}

/// Put a note on a page.
///
/// A `/Text` annotation is the one kind that needs no appearance stream of its
/// own: every reader draws the note icon itself. Anything richer -- a
/// highlight, a drawing -- would need an appearance built here, and one that is
/// not built is a mark that some readers show and others do not. So this does
/// the kind that is real everywhere, and says that is what it does.
pub(crate) fn annotate_pdf_document(input: &Value) -> Result<Value, PdfError> {
    let source = existing_pdf(input, "source", "document.annotate")?;
    let destination = new_pdf(input, "destination", "document.annotate")?;

    if source == destination {
        return Err(PdfError::invalid(
            "document.annotate source and destination must differ",
        ));
    }

    let notes = input
        .get("notes")
        .and_then(Value::as_array)
        .filter(|notes| !notes.is_empty())
        .ok_or_else(|| PdfError::invalid("document.annotate requires notes"))?;

    if notes.len() > MAX_ANNOTATIONS {
        return Err(PdfError::invalid(format!(
            "document.annotate accepts at most {MAX_ANNOTATIONS} notes at once"
        )));
    }

    let mut document = open_with(source, "document.annotate", password_of(input, "password"))?;
    let pages = document.get_pages();
    let mut placed = Vec::new();

    for note in notes {
        let page = note
            .get("page")
            .and_then(Value::as_u64)
            .and_then(|page| u32::try_from(page).ok())
            .ok_or_else(|| PdfError::invalid("every note needs a page"))?;

        let page_id = pages.get(&page).copied().ok_or_else(|| {
            PdfError::invalid(format!(
                "document.annotate was asked to mark page {page}, which the document does not have"
            ))
        })?;

        let contents = note
            .get("contents")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|contents| !contents.is_empty())
            .ok_or_else(|| PdfError::invalid("every note needs contents"))?;

        let x = note.get("x").and_then(Value::as_f64).unwrap_or(36.0);
        let y = note.get("y").and_then(Value::as_f64).unwrap_or(36.0);

        let mut annotation = lopdf::Dictionary::new();
        annotation.set("Type", Object::Name(b"Annot".to_vec()));
        annotation.set("Subtype", Object::Name(b"Text".to_vec()));
        annotation.set("Name", Object::Name(b"Note".to_vec()));
        annotation.set(
            "Rect",
            Object::Array(vec![
                Object::Real(x as f32),
                Object::Real(y as f32),
                Object::Real((x + NOTE_SIZE) as f32),
                Object::Real((y + NOTE_SIZE) as f32),
            ]),
        );
        annotation.set("Contents", lopdf::text_string(contents));
        // Printable, so the note is not something only one reader can see.
        annotation.set("F", Object::Integer(4));

        if let Some(author) = note
            .get("author")
            .and_then(Value::as_str)
            .filter(|author| !author.is_empty())
        {
            annotation.set("T", lopdf::text_string(author));
        }

        annotation.set("M", Object::string_literal(pdf_now()));

        let reference = Object::Reference(document.add_object(Object::Dictionary(annotation)));

        // `/Annots` is sometimes the array and sometimes a pointer to it, and
        // appending to the wrong one loses the note without complaining.
        let indirect = document
            .get_dictionary(page_id)
            .ok()
            .and_then(|page| page.get(b"Annots").ok())
            .and_then(|value| value.as_reference().ok());

        match indirect {
            Some(id) => {
                document
                    .get_object_mut(id)
                    .and_then(Object::as_array_mut)
                    .map_err(|_| PdfError::execution("the page's annotation list is unreadable"))?
                    .push(reference);
            }
            None => {
                let page_dictionary = document
                    .get_object_mut(page_id)
                    .and_then(Object::as_dict_mut)
                    .map_err(|_| PdfError::execution("the page is unreadable"))?;

                match page_dictionary.get_mut(b"Annots") {
                    Ok(Object::Array(existing)) => existing.push(reference),
                    _ => page_dictionary.set("Annots", Object::Array(vec![reference])),
                }
            }
        }

        placed.push(json!({"page": page, "contents": contents}));
    }

    let page_count = write(&mut document, destination)?;

    Ok(json!({
        "capability": "document.annotate",
        "selectedProvider": "local-pdf",
        "resourceLocation": "local",
        "inputResource": source,
        "operationResult": {
            "status": "annotated",
            "destination": destination,
            "pageCount": page_count,
            "notes": placed,
        },
        "warnings": [],
        "confirmationConsumed": true,
        "validationResult": "annotated",
    }))
}

/// Fill in a form.
pub(crate) fn fill_pdf_form(input: &Value) -> Result<Value, PdfError> {
    let source = existing_pdf(input, "source", "document.fill")?;
    let destination = new_pdf(input, "destination", "document.fill")?;

    if source == destination {
        return Err(PdfError::invalid(
            "document.fill source and destination must differ",
        ));
    }

    let wanted = input
        .get("fields")
        .and_then(Value::as_object)
        .filter(|fields| !fields.is_empty())
        .ok_or_else(|| PdfError::invalid("document.fill requires fields"))?;

    let mut document = open_with(source, "document.fill", password_of(input, "password"))?;
    let present = form_fields(&document);

    if present.is_empty() {
        return Err(PdfError::invalid(format!(
            "document.fill was given {source}, which has no form to fill"
        )));
    }

    // Every name is checked before anything is written, so a request naming one
    // field wrongly does not leave a half-filled form behind.
    let unknown: Vec<&str> = wanted
        .keys()
        .filter(|name| !present.iter().any(|field| &field.name == *name))
        .map(String::as_str)
        .collect();

    if !unknown.is_empty() {
        return Err(PdfError::invalid(format!(
            "document.fill was asked for fields this form does not have: {}",
            unknown.join(", ")
        )));
    }

    let mut filled = Vec::new();

    for field in &present {
        let Some(requested) = wanted.get(&field.name) else {
            continue;
        };

        let button = field.kind.as_deref() == Some("Btn");

        let value = match requested {
            Value::Bool(state) => {
                let on = on_state(&document, field.id).unwrap_or_else(|| "Yes".to_owned());
                let chosen = if *state { on } else { "Off".to_owned() };
                Object::Name(chosen.into_bytes())
            }
            Value::String(text) if button => Object::Name(text.clone().into_bytes()),
            Value::String(text) => lopdf::text_string(text),
            Value::Number(number) => lopdf::text_string(&number.to_string()),
            Value::Array(choices) => Object::Array(
                choices
                    .iter()
                    .filter_map(Value::as_str)
                    .map(lopdf::text_string)
                    .collect(),
            ),
            Value::Null => Object::Null,
            other => {
                return Err(PdfError::invalid(format!(
                    "document.fill cannot put {other} into {}",
                    field.name
                )));
            }
        };

        // A button's appearance state is separate from its value, and a button
        // whose value says on while its appearance says off looks unchecked.
        let appearance = button.then(|| value.clone());

        let node = document
            .get_object_mut(field.id)
            .and_then(Object::as_dict_mut)
            .map_err(|_| PdfError::execution(format!("{} is unreadable", field.name)))?;

        if matches!(value, Object::Null) {
            node.remove(b"V");
        } else {
            node.set("V", value);
        }

        // The old drawing of the old value would otherwise stay on the page.
        node.remove(b"AP");

        if let Some(state) = appearance.clone() {
            node.set("AS", state);
        }

        let kids: Vec<ObjectId> = document
            .get_dictionary(field.id)
            .ok()
            .and_then(|node| node.get(b"Kids").ok())
            .and_then(|value| value.as_array().ok())
            .map(|kids| kids.iter().filter_map(|kid| kid.as_reference().ok()).collect())
            .unwrap_or_default();

        for kid in kids {
            if let Ok(widget) = document.get_object_mut(kid).and_then(Object::as_dict_mut) {
                widget.remove(b"AP");

                if let Some(state) = appearance.clone() {
                    widget.set("AS", state);
                }
            }
        }

        filled.push(json!({"name": field.name, "value": requested}));
    }

    // Without this the reader shows the old drawing of an empty field and the
    // form looks untouched, which is the classic way a filled form arrives
    // apparently blank.
    let acroform = document
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .and_then(|value| value.as_reference().ok());

    match acroform {
        Some(id) => {
            if let Ok(form) = document.get_object_mut(id).and_then(Object::as_dict_mut) {
                form.set("NeedAppearances", Object::Boolean(true));
            }
        }
        None => {
            if let Ok(catalog) = document.catalog_mut() {
                if let Ok(Object::Dictionary(form)) = catalog.get_mut(b"AcroForm") {
                    form.set("NeedAppearances", Object::Boolean(true));
                }
            }
        }
    }

    let page_count = write(&mut document, destination)?;

    Ok(json!({
        "capability": "document.fill",
        "selectedProvider": "local-pdf",
        "resourceLocation": "local",
        "inputResource": source,
        "operationResult": {
            "status": "filled",
            "destination": destination,
            "pageCount": page_count,
            "fields": filled,
        },
        "warnings": [],
        "confirmationConsumed": true,
        "validationResult": "filled",
    }))
}

/// The moment, written the way a PDF date is written.
fn pdf_now() -> String {
    chrono::Utc::now().format("D:%Y%m%d%H%M%SZ").to_string()
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
fn parse_pages(spec: &str, operation: &str) -> Result<Vec<usize>, PdfError> {
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
        return Err(PdfError::invalid(format!("{operation} requires pages")));
    }

    if pages.len() > MAX_PAGES {
        return Err(PdfError::invalid(format!(
            "{operation} accepts at most {MAX_PAGES} pages"
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

    let wanted = parse_pages(spec, "document.split")?;
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
        assert_eq!(parse_pages("1", "document.split").unwrap(), vec![1]);
        assert_eq!(parse_pages("1-3", "document.split").unwrap(), vec![1, 2, 3]);
        assert_eq!(parse_pages("1-3,7", "document.split").unwrap(), vec![1, 2, 3, 7]);
        assert_eq!(parse_pages(" 2 , 4 - 5 ", "document.split").unwrap(), vec![2, 4, 5]);
        // Order and repetition are the caller's business: extracting page 3
        // twice, or 3 before 1, are both things people mean to do.
        assert_eq!(parse_pages("3,1,3", "document.split").unwrap(), vec![3, 1, 3]);

        // A stray separator is sloppy but unambiguous, so it is tolerated
        // rather than refused: "1,," can only mean page 1, and rejecting it
        // would be strictness that costs the caller something and buys nothing.
        assert_eq!(parse_pages("1,,", "document.split").unwrap(), vec![1]);
        assert_eq!(parse_pages(",2,", "document.split").unwrap(), vec![2]);

        // What is refused is what cannot be read as pages at all, or names a
        // page that cannot exist.
        for rejected in ["", "   ", ",", "0", "0-2", "3-1", "a", "1-b", "-", "1-"] {
            assert!(
                parse_pages(rejected, "document.split").is_err(),
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

    /// A PDF carrying one known line of text per page, built here.
    ///
    /// Portable on purpose. The whole claim of this module is that a PDF is
    /// read and written without an application and without a particular
    /// operating system, and a test whose fixture could only be built on one
    /// system could not prove that claim on any other.
    fn build_text_fixture(destination: &std::path::Path, pages: &[&str]) {
        use lopdf::content::{Content, Operation};
        use lopdf::{dictionary, Stream};

        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        // Every glyph half an em wide, declared rather than assumed.
        //
        // A base-14 font normally carries no `/Widths` at all -- the reader is
        // expected to know Helvetica -- and code that REWRITES a line cannot
        // work from metrics it does not have. Declaring them makes the fixture
        // behave like a real document and, checked against `pdftotext`, real
        // readers honour the declaration over their own idea of Helvetica.
        let widths: Vec<Object> = (32..=126).map(|_| Object::Integer(500)).collect();

        let font_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "FirstChar" => 32,
            "LastChar" => 126,
            "Widths" => widths,
        });
        let resources_id = document.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });

        let kids: Vec<Object> = pages
            .iter()
            .map(|text| {
                let content = Content {
                    operations: vec![
                        Operation::new("BT", vec![]),
                        Operation::new("Tf", vec!["F1".into(), 24.into()]),
                        Operation::new("Td", vec![72.into(), 700.into()]),
                        Operation::new("Tj", vec![Object::string_literal(*text)]),
                        Operation::new("ET", vec![]),
                    ],
                };

                let contents_id =
                    document.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

                document
                    .add_object(dictionary! {
                        "Type" => "Page",
                        "Parent" => pages_id,
                        "Contents" => contents_id,
                    })
                    .into()
            })
            .collect();

        let count = i64::try_from(kids.len()).unwrap();

        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => count,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            }),
        );

        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });

        document.trailer.set("Root", catalog_id);
        document.save(destination).unwrap();
    }

    /// The rotation a written file actually carries, read back from the file.
    fn rotation_of(path: &std::path::Path, page: u32) -> i64 {
        let document = Document::load(path).unwrap();
        let id = *document.get_pages().get(&page).unwrap();

        document
            .get_dictionary(id)
            .unwrap()
            .get(b"Rotate")
            .and_then(|value| value.as_i64())
            .unwrap_or(0)
    }

    /// Turning pages, and locking a file, on any machine.
    ///
    /// Not `#[ignore]`d and not `cfg`-gated: none of this needs an application
    /// or a platform, so the test that proves it should not need one either.
    #[test]
    fn pages_turn_and_a_password_really_locks_the_file() {
        let root = tempfile::tempdir().unwrap();
        let path = |name: &str| root.path().join(name);
        let text = |name: &str| path(name).to_str().unwrap().to_owned();

        build_text_fixture(
            &path("source.pdf"),
            &["Alpha paragraph.", "Bravo paragraph.", "Charlie paragraph."],
        );

        let words = |value: &Value| value["operationResult"]["text"].as_str().unwrap().to_owned();

        // The fixture is what the rest of this test assumes it is.
        let plain = read_pdf_document(&json!({"path": text("source.pdf")})).unwrap();
        assert_eq!(plain["operationResult"]["pageCount"], 3);
        assert!(words(&plain).contains("Alpha paragraph."));

        // ---- turning pages -------------------------------------------------

        let turned = rotate_pdf_document(&json!({
            "source": text("source.pdf"),
            "destination": text("turned.pdf"),
            "degrees": 90,
        }))
        .unwrap();

        assert_eq!(turned["operationResult"]["pageCount"], 3);
        assert_eq!(turned["operationResult"]["pages"][0]["rotation"], 90);
        assert_eq!(rotation_of(&path("turned.pdf"), 1), 90);
        assert_eq!(rotation_of(&path("turned.pdf"), 3), 90);

        // Turning is RELATIVE, which is what "rotate this 90 degrees" means to
        // someone looking at a page that is already sideways.
        rotate_pdf_document(&json!({
            "source": text("turned.pdf"),
            "destination": text("turned-again.pdf"),
            "degrees": 90,
        }))
        .unwrap();
        assert_eq!(rotation_of(&path("turned-again.pdf"), 1), 180);

        // A negative turn lands in the range the format allows, and only the
        // page asked for moves.
        rotate_pdf_document(&json!({
            "source": text("source.pdf"),
            "destination": text("turned-one.pdf"),
            "degrees": -90,
            "pages": "2",
        }))
        .unwrap();
        assert_eq!(rotation_of(&path("turned-one.pdf"), 2), 270);
        assert_eq!(rotation_of(&path("turned-one.pdf"), 1), 0);

        // Turning a page must not cost the document its words.
        let after = read_pdf_document(&json!({"path": text("turned-again.pdf")})).unwrap();
        assert!(words(&after).contains("Charlie paragraph."), "{after:#?}");

        for (label, request) in [
            (
                "a turn that is not a quarter",
                json!({"source": text("source.pdf"), "destination": text("no.pdf"), "degrees": 45}),
            ),
            (
                "no turn at all",
                json!({"source": text("source.pdf"), "destination": text("no.pdf")}),
            ),
            (
                "a page the document does not have",
                json!({"source": text("source.pdf"), "destination": text("no.pdf"), "degrees": 90, "pages": "9"}),
            ),
            (
                "writing over the source",
                json!({"source": text("source.pdf"), "destination": text("source.pdf"), "degrees": 90}),
            ),
        ] {
            let error = rotate_pdf_document(&request).unwrap_err();
            assert!(error.invalid_request, "{label} should be a request problem");
        }

        // ---- locking and unlocking ----------------------------------------

        // A trailing space, because a password is taken exactly as given.
        const PASSWORD: &str = "correct horse ";

        let locked = encrypt_pdf_document(&json!({
            "source": text("source.pdf"),
            "destination": text("locked.pdf"),
            "password": PASSWORD,
        }))
        .unwrap();

        assert_eq!(locked["operationResult"]["pageCount"], 3);
        assert_eq!(locked["operationResult"]["algorithm"], "AES-128");

        // The password is the caller's secret and must not come back out in
        // the answer, where it would end up in a log or a transcript.
        assert!(
            !serde_json::to_string(&locked).unwrap().contains(PASSWORD),
            "the password was echoed back: {locked:#?}"
        );

        // The lock is real, not a claim: the file says it is encrypted, and the
        // words are no longer sitting in it.
        assert!(Document::load(path("locked.pdf")).unwrap().is_encrypted());
        assert!(
            !String::from_utf8_lossy(&std::fs::read(path("locked.pdf")).unwrap())
                .contains("Alpha paragraph."),
            "the text is still readable in the encrypted file"
        );

        // Without the password, refused -- and refused honestly, rather than
        // read back as an empty document.
        let without = read_pdf_document(&json!({"path": text("locked.pdf")})).unwrap_err();
        assert!(without.invalid_request);
        assert!(without.message.contains("encrypted"));

        let wrong =
            read_pdf_document(&json!({"path": text("locked.pdf"), "password": "correct horse"}))
                .unwrap_err();
        assert!(wrong.invalid_request);
        assert!(wrong.message.contains("password"));

        // With it, the whole document is there.
        let opened =
            read_pdf_document(&json!({"path": text("locked.pdf"), "password": PASSWORD})).unwrap();
        assert_eq!(opened["operationResult"]["pageCount"], 3);
        assert!(words(&opened).contains("Bravo paragraph."), "{opened:#?}");

        // Changing the password is opening it under the old one and writing it
        // under the new one.
        encrypt_pdf_document(&json!({
            "source": text("locked.pdf"),
            "destination": text("relocked.pdf"),
            "sourcePassword": PASSWORD,
            "password": "a different secret",
        }))
        .unwrap();

        assert!(read_pdf_document(
            &json!({"path": text("relocked.pdf"), "password": PASSWORD})
        )
        .is_err());
        assert_eq!(
            read_pdf_document(&json!({"path": text("relocked.pdf"), "password": "a different secret"}))
                .unwrap()["operationResult"]["pageCount"],
            3
        );

        // Unlocking gives back a file anything can open.
        assert!(decrypt_pdf_document(&json!({
            "source": text("locked.pdf"),
            "destination": text("no.pdf"),
            "password": "not it",
        }))
        .unwrap_err()
        .invalid_request);

        decrypt_pdf_document(&json!({
            "source": text("locked.pdf"),
            "destination": text("unlocked.pdf"),
            "password": PASSWORD,
        }))
        .unwrap();

        assert!(!Document::load(path("unlocked.pdf")).unwrap().is_encrypted());
        let unlocked = read_pdf_document(&json!({"path": text("unlocked.pdf")})).unwrap();
        assert_eq!(unlocked["operationResult"]["pageCount"], 3);
        assert!(words(&unlocked).contains("Alpha paragraph."), "{unlocked:#?}");

        // And what cannot honestly be done says so.
        for (label, error) in [
            (
                "unlocking a file that is not locked",
                decrypt_pdf_document(&json!({
                    "source": text("source.pdf"),
                    "destination": text("no.pdf"),
                    "password": PASSWORD,
                }))
                .unwrap_err(),
            ),
            (
                "locking without a password",
                encrypt_pdf_document(&json!({
                    "source": text("source.pdf"),
                    "destination": text("no.pdf"),
                }))
                .unwrap_err(),
            ),
            (
                "locking a file that is already locked, with no way in",
                encrypt_pdf_document(&json!({
                    "source": text("locked.pdf"),
                    "destination": text("no.pdf"),
                    "password": "anything",
                }))
                .unwrap_err(),
            ),
        ] {
            assert!(error.invalid_request, "{label} should be a request problem");
        }

        assert!(!path("no.pdf").exists(), "a refused request wrote a file");
    }

    /// Where every letter of a page sits, for comparing before with after.
    fn letter_positions(path: &std::path::Path, page: u32) -> Vec<(String, f64)> {
        use crate::document::pdf_layout::text_runs;

        let document = Document::load(path).unwrap();
        let page_id = *document.get_pages().get(&page).unwrap();

        text_runs(&document, page_id)
            .into_iter()
            .flat_map(|run| {
                run.glyphs
                    .into_iter()
                    .map(|glyph| (glyph.text, glyph.x))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Changing words in place, without moving the ones around them.
    #[test]
    fn words_are_replaced_in_the_font_they_were_set_in() {
        let root = tempfile::tempdir().unwrap();
        let path = |name: &str| root.path().join(name);
        let text = |name: &str| path(name).to_str().unwrap().to_owned();

        build_text_fixture(
            &path("source.pdf"),
            &["Invoice 2024 for Ada Lovelace", "Second page untouched"],
        );

        let before = letter_positions(&path("source.pdf"), 1);

        // Same width, so nothing anywhere can move.
        let done = replace_in_pdf_document(&json!({
            "source": text("source.pdf"),
            "destination": text("fixed.pdf"),
            "replacements": [{"find": "2024", "with": "2026"}],
        }))
        .unwrap();

        assert_eq!(done["operationResult"]["replaced"][0]["to"], "2026");
        assert!(done["warnings"].as_array().unwrap().is_empty());

        let after = words_left(&path("fixed.pdf"));
        assert!(after.contains("Invoice 2026 for Ada Lovelace"), "{after:?}");
        assert!(!after.contains("2024"));
        assert!(after.contains("Second page untouched"));

        // Letter for letter, in the same places.
        let moved = letter_positions(&path("fixed.pdf"), 1);
        assert_eq!(moved.len(), before.len());

        for ((letter, x), (was, at)) in moved.iter().zip(&before) {
            assert!(
                (at - x).abs() < 0.01,
                "{letter:?} (was {was:?}) moved from {at} to {x}"
            );
        }

        // Longer words do not fit, and a PDF cannot reflow, so the default is
        // to say so rather than to overlap the line.
        let refused = replace_in_pdf_document(&json!({
            "source": text("source.pdf"),
            "destination": text("no.pdf"),
            "replacements": [{"find": "Ada", "with": "Augusta Ada"}],
        }))
        .unwrap_err();

        assert!(refused.invalid_request);
        assert!(refused.message.contains("wider"), "{}", refused.message);
        assert!(!path("no.pdf").exists());

        // Asked to fit, it is drawn smaller and says that it was.
        let shrunk = replace_in_pdf_document(&json!({
            "source": text("source.pdf"),
            "destination": text("shrunk.pdf"),
            "whenLonger": "shrink",
            "replacements": [{"find": "Ada", "with": "Augusta Ada"}],
        }))
        .unwrap();

        assert!(
            shrunk["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|warning| warning.as_str().unwrap().contains("smaller")),
            "shrinking must be reported: {:#?}",
            shrunk["warnings"]
        );
        // Drawn at a smaller size, so the text layer is split by the size
        // instruction and a reader shows a space at the seam. The words are all
        // there; they are not one uninterrupted run any more, and that is the
        // price of making longer text fit a fixed space.
        let shrunk_text = words_left(&path("shrunk.pdf"));
        assert!(shrunk_text.contains("Augusta Ada"), "{shrunk_text:?}");
        assert!(shrunk_text.contains("Lovelace"), "{shrunk_text:?}");

        // And the line still ends where it ended, because the replacement was
        // made to occupy exactly the old space.
        let ends = |glyphs: &[(String, f64)]| glyphs.last().map(|(_, x)| *x).unwrap_or_default();
        assert!(
            (ends(&letter_positions(&path("shrunk.pdf"), 1)) - ends(&before)).abs() < 0.05,
            "shrinking to fit must leave the line ending where it was"
        );

        // Asked to push, it is allowed to be wider and says the line moved.
        let pushed = replace_in_pdf_document(&json!({
            "source": text("source.pdf"),
            "destination": text("pushed.pdf"),
            "whenLonger": "push",
            "replacements": [{"find": "Ada", "with": "Augusta Ada"}],
        }))
        .unwrap();

        assert!(pushed["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().unwrap().contains("moved right")));
        assert!(
            ends(&letter_positions(&path("pushed.pdf"), 1)) > ends(&before) + 1.0,
            "pushing must actually move the rest of the line"
        );

        // Chinese, in the page's own font.
        build_chinese_fixture(&path("chinese.pdf"));

        replace_in_pdf_document(&json!({
            "source": text("chinese.pdf"),
            "destination": text("chinese-fixed.pdf"),
            "replacements": [{"find": "陈志明", "with": "明志陈"}],
        }))
        .unwrap();

        assert_eq!(words_left(&path("chinese-fixed.pdf")), "身份证号明志陈");

        // A character the document's own font never carried cannot be drawn in
        // it, and saying which character is the difference between a fix and a
        // page of blank boxes.
        let cannot = replace_in_pdf_document(&json!({
            "source": text("chinese.pdf"),
            "destination": text("no.pdf"),
            "replacements": [{"find": "陈志明", "with": "王小二"}],
        }))
        .unwrap_err();

        assert!(cannot.invalid_request);
        assert!(cannot.message.contains('王'), "{}", cannot.message);

        for (label, request) in [
            (
                "nothing to replace",
                json!({"source": text("source.pdf"), "destination": text("no.pdf")}),
            ),
            (
                "words that are not there",
                json!({"source": text("source.pdf"), "destination": text("no.pdf"),
                       "replacements": [{"find": "not in this document", "with": "x"}]}),
            ),
            (
                "no find",
                json!({"source": text("source.pdf"), "destination": text("no.pdf"),
                       "replacements": [{"with": "x"}]}),
            ),
            (
                "an unknown answer to being too long",
                json!({"source": text("source.pdf"), "destination": text("no.pdf"),
                       "whenLonger": "guess",
                       "replacements": [{"find": "2024", "with": "2026"}]}),
            ),
        ] {
            let error = replace_in_pdf_document(&request).unwrap_err();
            assert!(error.invalid_request, "{label} should be a request problem");
        }

        assert!(!path("no.pdf").exists(), "a refused request wrote a file");
    }

    /// A two-pixel PNG, written out here so the test needs no file beside it.
    ///
    /// Small on purpose: what is under test is that a picture reaches the page
    /// and can be found in it again, not the decoder.
    const RED_PIXELS: [u8; 73] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x02, 0x00, 0x00, 0x00, 0xfd,
        0xd4, 0x9a, 0x73, 0x00, 0x00, 0x00, 0x10, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8,
        0xcf, 0x00, 0x04, 0xff, 0x19, 0x20, 0x14, 0x00, 0x1b, 0xf2, 0x03, 0xfd, 0xd6, 0x96, 0xf2,
        0x2b, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    /// Marks put ON a page, without disturbing what was already on it.
    #[test]
    fn stamps_land_on_the_pages_asked_for_and_say_when_they_cannot() {
        let root = tempfile::tempdir().unwrap();
        let path = |name: &str| root.path().join(name);
        let text = |name: &str| path(name).to_str().unwrap().to_owned();

        build_text_fixture(
            &path("source.pdf"),
            &["First page body.", "Second page body.", "Third page body."],
        );

        let stamped = stamp_pdf_document(&json!({
            "source": text("source.pdf"),
            "destination": text("stamped.pdf"),
            "stamps": [
                {"text": "DRAFT", "pages": "1,3", "x": 200, "y": 400,
                 "size": 48, "rotate": 45, "opacity": 0.3},
                {"text": "page 2 only", "pages": "2", "x": 72, "y": 40},
            ],
        }))
        .unwrap();

        assert_eq!(stamped["operationResult"]["pageCount"], 3);
        assert_eq!(stamped["operationResult"]["stamps"].as_array().unwrap().len(), 3);

        let read = read_pdf_document(&json!({"path": text("stamped.pdf")})).unwrap();
        let pages = read["operationResult"]["pages"].as_array().unwrap().clone();

        // On the pages asked for, and nowhere else.
        assert!(pages[0]["text"].as_str().unwrap().contains("DRAFT"));
        assert!(pages[2]["text"].as_str().unwrap().contains("DRAFT"));
        assert!(!pages[1]["text"].as_str().unwrap().contains("DRAFT"));
        assert!(pages[1]["text"].as_str().unwrap().contains("page 2 only"));

        // And the page keeps everything it already had.
        for (index, body) in ["First page body.", "Second page body.", "Third page body."]
            .iter()
            .enumerate()
        {
            assert!(
                pages[index]["text"].as_str().unwrap().contains(body),
                "stamping page {} lost its own text: {:?}",
                index + 1,
                pages[index]["text"]
            );
        }

        // A picture goes on as a picture, and the page is still a valid PDF.
        std::fs::write(path("seal.png"), RED_PIXELS).unwrap();

        stamp_pdf_document(&json!({
            "source": text("source.pdf"),
            "destination": text("sealed.pdf"),
            "stamps": [{"image": path("seal.png").to_str().unwrap(),
                        "pages": "1", "x": 400, "y": 60, "width": 120}],
        }))
        .unwrap();

        {
            let sealed = Document::load(path("sealed.pdf")).unwrap();
            let page = *sealed.get_pages().get(&1).unwrap();
            let resources = sealed.get_dictionary(page).ok().and_then(|page| {
                page.get(b"Resources")
                    .ok()
                    .and_then(|value| value.as_dict().ok())
            });

            let has_picture = resources
                .and_then(|resources| resources.get(b"XObject").ok())
                .and_then(|value| value.as_dict().ok())
                .map(|xobjects| xobjects.len() > 0)
                .unwrap_or(false);

            assert!(has_picture, "the seal never reached the page");
        }

        // Chinese: drawn with the page's OWN font when that font can spell it.
        build_chinese_fixture(&path("chinese.pdf"));

        stamp_pdf_document(&json!({
            "source": text("chinese.pdf"),
            "destination": text("chinese-stamped.pdf"),
            "stamps": [{"text": "陈志明", "x": 300, "y": 300, "size": 36}],
        }))
        .unwrap();

        assert!(
            words_left(&path("chinese-stamped.pdf")).contains("陈志明"),
            "the stamp did not reach the page"
        );

        // And a character no font on the page can spell is refused BY NAME,
        // rather than drawn as empty boxes the caller only sees later.
        let refused = stamp_pdf_document(&json!({
            "source": text("chinese.pdf"),
            "destination": text("no.pdf"),
            "stamps": [{"text": "机密", "x": 100, "y": 100}],
        }))
        .unwrap_err();

        assert!(refused.invalid_request);
        assert!(
            refused.message.contains('机'),
            "the refusal must name the character: {}",
            refused.message
        );

        for (label, request) in [
            (
                "no stamps",
                json!({"source": text("source.pdf"), "destination": text("no.pdf")}),
            ),
            (
                "neither words nor a picture",
                json!({"source": text("source.pdf"), "destination": text("no.pdf"),
                       "stamps": [{"x": 10, "y": 10}]}),
            ),
            (
                "both at once",
                json!({"source": text("source.pdf"), "destination": text("no.pdf"),
                       "stamps": [{"text": "a", "image": "/tmp/none.png"}]}),
            ),
            (
                "a picture that is not there",
                json!({"source": text("source.pdf"), "destination": text("no.pdf"),
                       "stamps": [{"image": "/tmp/definitely-not-here.png"}]}),
            ),
            (
                "a relative path to a picture",
                json!({"source": text("source.pdf"), "destination": text("no.pdf"),
                       "stamps": [{"image": "seal.png"}]}),
            ),
            (
                "pages the document does not have",
                json!({"source": text("source.pdf"), "destination": text("no.pdf"),
                       "stamps": [{"text": "x", "pages": "9"}]}),
            ),
        ] {
            let error = stamp_pdf_document(&request).unwrap_err();
            assert!(error.invalid_request, "{label} should be a request problem");
        }

        assert!(!path("no.pdf").exists(), "a refused request wrote a file");
    }

    /// A page whose text layer is Chinese.
    ///
    /// The glyphs are Helvetica's and will not draw Chinese, which does not
    /// matter: what is under test is the text layer, the encoding and the
    /// geometry, and those are real. It exists because the first Chinese
    /// document crashed a scan that advanced through the text one BYTE at a
    /// time -- a bug an English-only fixture can never reach.
    fn build_chinese_fixture(destination: &std::path::Path) {
        use lopdf::{dictionary, Stream};

        // code 1..=7 -> 身 份 证 号 陈 志 明
        const LETTERS: [(u8, u16); 7] = [
            (1, 0x8EAB),
            (2, 0x4EFD),
            (3, 0x8BC1),
            (4, 0x53F7),
            (5, 0x9648),
            (6, 0x5FD7),
            (7, 0x660E),
        ];

        let mut cmap = String::from(
            "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
             /CMapName /Fixture def\n/CMapType 2 def\n\
             1 begincodespacerange\n<01> <07>\nendcodespacerange\n",
        );
        cmap.push_str(&format!("{} beginbfchar\n", LETTERS.len()));
        for (code, letter) in LETTERS {
            cmap.push_str(&format!("<{code:02x}> <{letter:04X}>\n"));
        }
        // The closing lines are not decoration: a CMap parser looks for exactly
        // this shape, and without them the fixture's text decodes to nothing.
        // The assertion below, that the fixture reads back as the Chinese it
        // was built from, is what caught that.
        cmap.push_str(
            "endbfchar\nendcmap\n\
             CMapName currentdict /CMap defineresource pop\nend\nend\n",
        );

        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();

        let to_unicode = document.add_object(Stream::new(dictionary! {}, cmap.into_bytes()));
        let widths: Vec<Object> = (1..=7).map(|_| Object::Integer(1000)).collect();

        let font_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "FirstChar" => 1,
            "LastChar" => 7,
            "Widths" => widths,
            "ToUnicode" => to_unicode,
        });

        let resources_id = document.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });

        // 身份证号陈志明 -- the name sits in the middle on purpose, so removing
        // it has to leave text on both sides untouched.
        let line: Vec<u8> = vec![1, 2, 3, 4, 5, 6, 7];
        let mut stream = Vec::new();
        stream.extend_from_slice(b"BT /F1 24 Tf 72 700 Td (");
        for code in &line {
            stream.push(*code);
        }
        stream.extend_from_slice(b") Tj ET");

        let contents_id = document.add_object(Stream::new(dictionary! {}, stream));

        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => contents_id,
        });

        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            }),
        );

        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });

        document.trailer.set("Root", catalog_id);
        document.save(destination).unwrap();
    }

    /// The text a page still has, as an outside reader sees it.
    ///
    /// Uses this module's own reader, which is the point: a redaction that only
    /// looks removed is exactly the failure being tested for, and the text
    /// layer is where it would still be found.
    fn words_left(path: &std::path::Path) -> String {
        read_pdf_document(&json!({"path": path.to_str().unwrap()})).unwrap()["operationResult"]
            ["text"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// Removing words has to remove them, and leave the rest where it was.
    #[test]
    fn redaction_takes_the_words_out_of_the_file_not_just_out_of_sight() {
        use crate::document::pdf_layout::text_runs;

        let root = tempfile::tempdir().unwrap();
        let path = |name: &str| root.path().join(name);
        let text = |name: &str| path(name).to_str().unwrap().to_owned();

        build_text_fixture(
            &path("source.pdf"),
            &[
                "Account 6011 1234 for Ada Lovelace",
                "Second page says nothing secret",
            ],
        );

        // Where every word sat before, so "the rest did not move" can be
        // checked against measurement rather than asserted.
        let before: Vec<(String, f64)> = {
            let document = Document::load(path("source.pdf")).unwrap();
            let page = *document.get_pages().get(&1).unwrap();
            text_runs(&document, page)
                .into_iter()
                .flat_map(|run| {
                    run.glyphs
                        .into_iter()
                        .map(|glyph| (glyph.text, glyph.x))
                        .collect::<Vec<_>>()
                })
                .collect()
        };

        let done = redact_pdf_document(&json!({
            "source": text("source.pdf"),
            "destination": text("clean.pdf"),
            "text": ["6011 1234"],
        }))
        .unwrap();

        assert_eq!(done["operationResult"]["removed"][0]["text"], "6011 1234");

        // The words are GONE from the file, not covered up. This is the whole
        // capability: a black box over live text has leaked real documents.
        let left = words_left(&path("clean.pdf"));
        assert!(!left.contains("6011"), "the account number survived: {left:?}");
        assert!(!left.contains("1234"), "the account number survived: {left:?}");
        assert!(left.contains("Ada Lovelace"), "too much was removed: {left:?}");
        assert!(
            left.contains("Second page says nothing secret"),
            "another page was damaged: {left:?}"
        );

        // And every letter that stayed is exactly where it was.
        let after: Vec<(String, f64)> = {
            let document = Document::load(path("clean.pdf")).unwrap();
            let page = *document.get_pages().get(&1).unwrap();
            text_runs(&document, page)
                .into_iter()
                .flat_map(|run| {
                    run.glyphs
                        .into_iter()
                        .map(|glyph| (glyph.text, glyph.x))
                        .collect::<Vec<_>>()
                })
                .collect()
        };

        // Compared one for one against the glyphs that SHOULD have survived,
        // rather than by searching for each letter in the old list. A page is
        // mostly spaces and repeated letters, and a search happily matches the
        // wrong one -- which is a test that passes while the page is wrong.
        let joined: String = before.iter().map(|(letter, _)| letter.as_str()).collect();
        let at = joined.find("6011 1234").expect("the fixture lost its own text");

        let survivors: Vec<(String, f64)> = before
            .iter()
            .enumerate()
            .filter(|(index, _)| !(at..at + "6011 1234".len()).contains(index))
            .map(|(_, glyph)| glyph.clone())
            .collect();

        assert_eq!(
            after.len(),
            survivors.len(),
            "the page kept {} letters and should have kept {}",
            after.len(),
            survivors.len()
        );

        for ((letter, x), (was, expected_x)) in after.iter().zip(&survivors) {
            assert_eq!(letter, was, "the letters that stayed are not the right ones");
            assert!(
                (expected_x - x).abs() < 0.01,
                "{letter:?} moved from {expected_x} to {x} -- a redaction must not shift the line"
            );
        }

        // An area takes whatever sits in it, which is how a caller redacts
        // something they can point at but cannot name.
        let by_area = redact_pdf_document(&json!({
            "source": text("source.pdf"),
            "destination": text("area.pdf"),
            "areas": [{"page": 1, "rect": [72, 690, 130, 720]}],
        }))
        .unwrap();

        assert!(!by_area["operationResult"]["removed"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(!words_left(&path("area.pdf")).contains("Account"));

        // Asking for something that is not there is a mistake worth reporting,
        // not a file quietly written unchanged.
        let missing = redact_pdf_document(&json!({
            "source": text("source.pdf"),
            "destination": text("no.pdf"),
            "text": ["a phrase this document does not contain"],
        }))
        .unwrap_err();

        assert!(missing.invalid_request);
        assert!(!path("no.pdf").exists(), "a refused request wrote a file");

        // Chinese, because a page is not always English and the code that
        // walks its text must not step into the middle of a character.
        build_chinese_fixture(&path("chinese.pdf"));
        assert_eq!(words_left(&path("chinese.pdf")), "身份证号陈志明");

        redact_pdf_document(&json!({
            "source": text("chinese.pdf"),
            "destination": text("chinese-clean.pdf"),
            "text": ["陈志明"],
        }))
        .unwrap();

        assert_eq!(words_left(&path("chinese-clean.pdf")), "身份证号");

        {
            // And what stayed did not move: the four characters before the name
            // are still at 24 points apart from 72, as they were.
            let document = Document::load(path("chinese-clean.pdf")).unwrap();
            let page = *document.get_pages().get(&1).unwrap();
            let runs = text_runs(&document, page);
            let kept: Vec<f64> = runs
                .iter()
                .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.x))
                .collect();

            assert_eq!(kept, vec![72.0, 96.0, 120.0, 144.0], "the line shifted");
        }

        for (label, request) in [
            (
                "nothing named at all",
                json!({"source": text("source.pdf"), "destination": text("no.pdf")}),
            ),
            (
                "an area with no rectangle",
                json!({"source": text("source.pdf"), "destination": text("no.pdf"),
                       "areas": [{"page": 1}]}),
            ),
        ] {
            let error = redact_pdf_document(&request).unwrap_err();
            assert!(error.invalid_request, "{label} should be a request problem");
        }
    }

    /// A one-page PDF carrying a real form: a text box and a checkbox.
    ///
    /// Built here rather than checked in, so what the form contains is stated
    /// in the test that relies on it. The checkbox turns on to `/On` -- not to
    /// `/Yes` -- on purpose: the on-state is whatever the document chose, and a
    /// filler that guesses it produces a box that silently stays empty.
    fn build_form_fixture(destination: &std::path::Path) {
        use lopdf::{dictionary, Stream};

        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let page_id = document.new_object_id();

        let font_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });

        let blank = |document: &mut Document| {
            document.add_object(Stream::new(
                dictionary! { "Type" => "XObject", "Subtype" => "Form",
                              "BBox" => vec![0.into(), 0.into(), 12.into(), 12.into()] },
                Vec::new(),
            ))
        };

        let on_appearance = blank(&mut document);
        let off_appearance = blank(&mut document);

        let text_field = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Tx",
            "T" => Object::string_literal("full name"),
            "Rect" => vec![72.into(), 700.into(), 300.into(), 724.into()],
            "P" => page_id,
            "F" => 4,
        });

        let checkbox = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Btn",
            "T" => Object::string_literal("agree"),
            "Rect" => vec![72.into(), 660.into(), 84.into(), 672.into()],
            "P" => page_id,
            "F" => 4,
            "V" => Object::Name(b"Off".to_vec()),
            "AS" => Object::Name(b"Off".to_vec()),
            "AP" => dictionary! {
                "N" => dictionary! {
                    "On" => on_appearance,
                    "Off" => off_appearance,
                },
            },
        });

        let resources_id = document.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });

        document.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Annots" => vec![text_field.into(), checkbox.into()],
            }),
        );

        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            }),
        );

        let form_id = document.add_object(dictionary! {
            "Fields" => vec![text_field.into(), checkbox.into()],
            "DA" => Object::string_literal("/F1 0 Tf 0 g"),
            "DR" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        });

        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "AcroForm" => form_id,
        });

        document.trailer.set("Root", catalog_id);
        document.save(destination).unwrap();
    }

    /// Marks on a document, and values in its form, on any machine.
    #[test]
    fn notes_are_left_on_pages_and_forms_are_really_filled() {
        let root = tempfile::tempdir().unwrap();
        let path = |name: &str| root.path().join(name);
        let text = |name: &str| path(name).to_str().unwrap().to_owned();

        // ---- notes ---------------------------------------------------------

        build_text_fixture(&path("source.pdf"), &["Alpha paragraph.", "Bravo paragraph."]);

        // Nothing has been written on it yet, and the answer says so rather
        // than leaving the caller to guess.
        let bare = read_pdf_document(&json!({"path": text("source.pdf")})).unwrap();
        assert_eq!(bare["operationResult"]["annotations"], json!([]));
        assert_eq!(bare["operationResult"]["formFields"], json!([]));

        let marked = annotate_pdf_document(&json!({
            "source": text("source.pdf"),
            "destination": text("marked.pdf"),
            "notes": [
                {"page": 2, "x": 100, "y": 500, "contents": "Check this figure", "author": "Vanessa"},
                {"page": 1, "contents": "And this one"},
            ],
        }))
        .unwrap();

        assert_eq!(marked["operationResult"]["pageCount"], 2);

        let read_back = read_pdf_document(&json!({"path": text("marked.pdf")})).unwrap();
        let notes = read_back["operationResult"]["annotations"]
            .as_array()
            .unwrap()
            .clone();

        assert_eq!(notes.len(), 2, "read back {notes:#?}");

        // Reported in page order, whatever order they were asked for in.
        assert_eq!(notes[0]["page"], 1);
        assert_eq!(notes[0]["contents"], "And this one");
        assert_eq!(notes[1]["page"], 2);
        assert_eq!(notes[1]["type"], "Text");
        assert_eq!(notes[1]["contents"], "Check this figure");
        assert_eq!(notes[1]["author"], "Vanessa");
        assert_eq!(notes[1]["rect"][0], 100.0);

        // The words of the document are still its words.
        assert!(read_back["operationResult"]["text"]
            .as_str()
            .unwrap()
            .contains("Bravo paragraph."));

        for (label, request) in [
            (
                "no notes",
                json!({"source": text("source.pdf"), "destination": text("no.pdf")}),
            ),
            (
                "a page the document does not have",
                json!({"source": text("source.pdf"), "destination": text("no.pdf"),
                       "notes": [{"page": 9, "contents": "nowhere"}]}),
            ),
            (
                "a note that says nothing",
                json!({"source": text("source.pdf"), "destination": text("no.pdf"),
                       "notes": [{"page": 1, "contents": "   "}]}),
            ),
        ] {
            let error = annotate_pdf_document(&request).unwrap_err();
            assert!(error.invalid_request, "{label} should be a request problem");
        }

        // ---- forms ---------------------------------------------------------

        build_form_fixture(&path("form.pdf"));

        let empty = read_pdf_document(&json!({"path": text("form.pdf")})).unwrap();
        let declared = empty["operationResult"]["formFields"].as_array().unwrap();

        assert_eq!(declared.len(), 2, "read back {declared:#?}");
        assert_eq!(declared[0]["name"], "full name");
        assert_eq!(declared[0]["type"], "text");
        assert_eq!(declared[0]["value"], Value::Null);
        assert_eq!(declared[1]["name"], "agree");
        assert_eq!(declared[1]["type"], "button");
        assert_eq!(declared[1]["value"], "Off");

        // A widget is the face of a field, not a remark anyone made, so it must
        // not turn up as an annotation.
        assert_eq!(empty["operationResult"]["annotations"], json!([]));

        let filled = fill_pdf_form(&json!({
            "source": text("form.pdf"),
            "destination": text("filled.pdf"),
            "fields": {"full name": "Ada Lovelace", "agree": true},
        }))
        .unwrap();

        assert_eq!(filled["operationResult"]["fields"].as_array().unwrap().len(), 2);

        let after = read_pdf_document(&json!({"path": text("filled.pdf")})).unwrap();
        let values = after["operationResult"]["formFields"].as_array().unwrap();

        assert_eq!(values[0]["value"], "Ada Lovelace");
        // Turned ON to the state this document actually uses, not to a guess.
        assert_eq!(values[1]["value"], "On");

        // Without this the reader draws the old empty box and the form arrives
        // looking untouched, which is the classic way a filled form is lost.
        let written = Document::load(path("filled.pdf")).unwrap();
        let form = written
            .catalog()
            .unwrap()
            .get(b"AcroForm")
            .and_then(|value| written.get_object(value.as_reference().unwrap()))
            .unwrap()
            .as_dict()
            .unwrap();
        assert_eq!(form.get(b"NeedAppearances").unwrap().as_bool().unwrap(), true);

        // Turning it back off is not the same as never setting it.
        fill_pdf_form(&json!({
            "source": text("filled.pdf"),
            "destination": text("unchecked.pdf"),
            "fields": {"agree": false},
        }))
        .unwrap();

        let unchecked = read_pdf_document(&json!({"path": text("unchecked.pdf")})).unwrap();
        assert_eq!(
            unchecked["operationResult"]["formFields"][1]["value"],
            "Off"
        );
        // And the other field kept what it was given.
        assert_eq!(
            unchecked["operationResult"]["formFields"][0]["value"],
            "Ada Lovelace"
        );

        for (label, error) in [
            (
                "a field this form does not have",
                fill_pdf_form(&json!({
                    "source": text("form.pdf"),
                    "destination": text("no.pdf"),
                    "fields": {"middle name": "Byron"},
                }))
                .unwrap_err(),
            ),
            (
                "no fields at all",
                fill_pdf_form(&json!({
                    "source": text("form.pdf"),
                    "destination": text("no.pdf"),
                    "fields": {},
                }))
                .unwrap_err(),
            ),
            (
                "a document with no form in it",
                fill_pdf_form(&json!({
                    "source": text("source.pdf"),
                    "destination": text("no.pdf"),
                    "fields": {"full name": "Ada Lovelace"},
                }))
                .unwrap_err(),
            ),
        ] {
            assert!(error.invalid_request, "{label} should be a request problem");
        }

        // One wrong name must not leave a half-filled form behind.
        assert!(!path("no.pdf").exists(), "a refused request wrote a file");
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
