---
title: Testing
parent: quench-core
grand_parent: Crates
nav_order: 5
---

`quench-core` sets `harness = false` for its library and for every integration test, and declares `autotests = false`. Tests use `quench-test` (dev-dependency): `lib.rs` ends with `#[cfg(test)] quench_test::main!();` and tests are annotated `#[quench_test::test]`.

## Layout

* Unit tests sit in `#[cfg(test)] mod tests` blocks at the bottom of each source file (the source files contain, per file: `base64` 8, `fs` 10, `env` 7, `error` 9, `process` 1, `dotenv` 8, `tmp` 4, `bytes` 8, `future` 6, `hex` 4, `ignore` 6, `shell` 7, `random` 3, `stream` 12, `uuid` 8, `version` 11, `time` 17, `base32` 4, `map` 8, `date` 5 `#[quench_test::test]` annotations).
* Integration tests, registered as `[[test]]` with `harness = false`:
  * `tests/registry.rs`: `submit!` used from outside the crate; registration before `main`; newest-first order.
  * `tests/error_derive.rs`: `#[derive(Error)]` display formats, sources, `#[from]` with `?`, structs, generics and empty enums, boxed use and use with the catch-all `Error`, `PathBuf` fields.
  * `tests/async_trait.rs`: trait objects, `Send` futures, `&mut self` and by-value receivers, many borrows and generics, `?Send`, `?` inside bodies.
* Because the custom harness does not set `cfg(test)`, the integration test files carry `#![allow(clippy::wildcard_imports)]`.

## Running

```bash
cargo test -p quench-core
```

## Testing code built on it

* Use `tmp::TempDir` for scratch directories (removed on drop).
* Use `future::block_on` to drive an async function in a plain test.
* If a test must change the environment, use variable names unique to that test (see [internals](internals.md)).

## Result of the last run

`cargo test --offline -p quench-core`: 146 unit tests, 7 (`async_trait`), 7 (`error_derive`) and 2 (`registry`) integration tests pass; the doctest in `registry.rs` is `ignore`d.
