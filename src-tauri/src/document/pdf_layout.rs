//! Where the words actually are.
//!
//! `extract_text` gives a page's words and not one thing about where they sit,
//! and every operation that changes a page needs to know: covering a phrase,
//! replacing one, or deciding whether replacement text fits all begin with its
//! box. So this walks the content stream as a PDF reader does -- keeping the
//! text matrices, the spacing parameters and the font in hand -- and reports
//! each run of text with the place it occupies on the page.

use lopdf::content::Content;
use lopdf::{Document, Object, ObjectId};
use std::collections::BTreeMap;

/// A 2x3 PDF matrix: a b c d e f.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Matrix(pub(crate) [f64; 6]);

impl Matrix {
    pub(crate) fn identity() -> Self {
        Matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
    }

    /// `self` then `other`, which is how PDF composes them.
    pub(crate) fn then(self, other: Matrix) -> Matrix {
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = other.0;
        Matrix([
            a * a2 + b * c2,
            a * b2 + b * d2,
            c * a2 + d * c2,
            c * b2 + d * d2,
            e * a2 + f * c2 + e2,
            e * b2 + f * d2 + f2,
        ])
    }

    pub(crate) fn apply(self, x: f64, y: f64) -> (f64, f64) {
        let [a, b, c, d, e, f] = self.0;
        (a * x + c * y + e, b * x + d * y + f)
    }
}

/// One glyph as the page draws it: the bytes that select it, what it means,
/// where it starts and how far it moves the pen.
///
/// Kept per glyph rather than per run because everything that changes a page
/// needs a PART of a run: the account number inside a sentence, not the
/// sentence. Without this, covering a phrase means covering its whole line.
#[derive(Debug, Clone)]
pub(crate) struct Glyph {
    pub(crate) codes: Vec<u8>,
    pub(crate) text: String,
    pub(crate) x: f64,
    pub(crate) advance: f64,
    /// The kerning number that stood before this glyph in a `TJ` array.
    ///
    /// Carried so a rewritten run can put it back. Dropping it would move
    /// every kept letter after the removed one, which is exactly the damage a
    /// redaction is supposed not to do.
    pub(crate) kern_before: f64,
}

/// One run of text as the content stream draws it.
#[derive(Debug, Clone)]
pub(crate) struct TextRun {
    /// Index of the operation that drew it, so it can be found again to change.
    pub(crate) operation: usize,
    pub(crate) text: String,
    /// The bytes as the content stream holds them, which is what has to be
    /// matched or rewritten -- the decoded text is for people.
    pub(crate) codes: Vec<u8>,
    pub(crate) font: Vec<u8>,
    pub(crate) size: f64,
    /// Bottom-left of the run in page coordinates, and its extent.
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) glyphs: Vec<Glyph>,
    /// False when the widths behind these positions were guessed rather than
    /// read out of the file. Anything that REWRITES the page must refuse such
    /// a run; anything that only reports position may use it and say so.
    pub(crate) measured: bool,
    /// The text state this run was drawn under. A writer has to reproduce it
    /// exactly, or its measurement of the new text is against different rules
    /// than the old text was laid out by.
    pub(crate) char_spacing: f64,
    pub(crate) word_spacing: f64,
    pub(crate) horizontal: f64,
}

impl TextRun {
    /// Where `text` sits inside this run, as a span of glyphs.
    ///
    /// Matched on the DECODED text, because that is what a caller can name,
    /// and returned as glyph indices, because that is what can be removed.
    pub(crate) fn find(&self, wanted: &str) -> Option<(usize, usize)> {
        self.find_all(wanted).into_iter().next()
    }

    /// Every place `wanted` sits in this run, as spans of glyphs.
    ///
    /// Matched on the DECODED text, because that is what a caller can name,
    /// and returned as glyph indices, because that is what can be removed.
    pub(crate) fn find_all(&self, wanted: &str) -> Vec<(usize, usize)> {
        if wanted.is_empty() {
            return Vec::new();
        }

        let mut assembled = String::new();
        let mut boundaries = Vec::with_capacity(self.glyphs.len() + 1);

        for glyph in &self.glyphs {
            boundaries.push(assembled.len());
            assembled.push_str(&glyph.text);
        }
        boundaries.push(assembled.len());

        let mut found = Vec::new();

        // `match_indices` rather than a hand-rolled scan: advancing by one
        // BYTE lands inside a character the moment the page is not English,
        // and slicing there panics. The first Chinese document found that.
        for (at, _) in assembled.match_indices(wanted) {
            let end = at + wanted.len();

            // A match starting or ending inside a glyph cannot be removed
            // without removing a character the caller did not name, so it is
            // not treated as a match.
            let (Some(first), Some(last)) = (
                boundaries.iter().position(|start| *start == at),
                boundaries.iter().position(|start| *start == end),
            ) else {
                continue;
            };

            found.push((first, last));
        }

        found
    }
}

