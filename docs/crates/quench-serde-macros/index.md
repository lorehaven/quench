---
title: quench-serde-macros
parent: Crates
nav_order: 43
has_children: true
permalink: /crates/quench-serde-macros/
---

`quench-serde-macros` provides `#[derive(Serialize)]` and `#[derive(Deserialize)]` for [quench-serde](../quench-serde/index.md), which re-exports them. The helper attribute is called `serde`, so types annotated for `serde` need no changes for the supported subset.

| | |
|---|---|
| Version | 0.1.0 |
| Edition | 2024 (workspace) |
| Crate type | `proc-macro = true` |
| Dependencies | `proc-macro2`, `quote`, `syn` (workspace, feature `full`) |

Use it through `quench-serde` (the generated code names `::quench_serde::...`, so the dependency must be called `quench-serde`):

```rust
use quench_serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Point { x: i32, y: i32 }
```

See [API](api.md) for every attribute and the generated shapes, and [Testing](testing.md).
