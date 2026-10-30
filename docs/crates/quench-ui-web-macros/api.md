---
title: Reference
parent: quench-ui-web-macros
grand_parent: Crates
nav_order: 1
---

The crate has one public item (`src/lib.rs`) and a private `expand` module (`src/expand.rs`) holding the logic as plain `proc_macro2` functions, so it can be unit tested.

## `#[plugin("name")]`

```rust
#[proc_macro_attribute]
pub fn plugin(attr: TokenStream, item: TokenStream) -> TokenStream
```

Turns a trait of `async fn` declarations into a client for the plugin of that name.

### Argument

Exactly one string literal: the plugin name. It must start with an ASCII lowercase letter and contain only ASCII lowercase letters, digits and `_` (as written in the Kotlin class). Anything else is a compile error (see below).

### Input

An item that parses as a `trait` (`syn::ItemTrait`). Its attributes, visibility and identifier are kept. Items inside it must all be `async fn` declarations without a body. Trait generics, supertraits and `where` clauses are not checked and are **dropped** from the expansion (no error).

### Expansion

For

```rust
#[quench_ui_web::plugin("echo")]
pub trait Echo {
    async fn say(text: String, times: u32) -> Result<String, BridgeError>;
    async fn device() -> Result<Device, BridgeError>;
}
```

the macro emits (shown simplified; the tests assert these fragments):

```rust
#[derive(Clone, Copy, Debug)]
pub struct Echo;

impl Echo {
    /// The plugin's name on the host side.
    pub const NAME: &'static str = "echo";

    #[allow(clippy::future_not_send)] // the browser runs one thread
    pub async fn say(text: String, times: u32) -> ::std::result::Result<String, ::quench_ui_web::BridgeError> {
        ::quench_ui_web::bridge::call("echo", "say", &::quench_ui_web::__private::json!({ "text": text, "times": times })).await
    }
    pub async fn device() -> ::std::result::Result<Device, ::quench_ui_web::BridgeError> {
        ::quench_ui_web::bridge::call("echo", "device", &::quench_ui_web::__private::json!({})).await
    }
}
```

Details:

* The trait becomes a **unit struct** with the same name, visibility and attributes (doc comments stay on the struct); `#[derive(Clone, Copy, Debug)]` is added. Do not put a `Clone`/`Copy`/`Debug` derive on the trait.
* `NAME` has the trait's visibility.
* Each method becomes an **associated function** (no `self`) with the declared name, parameters and visibility of the trait; its attributes (doc comments) are kept. The return type is rewritten to `::std::result::Result<T, ::quench_ui_web::BridgeError>`; the `BridgeError` you wrote is only matched by its last path segment and then replaced, so the name is never resolved in your module.
* The call is `bridge::call(plugin_name, method_name, &args).await`. The wire name of the method is the Rust function name; the JSON keys of `args` are the parameter names, in declaration order. A method with no parameters sends `{}`.
* Argument types must implement `quench_serde::Serialize`; `T` must implement `quench_serde::Deserialize` (`bridge::call`'s bounds). `Option<U>` arguments serialise as `null`.
* Each parameter pattern must be a plain identifier (`mut x: T` is accepted; `mut` is dropped in the expansion).
* The whole result is an ordinary `async fn`: it is `!Send` on the browser's single thread.

### Return type rule

`ok_type` accepts a path type whose last segment is `Result` with angle-bracketed arguments consisting of exactly two *type* arguments, the second being a path whose last segment is `BridgeError` (so `Result<T, BridgeError>`, `std::result::Result<T, quench_ui_web::BridgeError>` and `Result<(), BridgeError>` work). Type aliases and `Result<T>` do not.

### Compile errors

Each is a `compile_error!` at the offending tokens' span; the quoted text is the message.

| Cause | Message |
|---|---|
| attribute is not a string literal (`#[plugin]`, `#[plugin(echo)]`) | `give the plugin's name: `#[plugin("echo")]`` |
| name not lowercase-letter-first / has other characters (`"Echo"`, `"1echo"`, `"my-echo"`, `""`) | `a plugin name is lowercase letters, digits and `_`, starting with a letter` |
| item is not a trait (`struct S;`) | the `syn` parse error for a trait |
| a trait item that is not a method (`const X: u8;`, type, macro) | `a plugin trait has only `async fn` declarations` |
| method not `async` | `a plugin method is an `async fn`` |
| method has a default body | `a plugin method has no body: the host runs it` |
| method has generic parameters | `a plugin method cannot be generic` |
| return type missing or not `Result<T, BridgeError>` | `a plugin method returns `Result<T, BridgeError>`` |
| `self`/`&self` receiver | `a plugin method has no `self`: call it as `Plugin::method(..)`` |
| parameter pattern is not an identifier (`(a, b): (u8, u8)`) | `name each argument: it travels by name` |

Checks run in this order per method: async, body, generics, return type, parameters. The first error in the trait stops the expansion.

### Behaviour not checked

* A method with lifetime or const generics: only `generics.params` being non-empty is tested, so those are errors as well ("cannot be generic").
* Duplicate method names or parameter names are not detected here (the compiler reports them).
* Raw identifiers: the key is built with `Ident::to_string`, which for `r#type` gives `r#type` including the prefix (suspected bug; see the report).
* There is no way to rename a method or a parameter on the wire.
