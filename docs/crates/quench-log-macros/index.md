---
title: quench-log-macros
parent: Crates
nav_order: 30
has_children: true
permalink: /crates/quench-log-macros/
---

`quench-log-macros` holds the `#[instrument]` attribute for [quench-log](../quench-log/index.md), which re-exports it as `quench_log::instrument`. Use it through that crate.

| | |
|---|---|
| Version | 0.1.1 |
| Edition | 2024 (workspace) |
| Crate type | `proc-macro = true` |
| Dependencies | `proc-macro2`, `quote`, `syn` (workspace) |

The expansion lives in `instrument::expand` (a plain function with unit tests). The generated code names `::quench_log::span!`, `::quench_log::Level`, `::quench_log::Instrument`, so the caller must depend on `quench-log` under that name. See [API](api.md).
