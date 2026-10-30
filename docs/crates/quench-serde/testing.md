---
title: Testing
parent: quench-serde
grand_parent: Crates
nav_order: 6
---

* Harness: `harness = false` with `quench_test::main!()` at the end of `lib.rs` (under `cfg(test)`) and of `tests/derive.rs`.
* Unit tests are in each source file (counts of `#[quench_test::test]`: `json` 15, `map` 6, `value` 17, `traits` 18, `toml` 31, `urlencoded` 12, `error` 4, `macros` 6, `private` 7, `yaml/mod` 5, `yaml/parse` 14, `yaml/write` 9, `yaml/resolve` 4).
* `tests/derive.rs` exercises the derives on the shapes the workspace uses: plain structs in field order, rename/alias/`deny_unknown_fields`, defaults and skips, container `default`, `flatten`, tuple structs, external/internal/untagged enums, `rename_all` on enums, generics and lifetimes, raw identifiers, error paths, and `json!` with derived types.
* Run: `cargo test -p quench-serde` and `cargo test -p quench-serde --features yaml` (the YAML tests are compiled only with the feature).

Testing code that uses it: compare `Value`s (`assert_eq!(json::parse(a)?, json::parse(b)?)` ignores object key order) or `json!` literals.

## Result of the last run

`cargo test --offline -p quench-serde` (default features): 148 unit tests and 17 in `tests/derive.rs` pass, plus 1 doctest (`json!`). With default features the `yaml` tests are not compiled.
