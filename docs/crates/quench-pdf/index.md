---
title: quench-pdf
parent: Crates
nav_order: 33
has_children: true
permalink: /crates/quench-pdf/
---

`quench-pdf` (crate `quench_pdf`) is a PDF **reader that extracts text** page by page. It depends only on [quench-archive](../quench-archive/index.md) (for Flate) and replaces `pdf-extract` and `lopdf` in the estate. It does not write, render, edit or decrypt PDFs.

* Version: `0.1.2`. Edition 2024. No features. `unsafe_code = "deny"`; none present.
* Entry points: `extract_pages(&[u8]) -> Result<Vec<String>>`, `extract_text(&[u8]) -> Result<String>`, and `Document` for structure access.
* Reads cross-reference tables and streams (rebuilding by scanning when damaged), object streams, the page tree, content streams including form XObjects, simple and composite (CID) fonts via `ToUnicode`, base encodings and `Differences`.
* Rejects encrypted files with `Error::Unsupported`. Text drawn as outlines or images (scans) is not extracted; there is no OCR.
* `Document` is **not** `Send` or `Sync` (it holds `RefCell` and `Rc`): use it on one thread; `extract_*` are plain functions and can be called from any thread.

## Install

```toml
[dependencies]
quench-pdf = { version = "0.1", registry = "ennor" }   # in the workspace: { workspace = true }
```

## Dependencies

| Crate | Why |
|---|---|
| `quench-archive` | `deflate::decompress` for `FlateDecode` |
| `quench-test` (dev) | test harness |

## Where to go next

* [Usage](usage.md)
* [API reference](api.md)
* [Reading rules](internals.md): what is read, filters, fonts, layout rules, limits.
* [Errors](errors.md)
* [Testing](testing.md)
* Guide: [Working with files and formats](../../guides/files-and-formats.md).

## Minimal example

```rust
let bytes = std::fs::read("report.pdf")?;
for (number, text) in quench_pdf::extract_pages(&bytes)?.iter().enumerate() {
    println!("--- page {} ---\n{text}", number + 1);
}
# Ok::<(), Box<dyn std::error::Error>>(())
```
