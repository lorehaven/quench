---
title: Usage
parent: quench-pdf
nav_order: 1
---

## 1. Extract the text of every page

```rust
let bytes = std::fs::read("quench-pdf/tests/fixtures/cairo.pdf")?;
let pages: Vec<String> = quench_pdf::extract_pages(&bytes)?;
assert_eq!(pages.len(), 1);
assert!(pages[0].starts_with("Cairo text line 0: "));
# Ok::<(), Box<dyn std::error::Error>>(())
```

That fixture is a one-page file produced by cairo; the first line of its text is `Cairo text line 0: Zażółć gęślą jaźń ünï`. Running the call above on it gives exactly one page.

## 2. All the text at once

```rust
let bytes = std::fs::read("report.pdf")?;
let text = quench_pdf::extract_text(&bytes)?;   // pages joined by "\n\n"
# Ok::<(), Box<dyn std::error::Error>>(())
```

Pitfall: a blank page is an empty string in `extract_pages`; `extract_text` still puts the separator.

## 3. Handle encrypted and broken files

```rust
use quench_pdf::Error;

fn read(bytes: &[u8]) -> Option<String> {
    match quench_pdf::extract_text(bytes) {
        Ok(text) => Some(text),
        Err(Error::Unsupported(_)) => None,   // encrypted (trailer has /Encrypt)
        Err(Error::Malformed(_)) => None,     // no %PDF- header in the first 4 KiB, or no catalog
    }
}
assert!(read(b"hello").is_none());
```

## 4. Count pages without extracting text

```rust
let bytes = std::fs::read("report.pdf")?;
let doc = quench_pdf::Document::load(&bytes)?;
let n = doc.pages().len();
# Ok::<(), Box<dyn std::error::Error>>(())
```

`Document::pages()` returns `Vec<Page>`, but `Page`, `Object` and `Dict` are not exported from the crate root, so callers cannot name them (see [API reference](api.md#document)). In practice `pages().len()` is the useful part of the object API.

## 5. Use with scans

A page that only contains an image yields an empty string (the image is skipped). Detect it by comparing text length with page count, and send those files to an OCR pipeline; this crate has none.
