//! Serves [`quench_ui`] apps from [`quench_http`] and lets them call server code.
//!
//! The page is rendered on the server, the browser hydrates it with the same code, and everything
//! goes out with content-hashed asset names and a strict Content-Security-Policy. A function marked
//! [`server`] is an ordinary `async fn` to the code that calls it, in the browser or on the
//! server.
//!
//! ```ignore
//! // The crate the server and the browser both build.
//! #[server(auth)]
//! async fn add_note(text: String) -> Result<Note, ServerError> {
//!     if text.trim().is_empty() {
//!         return Err(ServerError::invalid("text", "write something"));
//!     }
//!     Ok(store().add(&user()?.sub, text))
//! }
//!
//! // In a component, in the browser:
//! spawn(async move { match add_note(text).await { Ok(note) => ..., Err(e) => ... } });
//! ```
//!
//! What a page contains, what the browser does with it, how a call travels and how sessions work
//! is described in `docs/quench-fullstack.md`.

// The `#[server]` expansion names this crate by path; make that work inside it too.
extern crate self as quench_fullstack;

pub mod client;
mod error;
mod user;
pub mod wire;

#[cfg(not(target_arch = "wasm32"))]
mod app;
#[cfg(not(target_arch = "wasm32"))]
mod assets;
#[cfg(not(target_arch = "wasm32"))]
mod dev;
#[cfg(not(target_arch = "wasm32"))]
mod document;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
#[cfg(not(target_arch = "wasm32"))]
pub mod testing;

pub use error::{ErrorBody, FieldError, FormExt, INTERNAL_MESSAGE, ServerError};
pub use quench_fullstack_macros::server;
pub use user::{User, user};

#[cfg(not(target_arch = "wasm32"))]
pub use app::{Ui, UiApp};
#[cfg(not(target_arch = "wasm32"))]
pub use assets::{Asset, Assets, PREFIX, content_hash};
#[cfg(not(target_arch = "wasm32"))]
pub use document::{
    Page, ROOT_ID, STATE_ID, content_security_policy, escape_json, new_nonce, render_document,
};

/// What the `#[server]` expansion refers to. Not for direct use.
#[doc(hidden)]
#[cfg(not(target_arch = "wasm32"))]
pub mod __private {
    pub use quench_core::{async_trait, submit};
    pub use quench_http::prelude::{Endpoint, FromRequest, Request, Response};
}

#[cfg(test)]
quench_test::main!();
