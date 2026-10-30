---
title: quench-test-bdd-macros
parent: Crates
has_children: true
permalink: /crates/quench-test-bdd-macros/
nav_order: 50
---

`quench-test-bdd-macros` is the proc-macro crate behind [quench-test-bdd](../quench-test-bdd/index.md): the step attributes `#[given]`, `#[when]`, `#[then]` and the derive `World`. Use them through `quench-test-bdd`, which re-exports them; the generated code names `::quench_test_bdd::..`.

| | |
|---|---|
| Version | 0.1.0 |
| Edition | 2024 |
| Kind | `[lib] proc-macro = true` |
| Dependencies | `proc-macro2`, `quote`, `syn` (feature `full`) |
| Public items | 3 attribute macros (`given`, `when`, `then`), 1 derive (`World`) |

See [Attributes and derive](attributes.md).
