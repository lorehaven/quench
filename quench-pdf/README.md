# quench-pdf
A PDF reader that extracts text page by page: cross-reference tables and streams, object streams, fonts and encodings.

## What it is
`quench-pdf` reads PDF files and extracts their text. It depends only on `quench-archive` (for Flate) and replaces `pdf-extract` and `lopdf` in the estate. It reads cross-reference tables and streams (rebuilding by scanning when damaged), object streams, the page tree, content streams including form XObjects, and simple and composite fonts via `ToUnicode`.

It does not write, render, edit or decrypt PDFs. Encrypted files are rejected with `Error::Unsupported`, and text drawn as outlines or images (scans) is not extracted; there is no OCR.

## Use cases
- Index the text of uploaded PDF documents.
- Extract a document page by page for search or summarisation.
- Count pages with `Document` without extracting text.
- Detect scanned files, whose pages come back empty, and route them to OCR elsewhere.

## Example
```rust
let bytes = std::fs::read("report.pdf")?;
for (number, text) in quench_pdf::extract_pages(&bytes)?.iter().enumerate() {
    println!("--- page {} ---\n{text}", number + 1);
}

// everything at once, pages joined by "\n\n"
let text = quench_pdf::extract_text(&bytes)?;

// structure access
let pages = quench_pdf::Document::load(&bytes)?.pages().len();
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-pdf/
