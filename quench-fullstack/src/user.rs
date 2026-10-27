//! The signed-in user, as a server function sees them.

use quench_serde::{Deserialize, Serialize};

/// Who made a call: what [`user`] returns inside a `#[server(auth)]` function.
///
/// ```ignore
/// #[server(auth)]
/// async fn my_notes() -> Result<Vec<Note>, ServerError> {
///     let me = user()?;
///     Ok(store().notes_of(&me.sub))
/// }
/// ```
///
/// With `auth`, a call without a valid session never reaches the function: it is answered 401,
/// which the browser sees as [`crate::ServerError::Unauthorized`]. The type exists on every target,
/// so a shared crate can name it; it is only filled in on the server.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct User {
    /// The account name.
    pub sub: String,
    /// The roles and permissions in the token, separated by spaces.
    pub scope: String,
    /// The services the token was issued for.
    pub audiences: Vec<String>,
    /// The session the token belongs to, if it has one.
    pub session: Option<String>,
}

impl User {
    /// Whether the scope contains `role` (an exact entry, such as `admin` or `notes:write`).
    #[must_use]
    pub fn has_role(&self, role: &str) -> bool {
        self.scope.split_whitespace().any(|r| r == role)
    }
}

/// The user whose call is being served. Use it inside a function marked `#[server(auth)]` (the
/// session is required) or `#[server(auth = "optional")]` (it may be missing).
///
/// # Errors
/// [`crate::ServerError::Unauthorized`] if there is no session, or if the function was not marked
/// with `auth`. In the browser it is always an error: the body of a server function does not run
/// there.
pub fn user() -> Result<User, crate::ServerError> {
    #[cfg(not(target_arch = "wasm32"))]
    return crate::server::current_user();
    #[cfg(target_arch = "wasm32")]
    return Err(crate::ServerError::internal(
        "user() only works on the server",
    ));
}
