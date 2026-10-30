---
title: API
parent: quench-core-macros
grand_parent: Crates
nav_order: 1
---

Two public items, both exported from `src/lib.rs` and re-exported by `quench-core` as `quench_core::async_trait` and `quench_core::Error`. The entry points only convert between `proc_macro` and `proc_macro2`; the expansion lives in plain functions (`async_trait::expand`, `error_derive::expand`) so it can be unit-tested.

## `#[async_trait]`

```rust
#[proc_macro_attribute]
pub fn async_trait(attr: TokenStream, item: TokenStream) -> TokenStream
```

Lets a trait (and its impls) declare `async fn`s while staying usable as `dyn Trait`: each `async fn` becomes a method returning a boxed future.

| Form | Effect |
|---|---|
| `#[async_trait]` | returned future is `Send` |
| `#[async_trait(?Send)]` | the `Send` bound is dropped |

Any other argument is a compile error. Apply it to a **trait** and to every **impl** of it (`impl Trait for T` blocks). Only methods that are `async` are rewritten; other items are untouched.

### Expansion

`async fn name(&self, a: &str) -> R` becomes

```rust
fn name<'life0, 'life1, 'async_trait>(&'life0 self, a: &'life1 str)
    -> ::core::pin::Pin<::std::boxed::Box<dyn ::core::future::Future<Output = R> + ::core::marker::Send + 'async_trait>>
where 'life0: 'async_trait, 'life1: 'async_trait, Self: 'async_trait
{ /* let __future = Box::pin(async move { ... }); __future */ }
```

Details taken from the code:

* `asyncness` is removed; the output type is `Pin<Box<dyn Future<Output = T> + Send + 'async_trait>>` (`()` when no return type). For a trait method without a body only the signature changes.
* Every elided or `'_` lifetime in the receiver and argument types is replaced by a fresh `'life0`, `'life1`, ...; each gets the bound `: 'async_trait`. Lifetimes the author declared also get `: 'async_trait`. Lifetime parameters are placed before type and const parameters.
* `Self: 'async_trait` is added to the `where` clause; type parameters get `'async_trait` as an inline bound (not in the `where` clause).
* For a **default method body in a trait** with `Send` futures: `&self` receivers add `Self: Sync`; `&mut self` and by-value receivers add `Self: Send`; no receiver adds nothing.
* The body is wrapped as `Box::pin(async move { original block })` bound to a typed local, so `?` and early `return` work inside.
* Functions without a receiver (`async fn build() -> Self`) are supported.

### Compile errors

| Condition | Message |
|---|---|
| attribute argument other than empty or `?Send` | `expected `#[async_trait]` or `#[async_trait(?Send)]`` |
| applied to anything but a trait or an impl | `#[async_trait] applies to a trait or an impl block` |

### Example (from the tests)

```rust
use quench_core::async_trait;

#[async_trait]
trait Store: Send + Sync {
    async fn get(&self, key: &str) -> Option<String>;
    async fn put(&self, key: &str, value: String);

    async fn get_or(&self, key: &str, fallback: &str) -> String {
        self.get(key).await.unwrap_or_else(|| fallback.to_string())
    }
}
```

Pitfalls: use `#[async_trait(?Send)]` on the trait **and** its impls consistently; traits only ever used generically can use native `async fn in trait` instead.

## `#[derive(Error)]`

```rust
#[proc_macro_derive(Error, attributes(error, from, source))]
pub fn derive_error(input: TokenStream) -> TokenStream
```

Implements `core::fmt::Display`, `std::error::Error` and (for `#[from]`) `From`. It works on structs and enums; unions are rejected. The type must implement `Debug` itself (derive it). Generics are carried over with their existing bounds; the macro adds none.

### Attributes

| Attribute | Where | Meaning |
|---|---|---|
| `#[error("text {field} {0}", args...)]` | each variant, or the struct | `Display` text, as `write!(f, "text", args...)`. Named fields are `{name}`; tuple fields are `{0}`, `{1}` (rewritten to `{_0}`, `{_1}`). Format specs work (`{0:?}`). |
| `#[error(transparent)]` | variant or struct with exactly one field | `Display` forwards to the field, and `source()` returns the field's own `source()`. |
| `#[from]` | the only field of a variant/struct | also generates `impl From<FieldType> for Type`; the field is the source. |
| `#[source]` | a field | `source()` returns this field. A field named `source` is treated the same without the attribute. |

Every variant (or the struct) **must** have an `#[error(...)]` attribute.

Argument rewriting: in the extra arguments after the format string, a `.name` or `.0` that begins an expression (at the start or after an operator) refers to the field: `#[error("{}", .items.join(", "))]`.

Path fields: a field whose type's last path segment is `PathBuf` or `Path` (also behind references) and that appears as a plain `{name}` placeholder (no format spec) without an explicit `name = ...` argument is printed with `.display()`, as `thiserror` does.

`source()`: for `transparent`, the inner error's `source()`; otherwise the field marked `#[from]`/`#[source]`/named `source` as `&(dyn Error + 'static)`; otherwise `None`. An empty enum gets `match *self {}` for `Display`.

### Compile errors

| Condition | Message |
|---|---|
| applied to a union | `#[derive(Error)] does not support unions` |
| no `#[error]` on a variant/struct | `missing #[error("..")] display message` |
| `#[error(...)]` is neither a string literal first nor `transparent` | `expected #[error("message", args..)] or #[error(transparent)]` |
| first token is a literal that is not a string | `the message must be a string literal` |
| `transparent` with not exactly one field | `#[error(transparent)] needs exactly one field` |
| `#[from]` on a variant with other than one field | `#[from] needs the variant to have exactly one field` |

### Example

```rust
use quench_core::Error;

#[derive(Debug, Error)]
enum LoadError {
    #[error("cannot read {path}: {source}")]
    Read { path: String, source: std::io::Error },
    #[error("bad value {0}")]
    Bad(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn read() -> Result<(), LoadError> {
    std::fs::read("x")?; // From<io::Error> via #[from]
    Ok(())
}
```
