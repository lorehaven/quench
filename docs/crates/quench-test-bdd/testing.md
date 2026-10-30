---
title: Testing
parent: quench-test-bdd
grand_parent: Crates
nav_order: 6
---

`tests/parsing.rs` covers the Gherkin parser, outline expansion, cucumber expressions and tag expressions; `tests/features.rs` runs real feature files written to a temporary directory against a `Calc` world: backgrounds, outlines, rules, doc strings and tables, `{string}` unescaping, `@serial`, tag and name filters, `fail_fast`, `concurrency`, report files, ambiguous/undefined steps, unparsable files. Both are `harness = false` and use `quench_test::main!()`. The macro crate has unit tests of its expansion.

```bash
cargo test -p quench-test-bdd
```
