---
title: Testing
parent: quench-html
nav_order: 5
---

* `tests/html.rs` (`required-features = ["parse"]`, `harness = false`): tree skeleton, implicit closing, tables, attributes, entities, raw-text elements, comments and doctypes, every selector form, navigation and serialisation.
* `tests/markdown.rs` (`required-features = ["markdown"]`): runs every example of `tests/fixtures/commonmark-spec.json` and asserts there are exactly **652** and that all pass (CommonMark 0.31).
* Doctests in `lib.rs` and `markdown/mod.rs`.
* Run: `cargo test -p quench-html` (add `--no-default-features --features parse` to test one feature).
* *Unverified here:* the old page's comparisons with `pulldown-cmark` on 200 000 random inputs and with `scraper` on 400 HTML files are not part of the repository's tests.