/// A font's widths, keyed by the code the content stream uses.
pub(crate) struct Metrics {
    first: i64,
    widths: Vec<f64>,
    default: f64,
    /// CID fonts address glyphs with two bytes, and a width table keyed by CID.
    two_byte: bool,
    cid: BTreeMap<u32, f64>,
    /// Whether the file actually told us how wide the glyphs are.
    ///
    /// A base-14 font may carry no `/Widths` at all: the reader is expected to
    /// know Helvetica's metrics, and we do not. Positions computed from a
    /// guess are close enough to read and NOT close enough to rewrite a line
    /// with, so the guess is marked rather than passed off as measurement.
    known: bool,
}

impl Metrics {
    /// Whether the file said how wide these glyphs are, rather than us guessing.
    pub(crate) fn known(&self) -> bool {
        self.known
    }

    /// How far a run of codes moves the pen.
    ///
    /// The same arithmetic the layout walk uses, exposed so that text being
    /// written onto a page is measured exactly the way text already on it was.
    pub(crate) fn advance(
        &self,
        codes: &[u8],
        size: f64,
        char_spacing: f64,
        word_spacing: f64,
        horizontal: f64,
    ) -> f64 {
        let step = if self.two_byte { 2 } else { 1 };
        let mut total = 0.0;

        for chunk in codes.chunks(step) {
            let code = chunk
                .iter()
                .fold(0u32, |value, byte| (value << 8) | u32::from(*byte));

            total += self.width(code) * size + char_spacing;

            if !self.two_byte && chunk == [32] {
                total += word_spacing;
            }
        }

        total * horizontal
    }

    fn width(&self, code: u32) -> f64 {
        if self.two_byte {
            return *self.cid.get(&code).unwrap_or(&self.default) / 1000.0;
        }

        let index = i64::from(code) - self.first;
        if index >= 0 && (index as usize) < self.widths.len() {
            self.widths[index as usize] / 1000.0
        } else {
            self.default / 1000.0
        }
    }
}

fn number(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(f64::from(*value)),
        _ => None,
    }
}

fn deref<'a>(document: &'a Document, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => document.get_object(*id).unwrap_or(object),
        other => other,
    }
}

pub(crate) fn metrics_of(document: &Document, font: &lopdf::Dictionary) -> Metrics {
    // A composite font keeps its widths one level down, on the descendant, and
    // addresses glyphs with two bytes rather than one.
    if let Ok(descendants) = font
        .get(b"DescendantFonts")
        .map(|value| deref(document, value))
        .and_then(Object::as_array)
    {
        let descendant = descendants
            .first()
            .map(|value| deref(document, value))
            .and_then(|value| value.as_dict().ok());

        let mut cid = BTreeMap::new();
        let mut default = 1000.0;

        if let Some(descendant) = descendant {
            if let Some(value) = descendant.get(b"DW").ok().and_then(number) {
                default = value;
            }

            if let Ok(widths) = descendant
                .get(b"W")
                .map(|value| deref(document, value))
                .and_then(Object::as_array)
            {
                // /W is [ c [w w w] c_first c_last w ] -- two shapes in one array.
                let mut index = 0;
                while index < widths.len() {
                    let Some(start) = number(deref(document, &widths[index])) else {
                        break;
                    };

                    match widths.get(index + 1).map(|value| deref(document, value)) {
                        Some(Object::Array(list)) => {
                            for (offset, item) in list.iter().enumerate() {
                                if let Some(width) = number(deref(document, item)) {
                                    cid.insert(start as u32 + offset as u32, width);
                                }
                            }
                            index += 2;
                        }
                        Some(end) => {
                            let end = number(end).unwrap_or(start);
                            let width = widths
                                .get(index + 2)
                                .and_then(|value| number(deref(document, value)))
                                .unwrap_or(default);
                            for code in start as u32..=end as u32 {
                                cid.insert(code, width);
                            }
                            index += 3;
                        }
                        None => break,
                    }
                }
            }
        }

        return Metrics {
            known: !cid.is_empty() || descendant.is_some(),
            first: 0,
            widths: Vec::new(),
            default,
            two_byte: true,
            cid,
        };
    }

    let first = font.get(b"FirstChar").ok().and_then(number).unwrap_or(0.0) as i64;
    let widths: Vec<f64> = font
        .get(b"Widths")
        .map(|value| deref(document, value))
        .and_then(Object::as_array)
        .map(|list| {
            list.iter()
                .map(|value| number(deref(document, value)).unwrap_or(0.0))
                .collect()
        })
        .unwrap_or_default();

    // A base-14 font carries no widths at all. 500 is a guess and is marked as
    // one by the caller, rather than pretending to a precision we do not have.
    Metrics {
        known: !widths.is_empty(),
        first,
        widths,
        default: 500.0,
        two_byte: false,
        cid: BTreeMap::new(),
    }
}

