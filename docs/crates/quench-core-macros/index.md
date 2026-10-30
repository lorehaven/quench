---
title: quench-core-macros
parent: Crates
nav_order: 13
has_children: true
permalink: /crates/quench-core-macros/
---

`quench-core-macros` is the procedural-macro crate behind two items re-exported by [quench-core](../quench-core/index.md): the attribute `#[async_trait]` and the derive `#[derive(Error)]`. Use them through `quench_core`; the crate is not meant to be depended on directly.

| | |
|---|---|
| Version | 0.1.0 |
| Edition | 2024 (workspace) |
| Crate type | `proc-macro = true` |
| Features | none |

```toml
[dependencies]
quench-core = { version = "0.1", registry = "ennor" }   # re-exports both macros
```

## Dependencies

| Dependency | Why |
|---|---|
| `proc-macro2` | token streams in the expansion functions (so they can be unit-tested) |
| `quote` | code generation |
| `syn` (feature `visit-mut`, workspace features `full`) | parsing items; `VisitMut` names elided lifetimes |

These are the only third-party crates allowed for macro crates ([dependency policy](../../guides/dependency-policy.md)).

## Replaces

`async-trait` and `thiserror`.

## Where to go next

* [API reference](api.md): both macros, their arguments, expansion and compile errors.
* [Testing](testing.md).
