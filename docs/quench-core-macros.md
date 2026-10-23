# Quench Core Macros

`quench-core-macros` holds the proc macros re-exported by `quench-core`: use them as `quench_core::async_trait` and `quench_core::Error`.

- **`#[async_trait]` / `#[async_trait(?Send)]`** — on a trait and its impls, rewrites each `async fn` into a method returning a boxed future (`Send` unless `?Send`), so the trait stays usable as `dyn Trait`. Every elided lifetime in the arguments is given a name, and type parameters get the `'async_trait` bound inline (a separate `where` clause would trip clippy's `multiple_bound_locations` in user code). Default method bodies get a `Self: Sync` (for `&self`) or `Self: Send` bound.
- **`#[derive(Error)]`** — `Display` and `std::error::Error` from `#[error("..")]`, and `From` from `#[from]`. In the message `{name}`, `{0}` and inline constants work; extra format arguments may use `.name` / `.0` for a field (`#[error("{}", .items.join(", "))]`) or the field's bare name. `#[error(transparent)]` forwards `Display` and `source`; `#[from]` and `#[source]` (or a field named `source`) mark the source error. Works on structs, enums and generics; an empty enum is allowed.

The expansion code is plain functions over `syn` types, unit-tested directly; behaviour is tested through `quench-core/tests/async_trait.rs` and `quench-core/tests/error_derive.rs`.

[Home](../README.md)