/// Every run of text on a page, with where it sits.
pub(crate) fn text_runs(document: &Document, page_id: ObjectId) -> Vec<TextRun> {
    let Ok(content) = document.get_and_decode_page_content(page_id) else {
        return Vec::new();
    };

    let fonts = document.get_page_fonts(page_id).unwrap_or_default();
    let metrics: BTreeMap<Vec<u8>, Metrics> = fonts
        .iter()
        .map(|(name, font)| (name.clone(), metrics_of(document, font)))
        .collect();
    let encodings: BTreeMap<Vec<u8>, lopdf::Encoding> = fonts
        .iter()
        .filter_map(|(name, font)| {
            font.get_font_encoding(document)
                .ok()
                .map(|encoding| (name.clone(), encoding))
        })
        .collect();

    let mut runs = Vec::new();

    // Graphics state, only as much of it as position needs.
    let mut ctm = Matrix::identity();
    let mut stack: Vec<Matrix> = Vec::new();

    let mut tm = Matrix::identity();
    let mut tlm = Matrix::identity();
    let mut font: Vec<u8> = Vec::new();
    let mut size = 0.0f64;
    let mut leading = 0.0f64;
    let mut char_spacing = 0.0f64;
    let mut word_spacing = 0.0f64;
    let mut horizontal = 1.0f64;
    let mut rise = 0.0f64;

    for (index, operation) in content.operations.iter().enumerate() {
        let operands = &operation.operands;
        let n = |i: usize| operands.get(i).and_then(number).unwrap_or(0.0);

        match operation.operator.as_str() {
            "q" => stack.push(ctm),
            "Q" => ctm = stack.pop().unwrap_or(ctm),
            "cm" => {
                ctm = Matrix([n(0), n(1), n(2), n(3), n(4), n(5)]).then(ctm);
            }
            "BT" => {
                tm = Matrix::identity();
                tlm = tm;
            }
            "Tf" => {
                font = operands
                    .first()
                    .and_then(|value| value.as_name().ok())
                    .map(<[u8]>::to_vec)
                    .unwrap_or_default();
                size = n(1);
            }
            "TL" => leading = n(0),
            "Tc" => char_spacing = n(0),
            "Tw" => word_spacing = n(0),
            "Tz" => horizontal = n(0) / 100.0,
            "Ts" => rise = n(0),
            "Td" => {
                tlm = Matrix([1.0, 0.0, 0.0, 1.0, n(0), n(1)]).then(tlm);
                tm = tlm;
            }
            "TD" => {
                leading = -n(1);
                tlm = Matrix([1.0, 0.0, 0.0, 1.0, n(0), n(1)]).then(tlm);
                tm = tlm;
            }
            "Tm" => {
                tlm = Matrix([n(0), n(1), n(2), n(3), n(4), n(5)]);
                tm = tlm;
            }
            "T*" => {
                tlm = Matrix([1.0, 0.0, 0.0, 1.0, 0.0, -leading]).then(tlm);
                tm = tlm;
            }
            "Tj" | "'" | "\"" | "TJ" => {
                // The quote operators move to the next line first, and the
                // double quote sets spacing on its way past.
                if operation.operator == "'" || operation.operator == "\"" {
                    if operation.operator == "\"" {
                        word_spacing = n(0);
                        char_spacing = n(1);
                    }
                    tlm = Matrix([1.0, 0.0, 0.0, 1.0, 0.0, -leading]).then(tlm);
                    tm = tlm;
                }

                let Some(metric) = metrics.get(&font) else {
                    continue;
                };
                let encoding = encodings.get(&font);
                let two_byte = metric.two_byte;

                let items: Vec<&Object> = match operation.operator.as_str() {
                    "TJ" => operands
                        .first()
                        .and_then(|value| value.as_array().ok())
                        .map(|list| list.iter().collect())
                        .unwrap_or_default(),
                    _ => operands.iter().filter(|o| matches!(o, Object::String(..))).collect(),
                };

                let start = tm;
                let mut text = String::new();
                let mut codes: Vec<u8> = Vec::new();
                let mut glyphs: Vec<Glyph> = Vec::new();
                let mut pending_kern = 0.0f64;

                for item in items {
                    match item {
                        Object::String(bytes, _) => {
                            codes.extend_from_slice(bytes);

                            let step = if two_byte { 2 } else { 1 };
                            for chunk in bytes.chunks(step) {
                                let code = chunk
                                    .iter()
                                    .fold(0u32, |value, byte| (value << 8) | u32::from(*byte));

                                let mut advance = metric.width(code) * size + char_spacing;
                                if !two_byte && chunk == [32] {
                                    advance += word_spacing;
                                }
                                advance *= horizontal;

                                let letter = encoding
                                    .and_then(|encoding| Document::decode_text(encoding, chunk).ok())
                                    .unwrap_or_default();

                                text.push_str(&letter);

                                let (x, _) = tm.then(ctm).apply(0.0, rise);

                                glyphs.push(Glyph {
                                    codes: chunk.to_vec(),
                                    text: letter,
                                    x,
                                    advance,
                                    kern_before: std::mem::take(&mut pending_kern),
                                });

                                tm = Matrix([1.0, 0.0, 0.0, 1.0, advance, 0.0]).then(tm);
                            }
                        }
                        adjustment => {
                            if let Some(value) = number(adjustment) {
                                pending_kern += value;
                                let shift = -value / 1000.0 * size * horizontal;
                                tm = Matrix([1.0, 0.0, 0.0, 1.0, shift, 0.0]).then(tm);
                            }
                        }
                    }
                }

                if text.is_empty() && codes.is_empty() {
                    continue;
                }

                let begin = start.then(ctm);
                let end = tm.then(ctm);
                let (x, y) = begin.apply(0.0, rise);
                let (x_end, _) = end.apply(0.0, rise);

                runs.push(TextRun {
                    operation: index,
                    text,
                    codes,
                    font: font.clone(),
                    size,
                    x,
                    y,
                    width: x_end - x,
                    // The em box, which is what a cover has to hide.
                    height: size * begin.0[3].abs().max(0.000_001),
                    glyphs,
                    measured: metric.known,
                    char_spacing,
                    word_spacing,
                    horizontal,
                });
            }
            _ => {}
        }
    }

    runs
}

