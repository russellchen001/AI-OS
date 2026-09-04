#!/bin/bash
# What can this machine do with a PDF without installing anything?
#
# The Office skill can WRITE a PDF from every application it drives and can
# READ none, which makes PDF a one-way street. Before deciding how far to go,
# this asks what macOS itself offers, because a dependency-free path is worth
# far more than a crate: PDFKit is Apple's own renderer and is reachable from
# JavaScript for Automation with no install.
#
# Each question is asked on its own so one failure cannot be read as another's.
#
# Named probe_ rather than verify_ so verify_all.sh does not pick it up.
set -u

WORK="$HOME/Documents/ai-os-pdf-probe"
rm -rf "$WORK"; mkdir -p "$WORK"

echo "=== fixture: a PDF with a real text layer, made by Word ==="
/usr/bin/osascript - "$WORK/source.docx" <<'APPLESCRIPT'
on run argv
    tell application id "com.microsoft.Word"
        try
            set made to make new document
            set content of text object of made to "Alpha paragraph." & return & "Bravo paragraph." & return & "Charlie paragraph."
            save as made file name (item 1 of argv) file format format document default add to recent files false
            close active document saving no
            return "docx ok"
        on error e number n
            try
                close active document saving no
            end try
            return "docx ERROR " & n & " :: " & e
        end try
    end tell
end run
APPLESCRIPT

/usr/bin/osascript - "$WORK/source.docx" "$WORK/source.pdf" <<'APPLESCRIPT'
-- The form word.rs already proves. Opening and immediately asking for the
-- active document returns -2763: the document is not there yet, so the count
-- has to be waited for.
on run argv
    set stagedAlias to POSIX file (item 1 of argv) as alias
    set beforeCount to 0
    tell application id "com.microsoft.Word"
        try
            set beforeCount to count of documents
            open stagedAlias confirm conversions false read only true add to recent files false
            repeat 100 times
                if (count of documents) > beforeCount then exit repeat
                delay 0.1
            end repeat
            if (count of documents) is not (beforeCount + 1) then error "document count did not increase"
            save as active document file name (item 2 of argv) file format format PDF add to recent files false
            close active document saving no
            return "pdf ok"
        on error e number n
            try
                close active document saving no
            end try
            return "pdf ERROR " & n & " :: " & e
        end try
    end tell
end run
APPLESCRIPT
ls -l "$WORK" 2>/dev/null | tail -3

echo
echo "=== Q1  PDFKit text extraction through JXA, no install ==="
/usr/bin/osascript -l JavaScript - "$WORK/source.pdf" <<'JXA'
function run(argv) {
  ObjC.import('Quartz');
  const url = $.NSURL.fileURLWithPath(argv[0]);
  const doc = $.PDFDocument.alloc.initWithURL(url);
  if (!doc.js) return 'PDFDocument was nil (could not open)';
  const pages = doc.pageCount;
  const text = ObjC.unwrap(doc.string) || '';
  const firstLine = text.split('\n').filter(l => l.trim())[0] || '';
  return `pages=${pages} chars=${text.length} first=[${firstLine}]`;
}
JXA

echo
echo "=== Q2  per-page text, which is what a bounded reader needs ==="
/usr/bin/osascript -l JavaScript - "$WORK/source.pdf" <<'JXA'
function run(argv) {
  ObjC.import('Quartz');
  const doc = $.PDFDocument.alloc.initWithURL($.NSURL.fileURLWithPath(argv[0]));
  if (!doc.js) return 'PDFDocument was nil';
  const out = [];
  for (let i = 0; i < doc.pageCount; i++) {
    const page = doc.pageAtIndex(i);
    const t = ObjC.unwrap(page.string) || '';
    out.push(`page ${i + 1}: ${t.replace(/\s+/g, ' ').trim().slice(0, 60)}`);
  }
  return out.join('\n');
}
JXA

echo
echo "=== Q3  is the PDF encrypted / locked? ==="
/usr/bin/osascript -l JavaScript - "$WORK/source.pdf" <<'JXA'
function run(argv) {
  ObjC.import('Quartz');
  const doc = $.PDFDocument.alloc.initWithURL($.NSURL.fileURLWithPath(argv[0]));
  if (!doc.js) return 'PDFDocument was nil';
  return `encrypted=${doc.isEncrypted} locked=${doc.isLocked}`;
}
JXA

echo
echo "=== Q4  can PDFKit WRITE: split one page out, and merge two ==="
/usr/bin/osascript -l JavaScript - "$WORK/source.pdf" "$WORK/page1.pdf" "$WORK/merged.pdf" <<'JXA'
function run(argv) {
  ObjC.import('Quartz');
  const src = $.PDFDocument.alloc.initWithURL($.NSURL.fileURLWithPath(argv[0]));
  if (!src.js) return 'PDFDocument was nil';

  const one = $.PDFDocument.alloc.init;
  one.insertPageAtIndex(src.pageAtIndex(0), 0);
  const wroteOne = one.writeToURL($.NSURL.fileURLWithPath(argv[1]));

  const both = $.PDFDocument.alloc.init;
  for (let i = 0; i < src.pageCount; i++) both.insertPageAtIndex(src.pageAtIndex(i), both.pageCount);
  for (let i = 0; i < src.pageCount; i++) both.insertPageAtIndex(src.pageAtIndex(i), both.pageCount);
  const wroteBoth = both.writeToURL($.NSURL.fileURLWithPath(argv[2]));

  return `split_written=${wroteOne} merged_written=${wroteBoth} merged_pages=${both.pageCount}`;
}
JXA

echo
echo "=== Q5  is the text layer what a scanned PDF lacks? (image-only page) ==="
# A real PNG on disk: an NSImage made from a size alone has no representations
# and arrives at PDFPage as NULL.
printf '89504e470d0a1a0a0000000d494844520000000100000001080400000' > /dev/null
/usr/bin/xxd -r -p <<'HEX' > "$WORK/dot.png"
89504e470d0a1a0a0000000d4948445200000001000000010804000000b51c0c020000000b4944415478da6364f80f0001050101271
8e3660000000049454e44ae426082
HEX
/usr/bin/osascript -l JavaScript - "$WORK/scan.pdf" "$WORK/dot.png" <<'JXA'
function run(argv) {
  ObjC.import('Quartz');
  ObjC.import('AppKit');
  // A page built from an image alone, which is what a scan is.
  const img = $.NSImage.alloc.initWithContentsOfFile(argv[1]);
  if (!img.js) return 'could not load the image fixture';
  const page = $.PDFPage.alloc.initWithImage(img);
  const doc = $.PDFDocument.alloc.init;
  doc.insertPageAtIndex(page, 0);
  doc.writeToURL($.NSURL.fileURLWithPath(argv[0]));
  const back = $.PDFDocument.alloc.initWithURL($.NSURL.fileURLWithPath(argv[0]));
  const text = ObjC.unwrap(back.string) || '';
  return `image_only_pdf chars=${text.length} (0 means OCR would be required)`;
}
JXA

echo
echo "=== files ==="
ls -l "$WORK"
rm -rf "$WORK"
