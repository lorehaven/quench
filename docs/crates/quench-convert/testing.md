---
title: Testing
parent: quench-convert
grand_parent: Crates
nav_order: 5
---

`tests/convert.rs` (`harness = false`, `quench_test::main!()`, 745 lines) covers: CSV records and quoting, errors and leniency, values, writing round trips; XML tree, whitespace and writing, errors; AXML with a builder for synthetic binary manifests (UTF-8 and UTF-16 pools), real manifests, every value type, errors; GGUF with a builder for versions 1-3, every value type and errors. The doctests in `csv.rs` and `xml.rs` are the overview examples.

```bash
cargo test -p quench-convert
```

To test code that consumes these readers, build input text/bytes in the test; for AXML and GGUF the helper functions `build_axml` and `build_gguf` in the test file show the byte layout.