/// Every character a font can actually draw, and the bytes that select it.
///
/// This is the reverse of what a reader needs, and it is what anything WRITING
/// on a page needs: given a letter, which code puts it there. It is built by
/// asking the font's own encoding what each code means, because an embedded
/// font is usually a SUBSET -- it carries only the glyphs the document already
/// used -- and a letter that is not in this map cannot be drawn in that font at
/// all. Answering that honestly is the difference between a stamp that reads
/// the way it was asked for and one that comes out as blanks.
pub(crate) fn writable_codes(
    document: &Document,
    font: &lopdf::Dictionary,
) -> BTreeMap<String, Vec<u8>> {
    let Ok(encoding) = font.get_font_encoding(document) else {
        return BTreeMap::new();
    };

    let composite = font
        .get(b"DescendantFonts")
        .map(|value| deref(document, value))
        .and_then(Object::as_array)
        .is_ok();

    let mut codes = BTreeMap::new();
    let last: u32 = if composite { 0xFFFF } else { 0xFF };

    for code in 0..=last {
        let bytes = if composite {
            vec![(code >> 8) as u8, code as u8]
        } else {
            vec![code as u8]
        };

        let Ok(letter) = Document::decode_text(&encoding, &bytes) else {
            continue;
        };

        if letter.is_empty() {
            continue;
        }

        // First code wins: a font can map two codes to the same letter, and the
        // lower one is the one the document is likelier to have used.
        codes.entry(letter).or_insert(bytes);
    }

    codes
}
