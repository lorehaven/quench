---
title: API - root, errors, user, wire
parent: quench-fullstack
grand_parent: Crates
nav_order: 2
---

Items available on **every** target (including `wasm32`) are marked *all*; items marked *server* exist only when `not(target_arch = "wasm32")`. The other API pages: [client](api-client.md), [server and testing](api-server.md), [pages, assets, `Ui`, `UiApp`](api-ui.md). The crate contains no `unsafe`.

## Crate root (`src/lib.rs`)

```rust
pub mod client;                                  // all
pub mod wire;                                    // all
#[cfg(not(target_arch = "wasm32"))] pub mod server;
#[cfg(not(target_arch = "wasm32"))] pub mod testing;

pub use error::{ErrorBody, FieldError, FormExt, INTERNAL_MESSAGE, ServerError};
pub use quench_fullstack_macros::server;         // the #[server] attribute
pub use user::{User, user};

#[cfg(not(target_arch = "wasm32"))] pub use app::{Ui, UiApp};
#[cfg(not(target_arch = "wasm32"))] pub use assets::{Asset, Assets, PREFIX, content_hash};
#[cfg(not(target_arch = "wasm32"))] pub use document::{
    Page, ROOT_ID, STATE_ID, content_security_policy, escape_json, new_nonce, render_document,
};

#[doc(hidden)] #[cfg(not(target_arch = "wasm32"))]
pub mod __private {
    pub use quench_core::{async_trait, submit};
    pub use quench_http::prelude::{Endpoint, FromRequest, Request, Response};
}
```

`extern crate self as quench_fullstack;` lets the `#[server]` expansion (which names the crate by path) work inside the crate too. `__private` is what the expansion refers to; not for direct use. Modules `app`, `assets`, `dev`, `document`, `error`, `user` are private; only the re-exports above are public.

## `wire` (all, `src/wire.rs`)

What the browser and the server agree on for a server function call.

```rust
pub const CONTENT_TYPE: &str = "application/json";
pub const RPC_HEADER: &str = "x-quench-rpc";
pub const RPC_VALUE: &str = "1";
pub const MAX_BODY: usize = 1 << 20;
```

| Constant | Meaning |
|---|---|
| `CONTENT_TYPE` | Every call is a `POST` of a JSON object (arguments by name) with this content type. |
| `RPC_HEADER` | Header every call carries. A page on another origin cannot add a custom header to a cross-origin request without a CORS preflight the server will not grant; a request without it is refused (CSRF protection for cookie sessions). |
| `RPC_VALUE` | The value of `RPC_HEADER`. |
| `MAX_BODY` | Largest request body a server function accepts: 1 048 576 bytes. |

## Errors (all, `src/error.rs`)

Full semantics in [Errors](errors.md).

```rust
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct FieldError { pub field: String, pub message: String }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerError {
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Validation { message: String, fields: Vec<FieldError> },
    Internal(String),
    Network(String),
    BadResponse(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct ErrorBody {
    pub kind: String,
    pub message: String,
    #[serde(default)] pub fields: Vec<FieldError>,
}

pub const INTERNAL_MESSAGE: &str = "internal server error";

impl ServerError {
    #[must_use] pub fn unauthorized(message: impl Into<String>) -> Self;
    #[must_use] pub fn forbidden(message: impl Into<String>) -> Self;
    #[must_use] pub fn not_found(message: impl Into<String>) -> Self;
    #[must_use] pub fn invalid(field: &str, message: &str) -> Self;
    #[must_use] pub fn internal(cause: impl fmt::Display) -> Self;
    #[must_use] pub const fn status(&self) -> u16;
    #[must_use] pub const fn kind(&self) -> &'static str;
    #[must_use] pub fn message(&self) -> &str;
    #[must_use] pub fn fields(&self) -> &[FieldError];
    #[must_use] pub fn to_body(&self) -> ErrorBody;
    #[must_use] pub fn from_response(status: u16, body: &str) -> Self;
}
impl fmt::Display for ServerError {}       // "{kind}: {message}"
impl std::error::Error for ServerError {}

pub trait FormExt {
    fn finish<T>(&self, result: Result<T, ServerError>) -> Option<T>;
}
impl FormExt for quench_ui::form::Form {}
```

| Item | Behaviour |
|---|---|
| `unauthorized`, `forbidden`, `not_found` | Variant with the message. |
| `invalid(field, message)` | `Validation` with `message = "{field}: {message}"` and one `FieldError { field, message }`. |
| `internal(cause)` | `Internal(cause.to_string())`. The cause is logged on the server; the browser receives `INTERNAL_MESSAGE`. |
| `status()` | 401, 403, 404, 422, 500; `Network` and `BadResponse` give 502 (they never travel). |
| `kind()` | `"unauthorized"`, `"forbidden"`, `"not_found"`, `"validation"`, `"internal"`, `"network"`, `"bad_response"`. |
| `message()` | The message of any variant (for `Validation` the overall message). |
| `fields()` | The `FieldError`s of a `Validation`, empty otherwise. |
| `to_body()` | `ErrorBody { kind, message, fields }`; an `Internal` message is replaced by `INTERNAL_MESSAGE`. |
| `from_response(status, body)` | Parses `body` as `ErrorBody` (failure is ignored). The kind decides first, the status second: `validation`/422, `unauthorized`/401, `forbidden`/403, `not_found`/404, `internal`/500, anything else `BadResponse`. Message: the body's non-empty message, else `"the server answered {status}"`. For `Validation` the fields come from the body. |
| `FormExt::finish` | Calls `set_submitting(false)`. `Ok(v)`: `clear_errors()`, returns `Some(v)`. `Validation { message, fields }`: `apply_errors(&message, &[(field, message)...])`, returns `None`. Other error: `apply_errors(other.message(), &[])`, returns `None`. |

## `User` and `user()` (all, `src/user.rs`)

```rust
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct User {
    pub sub: String,            // the account name
    pub scope: String,          // roles and permissions, separated by spaces
    pub audiences: Vec<String>, // services the token was issued for
    pub session: Option<String>,
}
impl User { #[must_use] pub fn has_role(&self, role: &str) -> bool; }

pub fn user() -> Result<User, ServerError>;
```

* `has_role`: `scope` split on whitespace contains exactly `role`.
* `user()`: server: the user of the call being served (set by `server::with_user` for `#[server(auth)]` and `#[server(auth = "optional")]` functions); `Err(ServerError::Unauthorized("sign in required"))` if there is none or the function was not marked with `auth`. Browser (`wasm32`): always `Err(ServerError::Internal("user() only works on the server"))`.
* `User` exists on every target so a shared crate can name it. Server-only extras: `User::from_claims` and `impl FromRequest for User` in [API: server](api-server.md).

## The `#[server]` attribute

`pub use quench_fullstack_macros::server;` See [quench-fullstack-macros](../quench-fullstack-macros/index.md).
