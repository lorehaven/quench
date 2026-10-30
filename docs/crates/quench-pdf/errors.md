---
title: Errors
parent: quench-pdf
nav_order: 3
---

`quench_pdf::Error` has two variants.

| Variant | Display | When |
|---|---|---|
| `Malformed(String)` | `malformed PDF: {m}` | no `%PDF-` in the first 4 096 bytes (`not a PDF: no %PDF- header`); no usable catalog after a rebuild (`no document catalog`); a rebuild that finds no objects (`no objects found`); syntax errors in objects and xref data (`no startxref`, `bad startxref`, `no trailer`, `bad /First`, `object stream is not a stream`, `object N missing from its stream`, ...); Flate failures inside `stream_data` are swallowed (the stream is just missing) |
| `Unsupported(String)` | `unsupported PDF: {m}` | `encrypted PDF`; `stream filter NAME` (only surfaces inside the decoder; see [Reading rules](internals.md)) |

Errors from `extract_pages` / `extract_text` come only from `Document::load`. Anything that goes wrong while interpreting a page (a missing font, a bad content stream, an undecodable stream) is skipped and never an error: the page text is just shorter.
