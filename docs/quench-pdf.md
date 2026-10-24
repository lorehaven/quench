# Quench PDF

`quench-pdf` (crate `quench_pdf`) reads PDF files and extracts their text page by page. It depends only on `quench-archive` (for Flate) and replaces `pdf-extract` and `lopdf`.

```rust
let pages = quench_pdf::extract_pages(&bytes)?;      // Vec<String>, one per page
let all = quench_pdf::extract_text(&bytes)?;         // pages joined by a blank line
```

`Document::load(&bytes)` gives the parsed structure (`pages()`, `get_object`, `resolve`, `stream_data`) for callers that need more than text.

## What it reads

- **Structure:** cross-reference tables and streams (including hybrid files and `/Prev` chains), object streams, the page tree with inherited resources. A file whose cross-reference data is missing or wrong is rebuilt by scanning for objects; streams with a wrong `/Length` are found by their `endstream`.
- **Filters:** Flate, LZW (verified against libtiff output), ASCII85, ASCIIHex, RunLength, with PNG and TIFF predictors. Image filters are not decoded; a stream that cannot be decoded gives an empty page, not an error.
- **Fonts:** simple fonts through `ToUnicode`, the base encodings (Standard, WinAnsi, MacRoman) and `Differences` with the Adobe Glyph List (also `uniXXXX`, `uXXXX`, `a_b`); composite (CID) fonts through `ToUnicode` and embedded CMaps; Type 3 fonts. Glyph widths come from `/Widths`, `/W` and `/DW`.
- **Content:** text operators (`Tj TJ ' " Td TD Tm T* Tc Tw Tz TL Ts Tf`), the graphics state (`q Q cm`), form XObjects (to a depth of 8) and inline images (skipped). Ligature code points are expanded (`ﬁ` → `fi`), soft hyphens and control characters dropped.
- **Layout:** text comes out in content-stream order. A new line starts when the baseline moves by more than 0.6 of the font size, a space when the gap to the previous glyph exceeds about 0.17 em; rotated text keeps its own line.

## What it does not do

- Encrypted files are `Error::Unsupported`.
- Text drawn as outlines or images (scans) is not text. There is no OCR.
- Reading order follows the content stream, so multi-column pages and plot labels can come out in a different order than `pdftotext` gives.

## Verification

Files from reportlab, fpdf, pdfTeX (Type 1, object streams, two columns), LibreOffice (TrueType subsets, ToUnicode), cairo, Ghostscript, qpdf (object streams, xref streams, QDF) and matplotlib (Type 3 and Type 42) were compared with `pdftotext`: the text matches word for word apart from spacing around superscripts and the ordering of plot labels. Small fixtures of several of them are in `tests/fixtures`; the corner cases (filters, encodings, damaged files, page trees) use hand-built files.
