---
title: Testing
parent: quench-core-macros
grand_parent: Crates
nav_order: 2
---

The crate has no `tests/` directory. Each expansion function (`async_trait::expand`, `error_derive::expand`) has `#[cfg(test)]` unit tests in its source file that feed token text in and assert on the generated token string (whitespace-normalised). The behavioural tests that compile and run the generated code live in `quench-core`: `tests/error_derive.rs` and `tests/async_trait.rs` (see [quench-core testing](../quench-core/testing.md)).

```bash
cargo test -p quench-core-macros
cargo test -p quench-core --test error_derive --test async_trait
```

`cargo test --offline -p quench-core-macros`: 27 unit tests pass; 2 doc examples are `ignore`d.
