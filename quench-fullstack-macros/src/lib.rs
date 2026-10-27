//! `#[server]`: an `async fn` the browser calls like any other and the server runs.
//!
//! See `quench-fullstack` for what it expands to and what it asks of the function.

mod expand;

use proc_macro::TokenStream;

/// Turns an `async fn` into a server function.
///
/// ```ignore
/// #[server]                                   // POST /_fn/list_notes, anyone may call it
/// async fn list_notes(owner: String) -> Result<Vec<Note>, ServerError> { /* server only */ }
///
/// #[server(auth, path = "/api/notes/add")]    // a valid session is required
/// async fn add_note(text: String) -> Result<Note, ServerError> {
///     let me = user()?;
///     /* ... */
/// }
/// ```
///
/// On the server the function is unchanged and a `POST` route is registered for it. In the browser
/// (`wasm32`) the body is replaced by a call to that route, so `add_note("hi".into()).await` works
/// in both places, with one signature. Options: `path = "/..."`, `auth` (answer 401 before the
/// function runs unless there is a session) and `auth = "optional"` (look the user up if there is
/// one). Inside, `user()` returns the signed-in user.
#[proc_macro_attribute]
pub fn server(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand::server(attr.into(), item.into()).into()
}
