---
title: Testing
parent: quench-pdf
nav_order: 5
---

* `tests/pdf.rs` (`harness = false`): real files in `tests/fixtures` (`cairo.pdf`, `fpdf.pdf`, `lo.pdf`, `lzw_libtiff.pdf`, `mpl3.pdf`) compared with expected text, plus hand-built PDFs (a tiny writer `build`/`stream`/`font` helpers) for: spacing and line breaks, fonts and encodings, every stream filter and predictor, form XObjects and inline images, page trees and inheritance, damaged files (rebuilt or refused), every object syntax, larger streams through every filter path, and a damaged xref-stream file rebuilt from its object streams.
* Run: `cargo test -p quench-pdf`.
* *Unverified:* the old page lists producers (reportlab, pdfTeX, LibreOffice, Ghostscript, qpdf, matplotlib) compared with `pdftotext`; only the five fixtures above are in the repository.
* To test code that calls the crate, build a minimal PDF string: `%PDF-1.4`, a catalog, a pages node, a page with a `Contents` stream such as `BT /F1 12 Tf 10 10 Td (Hello) Tj ET`, an `/F1` font `<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>`, and no xref table: the reader rebuilds it by scanning for `n g obj`, takes `/Root` from a `trailer` dictionary (or, failing that, the first object with `/Type /Catalog`). A file of this shape returns `["Hello"]` (checked while writing this page).
