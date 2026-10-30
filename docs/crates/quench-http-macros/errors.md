---
title: Compile errors
parent: quench-http-macros
grand_parent: Crates
nav_order: 2
---

Every macro error is produced by `syn::Error::to_compile_error()` and points at the offending tokens.

## Route attributes

| Cause | Message |
|---|---|
| Missing, non-literal or extra attribute tokens | `syn`'s parse error for `LitStr` (for example `unexpected end of input, expected string literal`) |
| Item is not a function (struct, impl, ...) | `syn`'s parse error for `ItemFn` |
| A parameter is `self`, `&self`, `&mut self` | ``route handlers can't take `self` `` |

Errors that come from rustc on the expansion, not from the macro: a parameter type that does not implement `FromRequest`; a return type that does not implement `IntoResponse`; a function name that collides with another `__quench_route_<name>` struct in the same module.

## `#[injectable]`

| Cause | Message |
|---|---|
| Item is not a function | `syn`'s parse error for `ItemFn` |
| No return type | ``#[injectable] functions must return a type (optionally `Result<T, E>`)`` |
| `self` receiver | ``#[injectable] functions can't take `self` `` |
| A parameter whose type is not `Arc<Dep>` (`Db`, `&Db`, `Arc`, `Arc<'static>`) | ``#[injectable] constructor parameters must be `Arc<Dependency>` - that's the only dependency shape the container resolves`` |

Runtime, not compile time: a missing dependency and a cycle are `DiError`s from `ContainerBuilder::build()`; see [quench-http errors](../quench-http/errors.md).
