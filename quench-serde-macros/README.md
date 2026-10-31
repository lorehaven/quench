# quench-serde-macros
Derive macros for `quench-serde`: `#[derive(Serialize)]` and `#[derive(Deserialize)]`.

## What it is
`quench-serde-macros` provides `#[derive(Serialize)]` and `#[derive(Deserialize)]` for `quench-serde`, which re-exports them. The helper attribute is called `serde`, so types annotated for `serde` need no changes for the supported subset.

The generated code names `::quench_serde::...`, so use the derives through `quench-serde` and name that dependency `quench-serde`. Anything outside the supported attributes is a compile error that names what is supported, never a silent no-op.

## Use cases
- Make a struct or enum convertible to and from a `quench-serde` `Value`.
- Rename fields or variants with `rename` and `rename_all`.
- Reject unknown keys with `deny_unknown_fields`.
- Fill absent keys with `default`, or omit fields with `skip` and `skip_serializing_if`.
- Model tagged enums with `tag`, or `untagged`, and merge keys with `flatten`.

## Example
```rust
use quench_serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Point {
    x_pos: i32,
    #[serde(default)]
    y_pos: i32,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum Shape {
    Dot,
    Line { length: u32 },
}
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-serde-macros/
