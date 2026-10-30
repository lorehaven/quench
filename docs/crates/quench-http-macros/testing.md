---
title: Testing
parent: quench-http-macros
grand_parent: Crates
nav_order: 4
---

Unit tests are in `quench-http-macros/src/expand.rs` (`#[cfg(test)] mod tests`, plain `#[test]`, run with `cargo test -p quench-http-macros`). They call `route` and `injectable` on `quote!` token streams and assert on the normalised output text:

* a route keeps the function, registers `Method :: GET` and the pattern, emits one `__arg<N>` per parameter, awaits an async handler and does not await a plain one;
* bad route input (no pattern, not a function, `self`) yields `compile_error`;
* an injectable registers its dependencies, handles `std::sync::Arc<T>` and bare `Arc<T>`, and registers the `Ok` type of a `Result`;
* the six rejected `#[injectable]` shapes each produce the documented message;
* `arc_inner` / `result_inner` behaviour.

There are no `trybuild` compile-fail tests. The end-to-end behaviour (discovery, extraction, DI) is tested through `quench-http` (`tests/route_macro.rs` and the other targets listed in [quench-http testing](../quench-http/testing.md)).
