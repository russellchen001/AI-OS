#!/bin/bash
# Can this machine read a scanned PDF without installing anything?
#
# PDFKit already answers text extraction, page splitting and merging
# (verify/probe_pdf_native_capability.sh). It returns zero characters for a page
# that is only an image, which is what a scan is. Vision is the built-in
# framework that would answer those, and this asks whether it can be driven the
# way this codebase drives everything else: synchronously, deterministically,
# from a script, with no install.
#
# The fixture is honest about its own answer: a text PDF is rendered to an
# image and rebuilt as an image-only PDF, so what OCR should recover is known
# exactly rather than judged by eye.
#
# Named probe_ rather than verify_ so verify_all.sh does not pick it up.
set -u

WORK="$HOME/Documents/ai-os-pdf-ocr-probe"
rm -rf "$WORK"; mkdir -p "$WORK"

echo "=== is a Swift compiler available at all? (a fallback if JXA cannot) ==="
if [ -x /usr/bin/swift ]; then
  /usr/bin/swift --version 2>&1 | head -2
else
  echo "no /usr/bin/swift"
fi

echo
echo "=== fixture: a text PDF from Word ==="
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

/usr/bin/osascript - "$WORK/source.docx" "$WORK/text.pdf" <<'APPLESCRIPT'
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
            save as active document file name (item 2 of argv) file format format PDF add to recent files false
            close active document saving no
            return "text pdf ok"
        on error e number n
            try
                close active document saving no
            end try
            return "text pdf ERROR " & n & " :: " & e
        end try
    end tell
end run
APPLESCRIPT

echo
echo "=== fixture: the same page rebuilt as an IMAGE, which is what a scan is ==="
/usr/bin/osascript -l JavaScript - "$WORK/text.pdf" "$WORK/scan.pdf" <<'JXA'
function run(argv) {
  ObjC.import('Quartz');
  ObjC.import('AppKit');
  const src = $.PDFDocument.alloc.initWithURL($.NSURL.fileURLWithPath(argv[0]));
  if (!src.js) return 'source PDF was nil';
  const page = src.pageAtIndex(0);
  // Rendered large, because OCR on a thumbnail is a test of the thumbnail.
  const bounds = page.boundsForBox($.kPDFDisplayBoxMediaBox);
  const scale = 3;
  const size = $.NSMakeSize(bounds.size.width * scale, bounds.size.height * scale);
  const image = page.thumbnailOfSizeForBox(size, $.kPDFDisplayBoxMediaBox);
  if (!image.js) return 'thumbnail was nil';
  const imagePage = $.PDFPage.alloc.initWithImage(image);
  if (!imagePage.js) return 'image page was nil';
  const out = $.PDFDocument.alloc.init;
  out.insertPageAtIndex(imagePage, 0);
  const wrote = out.writeToURL($.NSURL.fileURLWithPath(argv[1]));
  const back = $.PDFDocument.alloc.initWithURL($.NSURL.fileURLWithPath(argv[1]));
  const chars = (ObjC.unwrap(back.string) || '').length;
  return `written=${wrote} text_layer_chars=${chars} (0 confirms it is a scan)`;
}
JXA

echo
echo "=== Q1  Vision OCR from JXA, synchronously ==="
/usr/bin/osascript -l JavaScript - "$WORK/scan.pdf" <<'JXA'
function run(argv) {
  ObjC.import('Quartz');
  ObjC.import('AppKit');
  ObjC.import('Vision');

  const doc = $.PDFDocument.alloc.initWithURL($.NSURL.fileURLWithPath(argv[0]));
  if (!doc.js) return 'PDF was nil';
  const page = doc.pageAtIndex(0);
  const bounds = page.boundsForBox($.kPDFDisplayBoxMediaBox);
  const size = $.NSMakeSize(bounds.size.width * 3, bounds.size.height * 3);
  const image = page.thumbnailOfSizeForBox(size, $.kPDFDisplayBoxMediaBox);
  const cg = image.CGImageForProposedRectContextHints($(), $(), $());
  if (!cg) return 'could not get a CGImage from the page';

  const request = $.VNRecognizeTextRequest.alloc.init;
  request.recognitionLevel = 1; // accurate
  const handler = $.VNImageRequestHandler.alloc.initWithCGImageOptions(cg, $({}));
  const ok = handler.performRequestsError($([request]), $());
  if (!ok) return 'performRequests returned false';

  const results = request.results;
  const lines = [];
  for (let i = 0; i < results.count; i++) {
    const obs = results.objectAtIndex(i);
    const top = obs.topCandidates(1).objectAtIndex(0);
    lines.push(`${ObjC.unwrap(top.string)}  (confidence ${Number(top.confidence).toFixed(2)})`);
  }
  return `synchronous=yes lines=${results.count}\n` + lines.join('\n');
}
JXA

echo
echo "=== Q2  which languages does it support here? ==="
/usr/bin/osascript -l JavaScript <<'JXA'
function run() {
  ObjC.import('Vision');
  const request = $.VNRecognizeTextRequest.alloc.init;
  request.recognitionLevel = 1;
  try {
    const langs = request.supportedRecognitionLanguagesAndReturnError($());
    return ObjC.unwrap(langs).join(', ');
  } catch (e) {
    return 'could not query languages: ' + e;
  }
}
JXA

echo
echo "=== files ==="
ls -l "$WORK"
rm -rf "$WORK"
