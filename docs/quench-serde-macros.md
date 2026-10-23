# Quench Serde Macros

`quench-serde-macros` is the proc-macro crate behind `#[derive(Serialize, Deserialize)]`. Use it through `quench-serde`, which re-exports the derives next to the traits (the macro and the trait share a name; they live in different namespaces). Attribute reference: [Quench Serde](./quench-serde.md#derives).

The expansion is plain functions over `syn` types (`model` parses the item once, `ser` and `de` generate code), so the unit tests run them directly on source snippets; behaviour is tested end to end in `quench-serde/tests/derive.rs`. Generated code refers to the runtime only through `::quench_serde::...` paths, plus the hidden `quench_serde::__private` helpers.

[Home](../README.md)
