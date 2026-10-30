---
title: Errors
parent: quench-style
nav_order: 4
---

* `ftl::FtlError { line, message }` (`Display`: `line {line}: {message}`, `line` 1-based) from `ftl::parse`. Messages: `a comment is `#`, `##` or `###` followed by a space`, `expected a message, a term or a comment`, `expected an identifier`, `expected `=` after the identifier`, `unclosed `{``, and others for malformed placeables and attributes (see `src/ftl.rs`).
* `i18n` functions return `quench_core::Result`: I/O errors from `std::fs`, `missing locale file: <path>` and `Failed to parse FTL: <path>` (with the `FtlError` as the cause).
* `generate_translations_js` swallows `i18n` errors and yields `window.qTranslations = ;` (an empty object literal is *not* produced; a missing `i18n` directory does produce `window.qTranslations = {};`).
* `assets::write_assets` does not return errors; it prints `ERROR: failed to ...` lines to stderr.
* `Color::hex` / `Palette::new` panic (or fail the build in a `const`) on text that is not `#rrggbb`.
* `CssRule` and the stylesheet functions cannot fail.
