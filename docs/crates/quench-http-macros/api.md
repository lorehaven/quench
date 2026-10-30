---
title: API
parent: quench-http-macros
grand_parent: Crates
nav_order: 1
---

All six are `#[proc_macro_attribute]` items in `quench-http-macros/src/lib.rs`.

## Route attributes

```rust
#[proc_macro_attribute] pub fn get(attr: TokenStream, item: TokenStream) -> TokenStream
#[proc_macro_attribute] pub fn post(attr: TokenStream, item: TokenStream) -> TokenStream
#[proc_macro_attribute] pub fn put(attr: TokenStream, item: TokenStream) -> TokenStream
#[proc_macro_attribute] pub fn delete(attr: TokenStream, item: TokenStream) -> TokenStream
#[proc_macro_attribute] pub fn patch(attr: TokenStream, item: TokenStream) -> TokenStream
```

Each calls `expand::route(METHOD, attr, item)` with `"GET"`, `"POST"`, `"PUT"`, `"DELETE"`, `"PATCH"`; they differ only in the HTTP method.

**Arguments**: exactly one string literal, the route pattern: `#[get("/users/{id}")]`. Anything else (no argument, a non-literal, extra tokens such as `name = "x"`) is a compile error from `syn::parse2::<LitStr>`. The pattern is **not** validated at compile time: pattern errors (unsupported constraint, unclosed brace) panic at runtime in `discover_routes()`. Pattern syntax: see [quench-http internals](../quench-http/internals.md#route-patterns).

**Target**: a free `fn` (`syn::ItemFn`), `async` or not. Any other item is a compile error. Parameters: each must be a typed parameter whose type implements `quench_http::extract::FromRequest`; the pattern (`Path(id): Path<u64>`, `mut x: T`) is ignored, only the type is used. A `self` receiver is rejected. Return type: anything implementing `quench_http::endpoint::IntoResponse` (checked by rustc on the expansion, not by the macro). Generic parameters are not handled by the macro (the generated struct and impl carry none); treat generic handlers as unsupported. *Unverified:* no test covers them. Other attributes and the function itself are kept unchanged.

**Effect**: the function stays callable under its own name; additionally a hidden struct implementing `Endpoint` is generated and registered in `quench_http::route::ROUTES` with the method and pattern. `discover_routes()` turns all registrations into a `Router`.

Example (verified by `quench-http/tests/route_macro.rs`):

```rust
#[derive(quench_serde::Deserialize)]
struct UserPath { id: String }

#[get("/users/{id}")]
async fn get_user(Path(p): Path<UserPath>, Inject(g): Inject<Greeter>) -> Result<String, HttpError> {
    Ok(format!("{}, {}", g.prefix, p.id))
}
```

## `#[injectable]`

```rust
#[proc_macro_attribute]
pub fn injectable(_attr: TokenStream, item: TokenStream) -> TokenStream
```

**Arguments**: none are accepted; the attribute argument tokens are ignored silently (`#[injectable(anything)]` compiles).

**Target**: a free `fn` (`async` or not) that **returns a type** (not `()`), optionally `Result<T, E>`:

* Every parameter must be of the form `Arc<Dep>` (any path whose last segment is `Arc`, e.g. `std::sync::Arc<Dep>`; the first type argument is `Dep`). Anything else (`Db`, `&Db`, bare `Arc`, `Arc<'static>`) is a compile error.
* The registered component type is the return type; for `Result<T, E>` (any path whose last segment is `Result` with a first type argument), it is `T`, and `E` must implement `ToString` (usually `Display`). An `Err` becomes `DiError::ConstructionFailed { component: type_name::<T>(), reason: e.to_string() }` at `ContainerBuilder::build()`.
* A non-`Result` return type is registered as written, including `Arc<X>` (the component type would then be `Arc<X>` itself; return a bare `X` instead).

Example:

```rust
#[injectable]
async fn build_user_db(db: Arc<Db>) -> UserDb { UserDb::init(db).await }

#[injectable]
fn build_cache() -> Result<Cache, std::io::Error> { Cache::open() }
```

Handlers then take `Inject<UserDb>`, other components `Arc<UserDb>`.
