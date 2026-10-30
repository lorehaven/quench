---
title: quench-test-macros
parent: Crates
has_children: true
permalink: /crates/quench-test-macros/
nav_order: 51
---

`quench-test-macros` is the proc-macro crate behind [quench-test](../quench-test/index.md). It provides the single attribute `#[test]`, re-exported as `quench_test::test`. Use it through `quench-test`: the generated code names `::quench_test::..`, so the crate that uses the attribute must depend on `quench-test` (as a dev-dependency).

## Facts

| | |
|---|---|
| Version | 0.1.1 |
| Edition | 2024 |
| Kind | `[lib] proc-macro = true` |
| Dependencies | `proc-macro2`, `quote`, `syn` (feature `full`) |
| Features | none |
| Public items | 1 attribute macro, `test` |

## Pages

* [The `test` attribute](attributes.md): arguments, accepted signatures, `#[ignore]`, `#[should_panic]`, generated code, compile errors.
* The harness it feeds: [quench-test](../quench-test/index.md).
