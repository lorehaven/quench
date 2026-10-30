---
title: Testing
parent: quench-schema
grand_parent: Crates
nav_order: 6
---

`harness = false`; `src/lib.rs` ends with `mod tests;` and `quench_test::main!();`. The tests (`src/tests.rs`, about 460 lines) cover each keyword family, messages and instance paths, `$ref` forms, combinators, and the `SchemaError` cases. Run with `cargo test -p quench-schema`. The doctest in `src/lib.rs` is the overview example.
