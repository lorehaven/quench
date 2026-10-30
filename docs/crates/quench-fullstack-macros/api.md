---
title: API
parent: quench-fullstack-macros
grand_parent: Crates
nav_order: 2
---

## `#[server]`

```rust
#[proc_macro_attribute]
pub fn server(attr: TokenStream, item: TokenStream) -> TokenStream
```

Applies to an `async fn` item. Re-exported as `quench_fullstack::server`.

### Arguments

Parsed with `syn::meta::parser`; arguments are separated by commas.

| Argument | Form | Meaning |
|---|---|---|
| `path` | `path = "/..."` | The route (`POST`). Default `/_fn/<function name>`. Must start with `/` and contain no whitespace. |
| `auth` | `auth` or `auth = "required"` | A valid session is required; the call is answered before the function runs when there is none. |
| `auth` | `auth = "optional"` | The user is looked up and `user()` returns it if there is one. |
| (none) | | Public: the user is not looked up. |

If `auth` appears twice, the last wins. Any other argument name is an error.

### Constraints on the function

* Must be `async`.
* No generic parameters (type, lifetime or const) and no `where` clause.
* No `self` receiver.
* Every parameter pattern must be a plain identifier (`title: String`; `mut x: T` is also an identifier pattern).
* The return type must be a path whose last segment is `Result` with angle-bracketed arguments; the first type argument is taken as the success type `T`. (The error type is not checked by the macro; the runtime requires `ServerError`.)

### Compile errors

All point at the offending tokens (`syn::Error::to_compile_error`).

| Condition | Message |
|---|---|
| Item is not a function | the `syn` parse error for `ItemFn` |
| Bad `path` | `the path must start with `/` and have no spaces` |
| Bad `auth` value | ``` `auth` is `"required"` or `"optional"` ``` |
| Unknown argument | ``` unknown option; they are `path = "/..."` and `auth` or `auth = "optional"` ``` |
| Not async | ``` a server function must be `async` ``` |
| Generics / where clause | `a server function cannot be generic` |
| Return type not `Result<..>` (none, not a path, last segment not `Result`, no angle brackets, no type argument) | ``` a server function must return `Result<T, ServerError>` ``` |
| `self` parameter | ``` a server function cannot take `self` ``` |
| Pattern parameter | ``` parameters must be plain names, such as `title: String` ``` |

Order of checks: item parse, attribute arguments, async, generics, return type, parameters.

### Requirements on the calling crate

The expansion names `::quench_serde` (derives) and `::quench_fullstack` (runtime) by path, so the crate that contains server functions must depend on both.

### Generated items

See [Expansion](internals.md). Names the expansion introduces inside the function's scope: `__QuenchArgs` (argument struct), `__QuenchEndpoint`, `__user`, `__args`.
